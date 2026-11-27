use std::time::Duration;

use rusqlite::Connection;
use tauri::State;

use super::probe::best_eligible_profile;
use super::{
    emit_connection, exclusions, mark_faulted, remove_config, stop_child, AppState, CommandError,
    ConnectionStatePayload, ConnectionStatus,
};
use crate::core::config::build_connection_config;
use crate::core::snapshot::snapshot_for;
use crate::core::supervisor::wait_for_port;
use crate::storage::models::Profile;
use crate::storage::repos::{profile_repo, settings_repo};
use crate::storage::routing_repo;

/// Resolve which server a Connect should launch, without touching the network
/// or the process. An explicit id keeps the enabled-present lookup and is not
/// gated on cached service results. With no id, a passing cached result wins;
/// otherwise fall back to the first enabled-present server by `(sort_index,
/// id)` so the very first click works before any test cycle has run. Profiles
/// from disabled subscriptions are never eligible: `list_present_enabled`
/// joins `subscriptions` and filters on `enabled`.
pub fn select_connect_profile(
    conn: &Connection,
    profile_id: Option<String>,
) -> rusqlite::Result<Option<Profile>> {
    match profile_id {
        Some(id) => Ok(profile_repo::list_present_enabled(conn)?
            .into_iter()
            .find(|profile| profile.id == id)),
        None => {
            if let Some(best) = best_eligible_profile(conn)? {
                return Ok(Some(best));
            }
            Ok(profile_repo::list_present_enabled(conn)?.into_iter().next())
        }
    }
}

/// Caller holds the connection permit. Config generation and process launch
/// both consume the same immutable `ConfigSnapshot`.
pub(crate) async fn connect_impl(
    state: &AppState,
    profile_id: Option<String>,
) -> Result<ConnectionStatePayload, CommandError> {
    let (settings, profile, rules) = {
        let conn = state.inner.conn.lock().await;
        let settings = settings_repo::get(&conn)?;
        let profile = select_connect_profile(&conn, profile_id)?
            .ok_or(CommandError::NotFound)?;
        let rules = routing_repo::list_active_rules(&conn)?;
        (settings, profile, rules)
    };

    let port = u16::try_from(settings.local_proxy_port).map_err(|_| CommandError::InvalidInput)?;
    if port == 0 {
        return Err(CommandError::InvalidInput);
    }
    let exclusions = exclusions(&settings);

    // A switch keeps the ownership taken at connect: the applied local proxy
    // and the user's original state stay in place. Restoring here would send
    // browser traffic direct (or to the old user proxy) while the replacement
    // core starts; `wait_for_port` allows a two-second leak window on every
    // probe-triggered switch. Give up ownership only when the system proxy was
    // turned off since connect. On failure the previous state stays owned and
    // recoverable, and a child whose stop failed stays owned (never orphaned).
    let stopped = {
        let mut runtime = state.inner.runtime.lock().await;
        stop_child(&mut runtime.child)
    };
    if !stopped {
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }

    let owned_previous = state.inner.runtime.lock().await.previous.clone();
    let previous = match owned_previous {
        Some(previous) if !settings.system_proxy_enabled => {
            let mut restored = false;
            for _ in 0..2 {
                let runtime = state.inner.runtime.lock().await;
                if runtime.proxy.restore(&previous).is_ok() {
                    restored = true;
                    break;
                }
            }
            if !restored {
                mark_faulted(state).await;
                return Err(CommandError::Proxy);
            }
            None
        }
        // `Some(previous)` is carried forward untouched; `None` is captured
        // from the user's state after the new core is ready.
        other => other,
    };

    let old_config = {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.previous = None;
        runtime.config_path.take()
    };
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
    let config = build_connection_config(
        &snapshot,
        &exclusions,
        &rules,
        state.inner.asset_dir.as_deref(),
    )
    .map_err(|_| CommandError::Core)?;
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
        // `wait_for_port` succeeded while our core is already gone, so the
        // listener belongs to another program. This is a port conflict, not a
        // crash of a core we owned: keep the connection slot disconnected and
        // hand the user an actionable error instead of a faulted state.
        let mut runtime = state.inner.runtime.lock().await;
        runtime.previous = previous;
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Disconnected,
            profile_id: None,
            proxy_port: None,
        };
        emit_connection(&runtime.connection);
        return Err(CommandError::PortInUse);
    }

    let previous = if settings.system_proxy_enabled {
        // Reuse the capture from the first connect. Calling `current()` on a
        // switch would read our own local proxy and leak it on disconnect.
        let captured = if previous.is_some() {
            None
        } else {
            let current = {
                let runtime = state.inner.runtime.lock().await;
                runtime.proxy.current()
            };
            match current {
                Ok(current) => Some(current),
                Err(_) => {
                    stop_child(&mut child);
                    remove_config(&Some(snapshot.config_path.clone()));
                    mark_faulted(state).await;
                    return Err(CommandError::Proxy);
                }
            }
        };
        // Re-apply only after the replacement core is ready. The applied state
        // is unchanged across a switch with the same port/exclusions.
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
        previous.or(captured)
    } else {
        previous
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

/// Caller holds the connection permit. Restores the captured proxy state even
/// if stopping the core fails, so the machine is never left behind our proxy.
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
    if !stopped && child.is_some() {
        // `stop_child` kept the handle: leave it owned by the runtime so a
        // later disconnect can retry instead of orphaning the process.
        let mut runtime = state.inner.runtime.lock().await;
        if runtime.child.is_none() {
            runtime.child = child.take();
        }
    }
    let mut restored = true;
    if let Some(previous) = previous {
        let mut ok = false;
        for _ in 0..2 {
            let runtime = state.inner.runtime.lock().await;
            if runtime.proxy.restore(&previous).is_ok() {
                ok = true;
                break;
            }
        }
        if !ok {
            let mut runtime = state.inner.runtime.lock().await;
            if runtime.previous.is_none() {
                runtime.previous = Some(previous);
            }
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
    let _permit = state.connection_permit()?;
    connect_impl(&state, profile_id).await
}

#[tauri::command]
pub async fn connection_disconnect(
    state: State<'_, AppState>,
) -> Result<ConnectionStatePayload, CommandError> {
    let _permit = state.connection_permit()?;
    disconnect_impl(&state).await
}
