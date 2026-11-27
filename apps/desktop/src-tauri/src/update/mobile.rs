//! Mobile backend placeholder. App stores own updates on Android/iOS; the
//! signatures stay compatible with the desktop backend so shared command code
//! compiles unchanged.

use tauri::AppHandle;

use super::{UpdateInfo, UpdateProgressEvent};

pub(crate) async fn check(_app: &AppHandle) -> Option<UpdateInfo> {
    None
}

pub(crate) async fn install<F>(_app: &AppHandle, _on_progress: F) -> Result<(), ()>
where
    F: FnMut(UpdateProgressEvent) + Send,
{
    Err(())
}
