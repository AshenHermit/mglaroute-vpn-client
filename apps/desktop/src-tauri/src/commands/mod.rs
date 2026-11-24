//! Tauri command surface. Every mutating command takes the single-occupancy
//! mutation permit first; read commands never mutate storage or OS state.
//! Payloads are camelCase to match `packages/contracts`; the database stays
//! snake_case. No URL, credential, raw config or generated config is logged.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::anti_dpi::AntiDpiStatus;
use crate::core::snapshot::ConfigSnapshot;
use crate::core::supervisor::{binary_path, ChildGuard, SupervisorError};
use crate::platform::proxy::{load_previous, PreviousProxy, ProxyBackend};
use crate::platform::proxy_windows::WindowsProxy;
use crate::probe::scheduler::should_refetch;
use crate::service_profiles::ensure_defaults_on_empty;
use crate::state::{emit, MutationError, MutationGuard, MutationPermit};
use crate::storage::migrate;
use crate::storage::models::Settings;
use crate::storage::repos::{settings_repo, subscription_repo};

pub mod connection;
pub mod probe;
pub mod service_profile;
pub mod settings;
pub mod subscription;

pub use connection::{connection_connect, connection_disconnect, connection_status};
pub use probe::{best_eligible_profile, probe_run, profile_list};
pub use service_profile::{
    service_profile_create, service_profile_list, service_profile_remove, service_profile_update,
};
pub use settings::{settings_get, settings_update};
pub use subscription::{
    subscription_add, subscription_list, subscription_refresh, subscription_remove,
    subscription_update,
};

/// Generic, secret-free command errors. Underlying library errors are never
/// forwarded: reqwest messages can echo a subscription URL and its token.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandError {
    #[error("another mutation is already in progress")]
    Busy,
    #[error("not found")]
    NotFound,
    #[error("invalid input")]
    InvalidInput,
    #[error("storage failure")]
    Storage,
    #[error("fetch failed")]
    Fetch,
    #[error("core failure")]
    Core,
    #[error("proxy failure")]
    Proxy,
    #[error("probe failure")]
    Probe,
    #[error("unsupported")]
    Unsupported,
}

impl From<MutationError> for CommandError {
    fn from(_: MutationError) -> Self {
        Self::Busy
    }
}

impl From<rusqlite::Error> for CommandError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}

impl From<SupervisorError> for CommandError {
    fn from(_: SupervisorError) -> Self {
        Self::Core
    }
}

impl From<crate::probe::cycle::ProbeError> for CommandError {
    fn from(_: crate::probe::cycle::ProbeError) -> Self {
        Self::Probe
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Faulted,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatePayload {
    pub status: ConnectionStatus,
    pub profile_id: Option<String>,
    pub proxy_port: Option<u16>,
}

/// Owns the spawned core. Production wraps `ChildGuard`; tests inject a fake.
pub trait CoreHandle: Send {
    fn stop(&mut self) -> std::result::Result<(), String>;
}

pub trait CoreLauncher: Send + Sync {
    fn launch(&self, snapshot: &ConfigSnapshot) -> std::result::Result<Box<dyn CoreHandle>, String>;
}

struct XrayCoreHandle(ChildGuard);

impl CoreHandle for XrayCoreHandle {
    fn stop(&mut self) -> std::result::Result<(), String> {
        self.0.stop().map_err(|_| "failed to stop core".to_string())
    }
}

/// Resolves the binary with `core::supervisor::binary_path` (env → override →
/// bundled) before spawning, so a missing binary is reported generically.
pub struct XrayLauncher;

impl CoreLauncher for XrayLauncher {
    fn launch(&self, snapshot: &ConfigSnapshot) -> std::result::Result<Box<dyn CoreHandle>, String> {
        if !binary_path(None).exists() {
            return Err("core binary unavailable".to_string());
        }
        ChildGuard::spawn(snapshot)
            .map(|child| Box::new(XrayCoreHandle(child)) as Box<dyn CoreHandle>)
            .map_err(|_| "failed to start core".to_string())
    }
}

struct RuntimeState {
    connection: ConnectionStatePayload,
    child: Option<Box<dyn CoreHandle>>,
    previous: Option<PreviousProxy>,
    config_path: Option<PathBuf>,
    proxy: Box<dyn ProxyBackend + Send + Sync>,
    last_switch: Option<DateTime<Utc>>,
}

struct AppStateInner {
    conn: Mutex<Connection>,
    guard: MutationGuard,
    runtime: Mutex<RuntimeState>,
    config_dir: PathBuf,
    launcher: Box<dyn CoreLauncher>,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

impl AppState {
    pub fn new(
        conn: Connection,
        config_dir: PathBuf,
        proxy: Box<dyn ProxyBackend + Send + Sync>,
        launcher: Box<dyn CoreLauncher>,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                conn: Mutex::new(conn),
                guard: MutationGuard::default(),
                runtime: Mutex::new(RuntimeState {
                    connection: ConnectionStatePayload {
                        status: ConnectionStatus::Disconnected,
                        profile_id: None,
                        proxy_port: None,
                    },
                    child: None,
                    previous: None,
                    config_path: None,
                    proxy,
                    last_switch: None,
                }),
                config_dir,
                launcher,
            }),
        }
    }

    fn permit(&self) -> std::result::Result<MutationPermit, CommandError> {
        self.inner.guard.try_acquire().map_err(CommandError::from)
    }
}

/// Open storage, seed defaults and restore a proxy captured before a prior
/// process exit. No auto-connect: the operator decides when to reconnect.
pub fn bootstrap(dir: &Path) -> std::result::Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let mut conn = Connection::open(dir.join("client.db"))?;
    migrate(&conn)?;
    ensure_defaults_on_empty(&mut conn)?;

    let proxy_path = dir.join("proxy_previous.json");
    let proxy: Box<dyn ProxyBackend + Send + Sync> =
        Box::new(WindowsProxy::with_state_file(proxy_path.clone()));
    if proxy_path.exists() {
        let previous = load_previous(&proxy_path)?;
        proxy.restore(&previous)?;
    }

    Ok(AppState::new(
        conn,
        dir.to_path_buf(),
        proxy,
        Box::new(XrayLauncher),
    ))
}

fn exclusions(settings: &Settings) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&settings.proxy_exclusions).unwrap_or_default()
}

pub(crate) fn emit_connection(state: &ConnectionStatePayload) {
    emit("connection-state", state.clone());
}

async fn mark_faulted(state: &AppState) {
    let mut runtime = state.inner.runtime.lock().await;
    runtime.connection.status = ConnectionStatus::Faulted;
    emit_connection(&runtime.connection);
}

fn stop_child(child: &mut Option<Box<dyn CoreHandle>>) -> bool {
    match child.take() {
        Some(mut handle) => handle.stop().is_ok(),
        None => true,
    }
}

fn remove_config(path: &Option<PathBuf>) {
    if let Some(path) = path {
        let _ = std::fs::remove_file(path);
    }
}

#[tauri::command]
pub fn anti_dpi_status() -> AntiDpiStatus {
    crate::anti_dpi::status()
}

#[tauri::command]
pub fn anti_dpi_start() -> AntiDpiStatus {
    crate::anti_dpi::start()
}

#[tauri::command]
pub fn anti_dpi_stop() -> AntiDpiStatus {
    crate::anti_dpi::stop()
}

/// One-minute auto-refetch tick. Each eligibility check and refresh shares the
/// command mutation guard, so a manual action cannot race the scheduler.
pub async fn refetch_loop(state: AppState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.tick().await; // consume the immediate first tick
    loop {
        ticker.tick().await;
        let (subscriptions, settings) = {
            let conn = state.inner.conn.lock().await;
            let subscriptions = subscription_repo::list(&conn).unwrap_or_default();
            let settings = settings_repo::get(&conn).unwrap_or_default();
            (subscriptions, settings)
        };
        let now = Utc::now();
        for sub in subscriptions {
            if !sub.enabled || !should_refetch(&sub, now, settings.auto_refetch_enabled) {
                continue;
            }
            if let Ok(_permit) = state.permit() {
                let _ = subscription::refresh_impl(&state, &sub.id).await;
            }
        }
    }
}

/// One-minute auto-test tick, gated by `auto_test_enabled`. `probe::run_impl`
/// owns the reconnect decision and records the switch timestamp.
pub async fn auto_test_loop(state: AppState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let enabled = {
            let conn = state.inner.conn.lock().await;
            settings_repo::get(&conn)
                .map(|settings| settings.auto_test_enabled)
                .unwrap_or(false)
        };
        if enabled {
            if let Ok(_permit) = state.permit() {
                let _ = probe::run_impl(&state).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;
    use crate::anti_dpi;
    use crate::{invoke_handler, COMMAND_NAMES};
    use crate::storage::models::{Profile, ServiceProfile, Subscription};
    use crate::storage::repos::{profile_repo, service_profile_repo};

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        subscription_repo::insert(
            &conn,
            &Subscription {
                id: "sub-1".to_string(),
                name: "test".to_string(),
                url: "https://example.test/sub".to_string(),
                enabled: true,
                ..Default::default()
            },
        )
        .expect("insert subscription");

        let profile = |id: &str, latency: Option<i64>| Profile {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            server_id: id.to_string(),
            link: format!("vless://uuid@node.example.com:443#{id}"),
            latency_ms: latency,
            ..Default::default()
        };
        profile_repo::upsert_many(
            &conn,
            &[
                profile("p-slow", Some(40)),
                profile("p-fast", Some(10)),
                profile("p-mid", Some(25)),
            ],
        )
        .expect("insert profiles");

        service_profile_repo::insert(
            &conn,
            &ServiceProfile {
                id: "svc-active".to_string(),
                name: "active".to_string(),
                test_urls: json!(["https://service.test"]).to_string(),
                is_enabled: true,
                ..Default::default()
            },
        )
        .expect("insert service profile");

        // The slowest candidate fails the one active service profile.
        for (id, reachable) in [("p-slow", false), ("p-fast", true), ("p-mid", true)] {
            profile_repo::upsert_service_result(
                &conn,
                id,
                "svc-active",
                reachable,
                chrono::Utc::now(),
            )
            .expect("insert service result");
        }
        conn
    }

    #[test]
    fn connect_remote_uses_best_eligible() {
        let conn = seeded_conn();
        let chosen = best_eligible_profile(&conn)
            .expect("select best eligible")
            .expect("a passing profile exists");
        assert_eq!(chosen.id, "p-fast");
        assert_eq!(chosen.latency_ms, Some(10));
    }

    #[test]
    fn anti_dpi_stub_unsupported() {
        assert_eq!(
            anti_dpi::status().status,
            anti_dpi::AntiDpiMode::Unsupported
        );
    }

    #[test]
    fn command_names_registered() {
        // Constructing the generated handler proves every command is
        // registered; running a full Tauri app in a unit test would load the
        // host WebView2 runtime, which unit tests must not depend on.
        let _handler = invoke_handler();

        let mut names = COMMAND_NAMES.to_vec();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "anti_dpi_start",
                "anti_dpi_status",
                "anti_dpi_stop",
                "connection_connect",
                "connection_disconnect",
                "connection_status",
                "probe_run",
                "profile_list",
                "service_profile_create",
                "service_profile_list",
                "service_profile_remove",
                "service_profile_update",
                "settings_get",
                "settings_update",
                "subscription_add",
                "subscription_list",
                "subscription_refresh",
                "subscription_remove",
                "subscription_update",
            ]
        );
    }
}
