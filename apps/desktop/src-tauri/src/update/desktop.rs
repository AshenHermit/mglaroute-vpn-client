//! Desktop updater backend. All native failures collapse to `None` / `Err(())`
//! and are never logged: updater error text can carry URLs. Traffic keeps the
//! active OS proxy route — no proxy force or bypass is configured here.

use std::sync::{Mutex, MutexGuard};

use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use super::{release_url, UpdateInfo, UpdatePhase, UpdateProgressEvent};

/// Returns the announced update in frontend shape, or `None` for any failure
/// (no update available, bad config, network error).
pub(crate) async fn check(app: &AppHandle) -> Option<UpdateInfo> {
    let update = app.updater().ok()?.check().await.ok()??;
    let current_version = app.package_info().version.to_string();
    let release_url = release_url(&update.version);

    Some(UpdateInfo {
        version: update.version,
        current_version,
        notes: update.body,
        pub_date: update.date.map(|date| date.to_string()),
        can_install_in_app: true,
        release_url,
    })
}

/// Downloads and installs the announced update, reporting every phase through
/// `on_progress`. The shared mutexes let the chunk and finish callbacks both
/// feed the same emit seam without cloning the caller's callback.
pub(crate) async fn install<F>(app: &AppHandle, on_progress: F) -> Result<(), ()>
where
    F: FnMut(UpdateProgressEvent) + Send,
{
    let update = app
        .updater()
        .map_err(|_| ())?
        .check()
        .await
        .map_err(|_| ())?
        .ok_or(())?;

    let counts = Mutex::new((0u64, None::<u64>));
    let emit = Mutex::new(on_progress);

    update
        .download_and_install(
            |received, total| {
                let received = received as u64;
                *lock(&counts) = (received, total);
                (*lock(&emit))(UpdateProgressEvent {
                    phase: UpdatePhase::Downloading,
                    received,
                    total,
                });
            },
            || {
                let (received, total) = *lock(&counts);
                (*lock(&emit))(UpdateProgressEvent {
                    phase: UpdatePhase::Installing,
                    received,
                    total,
                });
            },
        )
        .await
        .map_err(|_| ())?;

    let (received, total) = *lock(&counts);
    (*lock(&emit))(UpdateProgressEvent {
        phase: UpdatePhase::Done,
        received,
        total,
    });

    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
