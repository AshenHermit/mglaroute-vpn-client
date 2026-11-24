pub mod core;
pub mod service_profiles;
pub mod storage;
pub mod subscription;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running mglaroute client");
}
