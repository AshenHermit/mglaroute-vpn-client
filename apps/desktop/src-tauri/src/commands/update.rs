//! Update command surface. Install is a terminal action: it waits briefly for
//! every domain guard so scheduler ticks do not spuriously block it, claims the
//! shutdown gate, disconnects, downloads/installs and restarts. On failure it
//! reconnects the previously active server before returning a generic code.
//! Native updater errors and user-supplied URLs never reach logs.

use std::time::{Duration, Instant};

use tauri::AppHandle;

use super::{AppState, CommandError, ConnectionStatus};
use crate::state::{emit, MutationError};
use crate::update::{self, EVENT_UPDATE_PROGRESS};

/// Bounded wait for one domain permit. Scheduler ticks hold these slots for a
/// short time; a one-shot probe would fail Install while a 60 s cycle is busy.
/// Only a permit still unavailable at the deadline maps to `update_busy`.
async fn wait_for_permit<T, F>(deadline: Instant, mut acquire: F) -> Result<T, MutationError>
where
    F: FnMut() -> Result<T, MutationError>,
{
    loop {
        match acquire() {
            Ok(permit) => return Ok(permit),
            Err(MutationError::Busy) => {
                if Instant::now() >= deadline {
                    return Err(MutationError::Busy);
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn update_check(app: AppHandle) -> Option<update::UpdateInfo> {
    update::check(&app).await
}

#[tauri::command]
pub(crate) async fn update_install(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    // Remember the live server before teardown wipes it. Without a connected
    // core there is nothing to restore after a failed install.
    let active_profile_id = {
        let runtime = state.inner.runtime.lock().await;
        (runtime.connection.status == ConnectionStatus::Connected)
            .then(|| runtime.connection.profile_id.clone())
            .flatten()
    };

    // Hold a permit from each domain until process replacement. The shared
    // deadline caps the total wait; permits stay held through retry/reconnect.
    let deadline = Instant::now() + Duration::from_secs(2);
    let (connection_guard, probe_guard, mutation_guard) = state.guards();
    let _connection = wait_for_permit(deadline, || connection_guard.try_acquire())
        .await
        .map_err(|_| CommandError::UpdateBusy)?;
    let _probe = wait_for_permit(deadline, || probe_guard.try_acquire())
        .await
        .map_err(|_| CommandError::UpdateBusy)?;
    let _mutation = wait_for_permit(deadline, || mutation_guard.try_acquire())
        .await
        .map_err(|_| CommandError::UpdateBusy)?;

    if !crate::lifecycle::begin_shutdown() {
        return Err(CommandError::UpdateBusy);
    }

    crate::lifecycle::cleanup(&state).await;
    if update::install(&app, |progress| emit(EVENT_UPDATE_PROGRESS, progress))
        .await
        .is_err()
    {
        // No exit on failure: release the gate so tray Exit and a retry work.
        // Cleanup already restored the proxy; reconnect the server captured
        // above so a failed download does not leave the VPN off. Best effort —
        // never mask the original install failure.
        crate::lifecycle::abort_shutdown();
        if let Some(id) = active_profile_id {
            let _ = crate::commands::connection::connect_impl(&state, Some(id)).await;
        }
        return Err(CommandError::Update);
    }

    app.restart()
}

#[tauri::command]
#[allow(deprecated)]
pub(crate) fn update_open_releases(url: String) -> Result<(), CommandError> {
    if !update::validate_release_url(&url) {
        return Err(CommandError::InvalidInput);
    }

    #[cfg(desktop)]
    {
        tauri_plugin_shell::open::open(None, &url, None).map_err(|_| CommandError::Update)?;
    }
    #[cfg(not(desktop))]
    {
        return Err(CommandError::Unsupported);
    }

    Ok(())
}
