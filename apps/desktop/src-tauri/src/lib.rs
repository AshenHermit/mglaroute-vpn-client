pub mod anti_dpi;
pub mod commands;
pub mod core;
pub mod platform;
pub mod probe;
pub mod reconnect;
pub mod service_profiles;
pub mod state;
pub mod storage;
pub mod subscription;

use tauri::Manager;

use commands::*;

/// Register every command through one list; the same list supplies the names
/// asserted by the command-surface test, so the two cannot drift apart.
macro_rules! command_list {
    ($($cmd:ident),+ $(,)?) => {
        #[allow(dead_code)]
        pub(crate) const COMMAND_NAMES: &[&str] = &[$(stringify!($cmd)),+];

        pub(crate) fn invoke_handler(
        ) -> impl Fn(::tauri::ipc::Invoke<::tauri::Wry>) -> bool + Send + Sync + 'static {
            ::tauri::generate_handler![$($cmd),+]
        }
    };
}

command_list!(
    subscription_add,
    subscription_list,
    subscription_update,
    subscription_remove,
    subscription_refresh,
    service_profile_list,
    service_profile_create,
    service_profile_update,
    service_profile_remove,
    settings_get,
    settings_update,
    probe_run,
    profile_list,
    connection_status,
    connection_connect,
    connection_disconnect,
    anti_dpi_status,
    anti_dpi_start,
    anti_dpi_stop,
);

pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    let handle = runtime.handle().clone();

    tauri::Builder::default()
        .setup(move |app| {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            // Startup restore happens before either scheduler loop is spawned.
            // No auto-connect: proxy state is restored, the operator connects.
            let state = commands::bootstrap(&dir)?;
            app.manage(state.clone());
            state::install_tauri_sink(app.handle().clone());
            handle.spawn(commands::refetch_loop(state.clone()));
            handle.spawn(commands::auto_test_loop(state.clone()));
            handle.spawn(commands::child_exit_watch(state));
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running mglaroute client");
}
