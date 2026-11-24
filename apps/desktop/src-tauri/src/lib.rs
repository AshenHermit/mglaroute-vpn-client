pub mod core;
pub mod probe;
pub mod reconnect;
pub mod service_profiles;
pub mod state;
pub mod storage;
pub mod subscription;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            state::set_app_handle(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mglaroute client");
}
