use std::time::Duration;

use tauri::State;

use super::probe::best_eligible_profile;
use super::{
    emit_connection, exclusions, mark_faulted, remove_config, stop_child, AppState, CommandError,
    ConnectionStatePayload, ConnectionStatus,
};
use crate::core::config::build_connection_config;
use crate::core::snapshot::snapshot_for;
use crate::core::supervisor::wait_for_port;
use crate::storage::repos::{profile_repo, settings_repo};

/// Caller holds the mutation permit. Config generation and process launch both
/// consume the same immutable `ConfigSnapshot`.
pub(crate) async fn connect_impl(
    state: &AppState,
    profile_id: Option<String>,
) -> Result<ConnectionStatePayload, CommandError> {
    let (settings, profile) = {
        let conn = state.inner.conn.lock().await;
        let settings = settings_repo::get(&conn)?;
        let profile = match profile_id {
            Some(id) => profile_repo::list_present_enabled(&conn)?
                .into_iter()
                .find(|profile| profile.id == id),
            None => best_eligible_profile(&conn)?,
        }
        .ok_or(CommandError::NotFound)?;
        (settings, profile)
    };

    let port = u16::try_from(settings.local_proxy_port).map_err(|_| CommandError::InvalidInput)?;
    if port == 0 {
        return Err(CommandError::InvalidInput);
    }
    let exclusions = exclusions(&settings);

    // A switch while connected must first give up ownership of the old
    // connection. Stopping its core and restoring its proxy means the proxy
    // read below is the user's original state, not our own local proxy.
    let (mut old_child, old_previous, old_config) = {
        let mut runtime = state.inner.runtime.lock().await;
        (
            runtime.child.take(),
            runtime.previous.take(),
            runtime.config_path.take(),
        )
    };
    if !stop_child(&mut old_child) {
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }
    if let Some(previous) = old_previous {
        let runtime = state.inner.runtime.lock().await;
        if runtime.proxy.restore(&previous).is_err() {
            mark_faulted(state).await;
            return Err(CommandError::Proxy);
        }
    }
    remove_config(&old_config);

    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Connecting,
            profile_id: Some(profile.id.clone()),
            proxy_port: Some(port),
        };
        emit_connection(&runtime.connection);
    }

    let snapshot = snapshot_for(&profile, port, &state.inner.config_dir);
    let config =
        build_connection_config(&snapshot, &exclusions).map_err(|_| CommandError::Core)?;
    if let Some(parent) = snapshot.config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| CommandError::Core)?;
    }
    let serialized =
        serde_json::to_string(&config).map_err(|_| CommandError::Core)?;
    std::fs::write(&snapshot.config_path, serialized).map_err(|_| CommandError::Core)?;

    let mut child = match state.inner.launcher.launch(&snapshot) {
        Ok(child) => Some(child),
        Err(_) => {
            remove_config(&Some(snapshot.config_path.clone()));
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };

    if wait_for_port(port, Duration::from_secs(2)).is_err() {
        stop_child(&mut child);
        remove_config(&Some(snapshot.config_path.clone()));
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }
    // The old core may still be closing its port; readiness alone cannot prove
    // the replacement bound successfully. A dead new core is a hard failure.
    let new_core_alive = child
        .as_mut()
        .map(|handle| handle.try_wait().map(|exited| !exited))
        .transpose()
        .map_err(|_| CommandError::Core)?
        .unwrap_or(false);
    if !new_core_alive {
        stop_child(&mut child);
        remove_config(&Some(snapshot.config_path.clone()));
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }

    let previous = if settings.system_proxy_enabled {
        let current = {
            let runtime = state.inner.runtime.lock().await;
            runtime.proxy.current()
        };
        match current {
            Ok(current) => {
                let applied = {
                    let runtime = state.inner.runtime.lock().await;
                    runtime.proxy.apply(port, &exclusions)
                };
                if applied.is_err() {
                    stop_child(&mut child);
                    remove_config(&Some(snapshot.config_path.clone()));
                    mark_faulted(state).await;
                    return Err(CommandError::Proxy);
                }
                Some(current)
            }
            Err(_) => {
                stop_child(&mut child);
                remove_config(&Some(snapshot.config_path.clone()));
                mark_faulted(state).await;
                return Err(CommandError::Proxy);
            }
        }
    } else {
        None
    };

    let mut runtime = state.inner.runtime.lock().await;
    runtime.child = child;
    runtime.previous = previous;
    runtime.config_path = Some(snapshot.config_path);
    runtime.connection = ConnectionStatePayload {
        status: ConnectionStatus::Connected,
        profile_id: Some(profile.id),
        proxy_port: Some(port),
    };
    emit_connection(&runtime.connection);
    Ok(runtime.connection.clone())
}

/// Caller holds the mutation permit. Restores the captured proxy state even if
/// stopping the core fails, so the machine is never left behind our proxy.
pub(crate) async fn disconnect_impl(
    state: &AppState,
) -> Result<ConnectionStatePayload, CommandError> {
    let (mut child, previous, config_path) = {
        let mut runtime = state.inner.runtime.lock().await;
        (
            runtime.child.take(),
            runtime.previous.take(),
            runtime.config_path.take(),
        )
    };

    let stopped = stop_child(&mut child);
    let mut restored = true;
    if let Some(previous) = previous {
        let runtime = state.inner.runtime.lock().await;
        if runtime.proxy.restore(&previous).is_err() {
            restored = false;
        }
    }
    remove_config(&config_path);

    let status = if stopped && restored {
        ConnectionStatus::Disconnected
    } else {
        ConnectionStatus::Faulted
    };
    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.connection = ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        };
        emit_connection(&runtime.connection);
    }

    if !stopped {
        Err(CommandError::Core)
    } else if !restored {
        Err(CommandError::Proxy)
    } else {
        Ok(ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        })
    }
}

#[tauri::command]
pub async fn connection_status(
    state: State<'_, AppState>,
) -> Result<ConnectionStatePayload, CommandError> {
    let runtime = state.inner.runtime.lock().await;
    Ok(runtime.connection.clone())
}

#[tauri::command]
pub async fn connection_connect(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<ConnectionStatePayload, CommandError> {
    let _permit = state.permit()?;
    connect_impl(&state, profile_id).await
}

#[tauri::command]
pub async fn connection_disconnect(
    state: State<'_, AppState>,
) -> Result<ConnectionStatePayload, CommandError> {
    let _permit = state.permit()?;
    disconnect_impl(&state).await
}
