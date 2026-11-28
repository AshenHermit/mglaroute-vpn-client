pub mod anti_dpi;
pub mod bypass;
pub mod commands;
pub mod core;
pub mod lifecycle;
pub mod platform;
pub mod probe;
pub mod reconnect;
pub mod service_profiles;
pub mod state;
pub mod storage;
pub mod subscription;
pub mod update;

use std::time::Instant;
#[cfg(desktop)]
use std::time::SystemTime;

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
    probe_cancel,
    profile_list,
    routing_profile_list,
    routing_profile_create,
    routing_profile_update,
    routing_profile_activate,
    routing_profile_remove,
    routing_rule_list,
    routing_rule_create,
    routing_rule_update,
    routing_rule_remove,
    routing_rule_reorder,
    connection_status,
    connection_connect,
    connection_disconnect,
    anti_dpi_status,
    anti_dpi_start,
    anti_dpi_stop,
    update_check,
    update_install,
    update_open_releases,
);

fn startup_phase_line(started: Instant, phase: &'static str) -> String {
    format!("startup {phase} {}ms", started.elapsed().as_millis())
}

fn trace_startup_phase(started: Instant, phase: &'static str) {
    if std::env::var_os("MGLAROUTE_STARTUP_TRACE").is_some() {
        eprintln!("{}", startup_phase_line(started, phase));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let started = Instant::now();
    #[cfg(desktop)]
    let cutoff = SystemTime::now();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    let handle = runtime.handle().clone();
    trace_startup_phase(started, "runtime-ready");

    #[cfg(mobile)]
    let vpn_plugin =
        tauri::plugin::Builder::<tauri::Wry, ()>::new(platform::vpn::mobile::VPN_PLUGIN_NAME)
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let plugin_handle = api.register_android_plugin(
                    platform::vpn::mobile::VPN_PLUGIN_PACKAGE,
                    platform::vpn::mobile::VPN_PLUGIN_CLASS,
                )?;
                app.manage(platform::vpn::mobile::MglarouteVpnBackend::new(
                    plugin_handle,
                ));
            }
            Ok(())
        })
        .build();

    #[cfg(mobile)]
    let builder = tauri::Builder::default().plugin(vpn_plugin);
    #[cfg(not(mobile))]
    let builder = tauri::Builder::default();

    builder
        .on_window_event(lifecycle::hide_on_close)
        .setup(move |app| {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            // Startup restore happens before either scheduler loop is spawned.
            // No auto-connect: proxy state is restored, the operator connects.
            let asset_dir = app
                .path()
                .resource_dir()
                .ok()
                .map(|path| path.join("binaries"));
            let state = commands::bootstrap(&dir, asset_dir)?;
            trace_startup_phase(started, "bootstrap");
            app.manage(state.clone());

            #[cfg(desktop)]
            {
                // Stale sidecars predate this launch; anything written after the
                // cutoff belongs to a probe started by the now-responsive UI.
                let temp_dir = std::env::temp_dir();
                handle.spawn_blocking(move || {
                    let _ = crate::core::job::reap_orphans(&temp_dir, Some(cutoff));
                });
            }

            state::install_tauri_sink(app.handle().clone());
            handle.spawn(commands::refetch_loop(state.clone()));
            handle.spawn(commands::auto_test_loop(state.clone()));
            handle.spawn(commands::child_exit_watch(state));
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_shell::init())?;
            }
            trace_startup_phase(started, "plugins");
            #[cfg(desktop)]
            {
                lifecycle::setup_tray(app)?;
            }
            trace_startup_phase(started, "tray");
            trace_startup_phase(started, "setup-done");
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running mglaroute client");
}

#[cfg(test)]
mod tests {
    use super::startup_phase_line;

    #[test]
    fn startup_phase_line_contains_only_fixed_label_and_elapsed_ms() {
        let line = startup_phase_line(std::time::Instant::now(), "bootstrap");
        assert!(line.starts_with("startup bootstrap "), "{line}");
        assert!(line.ends_with("ms"), "{line}");
        assert!(!line.contains('/'), "{line}");
    }

    #[test]
    fn android_config_overlay_removes_desktop_only_bundles() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tauri.android.conf.json"
        );
        let text = std::fs::read_to_string(path)
            .expect("src-tauri/tauri.android.conf.json must exist");
        let value: serde_json::Value =
            serde_json::from_str(&text).expect("valid Android overlay JSON");
        // RFC 7396 merge patch: null means "delete this member" once the
        // overlay is applied to tauri.conf.json.
        assert!(
            value
                .get("plugins")
                .and_then(|plugins| plugins.get("updater"))
                .is_none_or(serde_json::Value::is_null),
            "Android overlay must remove plugins.updater"
        );
        assert_eq!(
            value
                .get("bundle")
                .and_then(|bundle| bundle.get("resources"))
                .and_then(|resources| resources.as_array()),
            Some(
                &vec![
                    serde_json::Value::String("binaries/geoip.dat".into()),
                    serde_json::Value::String("binaries/geosite.dat".into()),
                ]
            ),
            "Android bundle resources must contain geo dat files only"
        );
        assert_eq!(
            value
                .get("bundle")
                .and_then(|bundle| bundle.get("createUpdaterArtifacts"))
                .and_then(|flag| flag.as_bool()),
            Some(false),
            "Android must not create updater artifacts"
        );
        assert_eq!(
            value
                .get("bundle")
                .and_then(|bundle| bundle.get("android"))
                .and_then(|android| android.get("minSdkVersion"))
                .and_then(|sdk| sdk.as_u64()),
            Some(26),
            "Android minSdkVersion must be 26"
        );
        assert_eq!(
            value
                .get("bundle")
                .and_then(|bundle| bundle.get("android"))
                .and_then(|android| android.get("versionCode"))
                .and_then(|code| code.as_u64()),
            Some(2003),
            "Android versionCode must be 2003"
        );
    }

    #[test]
    fn capabilities_allow_frontend_event_listeners() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities/default.json");
        let text = std::fs::read_to_string(path)
            .expect("src-tauri/capabilities/default.json must grant event permissions");
        let value: serde_json::Value =
            serde_json::from_str(&text).expect("valid capability JSON");
        let granted = value
            .get("permissions")
            .and_then(|permissions| permissions.as_array())
            .map(|permissions| {
                permissions.iter().any(|permission| {
                    matches!(
                        permission.as_str(),
                        Some("core:event:default" | "core:event:allow-listen")
                    )
                })
            })
            .unwrap_or(false);
        assert!(
            granted,
            "frontend listen() needs core:event:default or core:event:allow-listen; without it probe events are denied"
        );
    }
}
