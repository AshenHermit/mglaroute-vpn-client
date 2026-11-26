//! Update command surface. Install is a terminal action: it claims every
//! domain guard so no in-flight mutation races teardown, then exits the gate,
//! disconnects, downloads/installs and restarts. Native updater errors and
//! user-supplied URLs never reach logs; all failures map to generic codes.

use tauri::AppHandle;

use super::{AppState, CommandError};
use crate::state::emit;
use crate::update::{self, EVENT_UPDATE_PROGRESS};

#[tauri::command]
pub(crate) async fn update_check(app: AppHandle) -> Option<update::UpdateInfo> {
    update::check(&app).await
}

#[tauri::command]
pub(crate) async fn update_install(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    // Hold a permit from each domain until process replacement. Disconnect's
    // connection-permit precondition is satisfied by the first one.
    let (connection_guard, probe_guard, mutation_guard) = state.guards();
    let _connection = connection_guard
        .try_acquire()
        .map_err(|_| CommandError::UpdateBusy)?;
    let _probe = probe_guard
        .try_acquire()
        .map_err(|_| CommandError::UpdateBusy)?;
    let _mutation = mutation_guard
        .try_acquire()
        .map_err(|_| CommandError::UpdateBusy)?;

    if !crate::lifecycle::begin_shutdown() {
        return Err(CommandError::UpdateBusy);
    }

    crate::lifecycle::cleanup(&state).await;
    update::install(&app, |progress| emit(EVENT_UPDATE_PROGRESS, progress))
        .await
        .map_err(|_| CommandError::Update)?;

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
