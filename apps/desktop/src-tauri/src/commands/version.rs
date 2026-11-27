//! Read-only version command. Reports the bundled package version to the UI;
//! no state is read or mutated and nothing is logged.

use tauri::AppHandle;

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
