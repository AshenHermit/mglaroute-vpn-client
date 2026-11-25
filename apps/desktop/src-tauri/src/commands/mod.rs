//! Tauri command surface. Mutating commands take their domain's single-occupancy
//! permit (connection, probe or mutation) first; read commands never mutate
//! storage or OS state.
//! Payloads are camelCase to match `packages/contracts`; the database stays
//! snake_case. No URL, credential, raw config or generated config is logged.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use tokio::sync::{watch, Mutex};
use tokio::time::MissedTickBehavior;

use crate::anti_dpi::AntiDpiStatus;
use crate::core::job::{reap_orphans, JobHandle};
use crate::core::snapshot::ConfigSnapshot;
use crate::core::supervisor::{binary_path, ChildGuard, SupervisorError};
use crate::platform::proxy::{load_previous, PreviousProxy, ProxyBackend};
use crate::platform::proxy_windows::WindowsProxy;
use crate::probe::scheduler::should_refetch;
use crate::service_profiles::ensure_defaults_on_empty;
use crate::state::{emit, MutationGuard, MutationPermit};
use crate::storage::migrate;
use crate::storage::models::Settings;
use crate::storage::repos::{settings_repo, subscription_repo};

pub mod connection;
pub mod probe;
pub mod service_profile;
pub mod settings;
pub mod subscription;

pub use connection::{connection_connect, connection_disconnect, connection_status};
pub use probe::{best_eligible_profile, probe_cancel, probe_run, profile_list};
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
#[derive(Debug, thiserror::Error, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandError {
    #[error("connection operation in progress")]
    ConnectionBusy,
    #[error("probe operation in progress")]
    ProbeBusy,
    #[error("another mutation is already in progress")]
    MutationBusy,
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
    /// `true` once the core has exited; never blocks.
    fn try_wait(&mut self) -> std::result::Result<bool, String>;
}

pub trait CoreLauncher: Send + Sync {
    fn launch(&self, snapshot: &ConfigSnapshot) -> std::result::Result<Box<dyn CoreHandle>, String>;
}

struct XrayCoreHandle(ChildGuard);

impl CoreHandle for XrayCoreHandle {
    fn stop(&mut self) -> std::result::Result<(), String> {
        self.0.stop().map_err(|_| "failed to stop core".to_string())
    }

    fn try_wait(&mut self) -> std::result::Result<bool, String> {
        self.0
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|_| "failed to poll core".to_string())
    }
}

/// Resolves the binary with `core::supervisor::binary_path` (env → override →
/// bundled) before spawning, so a missing binary is reported generically. The
/// shared job owns every connection core started through this launcher.
pub struct XrayLauncher {
    job: Arc<JobHandle>,
}

impl CoreLauncher for XrayLauncher {
    fn launch(&self, snapshot: &ConfigSnapshot) -> std::result::Result<Box<dyn CoreHandle>, String> {
        if !binary_path(None).exists() {
            return Err("core binary unavailable".to_string());
        }
        ChildGuard::spawn(snapshot, Some(&self.job))
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
    connection_guard: MutationGuard,
    probe_guard: MutationGuard,
    mutation_guard: MutationGuard,
    runtime: Mutex<RuntimeState>,
    config_dir: PathBuf,
    launcher: Box<dyn CoreLauncher>,
    job: Arc<JobHandle>,
    probe_running: AtomicBool,
    probe_cancel_tx: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

impl AppState {
    /// Test constructor: owns no job, so a fake launcher needs no Win32 setup.
    pub fn new(
        conn: Connection,
        config_dir: PathBuf,
        proxy: Box<dyn ProxyBackend + Send + Sync>,
        launcher: Box<dyn CoreLauncher>,
    ) -> Self {
        Self::new_with_job(conn, config_dir, proxy, launcher, Arc::new(JobHandle::noop()))
    }

    /// Production constructor: keeps the shared job alive for as long as the
    /// state (and therefore its connection cores) exists.
    pub fn new_with_job(
        conn: Connection,
        config_dir: PathBuf,
        proxy: Box<dyn ProxyBackend + Send + Sync>,
        launcher: Box<dyn CoreLauncher>,
        job: Arc<JobHandle>,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                conn: Mutex::new(conn),
                connection_guard: MutationGuard::default(),
                probe_guard: MutationGuard::default(),
                mutation_guard: MutationGuard::default(),
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
                job,
                probe_running: AtomicBool::new(false),
                probe_cancel_tx: watch::Sender::new(false),
            }),
        }
    }

    /// Shared job owning this state's connection cores.
    pub fn job(&self) -> Arc<JobHandle> {
        self.inner.job.clone()
    }

    fn connection_permit(&self) -> std::result::Result<MutationPermit, CommandError> {
        self.inner
            .connection_guard
            .try_acquire()
            .map_err(|_| CommandError::ConnectionBusy)
    }

    /// Non-blocking variant for the crash watcher: skip the tick when a
    /// connection command already owns the slot.
    fn try_connection_permit(&self) -> Option<MutationPermit> {
        self.inner.connection_guard.try_acquire().ok()
    }

    fn probe_permit(&self) -> std::result::Result<MutationPermit, CommandError> {
        self.inner
            .probe_guard
            .try_acquire()
            .map_err(|_| CommandError::ProbeBusy)
    }

    /// Begin a cycle: clear any previous stop request, mark the probe as
    /// running (so `probe_cancel` is accepted) and hand out the watch receiver.
    pub fn start_probe(&self) -> watch::Receiver<bool> {
        self.inner.probe_running.store(true, Ordering::SeqCst);
        self.inner.probe_cancel_tx.send_replace(false);
        self.inner.probe_cancel_tx.subscribe()
    }

    /// End a cycle. `probe-cancelled` is emitted by `probe_cancel` when the
    /// stop is accepted, never by the cycle or by this method.
    pub fn finish_probe(&self) {
        self.inner.probe_running.store(false, Ordering::SeqCst);
    }

    fn permit(&self) -> std::result::Result<MutationPermit, CommandError> {
        self.inner
            .mutation_guard
            .try_acquire()
            .map_err(|_| CommandError::MutationBusy)
    }
}

/// Open storage, seed defaults, reap stale probe cores, then restore a proxy
/// captured before a prior process exit. No auto-connect: the operator decides
/// when to reconnect.
pub fn bootstrap(dir: &Path) -> std::result::Result<AppState, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let mut conn = Connection::open(dir.join("client.db"))?;
    migrate(&conn)?;
    ensure_defaults_on_empty(&mut conn)?;

    // Reap before restoring the proxy: verification rejects recycled PIDs and
    // never reads or logs config contents.
    reap_orphans(&std::env::temp_dir())?;
    let job = Arc::new(JobHandle::create()?);

    let proxy_path = dir.join("proxy_previous.json");
    let proxy: Box<dyn ProxyBackend + Send + Sync> =
        Box::new(WindowsProxy::with_state_file(proxy_path.clone()));
    if proxy_path.exists() {
        let previous = load_previous(&proxy_path)?;
        proxy.restore(&previous)?;
    }

    Ok(AppState::new_with_job(
        conn,
        dir.to_path_buf(),
        proxy,
        Box::new(XrayLauncher { job: job.clone() }),
        job,
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

/// Caller holds the connection permit. When the connected core has exited,
/// restore the captured proxy, drop the temp config and report `Faulted`.
/// No restart is attempted — the operator reconnects explicitly.
pub(crate) async fn handle_exited_child(
    state: &AppState,
) -> std::result::Result<bool, CommandError> {
    let (child, previous, config_path, profile_id) = {
        let mut runtime = state.inner.runtime.lock().await;
        if runtime.connection.status != ConnectionStatus::Connected {
            return Ok(false);
        }
        let exited = match runtime.child.as_mut() {
            Some(handle) => handle.try_wait().map_err(|_| CommandError::Core)?,
            None => return Ok(false),
        };
        if !exited {
            return Ok(false);
        }
        (
            runtime.child.take(),
            runtime.previous.take(),
            runtime.config_path.take(),
            runtime.connection.profile_id.clone(),
        )
    };

    // The process already exited; dropping the handle just frees the slot.
    drop(child);
    let mut restore_ok = true;
    if let Some(previous) = previous {
        for _ in 0..2 {
            let runtime = state.inner.runtime.lock().await;
            if runtime.proxy.restore(&previous).is_ok() {
                restore_ok = true;
                break;
            }
            restore_ok = false;
        }
    }
    remove_config(&config_path);

    let mut runtime = state.inner.runtime.lock().await;
    runtime.connection = ConnectionStatePayload {
        status: ConnectionStatus::Faulted,
        profile_id,
        proxy_port: None,
    };
    emit_connection(&runtime.connection);
    if !restore_ok {
        // Generic only: proxy errors never carry subscription data.
        return Err(CommandError::Proxy);
    }
    Ok(true)
}

/// Non-blocking probe of the connected core. No permit: a probe cycle can hold
/// the single mutation permit for a while and must not delay crash detection.
pub(crate) fn child_has_exited(state: &AppState) -> bool {
    let mut runtime = match state.inner.runtime.try_lock() {
        Ok(runtime) => runtime,
        Err(_) => return false,
    };
    if runtime.connection.status != ConnectionStatus::Connected {
        return false;
    }
    runtime
        .child
        .as_mut()
        .is_some_and(|handle| handle.try_wait().unwrap_or(false))
}

/// Polls the connected core every 500 ms. `try_wait` needs no permit; the
/// connection guard is taken only for the state/proxy transition, and a tick
/// that cannot get it is skipped so an in-flight connect is never delayed.
pub async fn child_exit_watch(state: AppState) {
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if !child_has_exited(&state) {
            continue;
        }
        let Some(_permit) = state.try_connection_permit() else {
            continue;
        };
        if let Err(error) = handle_exited_child(&state).await {
            if error == CommandError::Proxy {
                // Generic text only: proxy errors never carry secrets.
                eprintln!("failed to restore previous proxy after core exit");
            }
        }
    }
}

/// Stops the child, retrying once. A handle whose stop failed is put back into
/// `child` so the caller can retry; `ChildGuard`'s `Drop` is only a panic/early-
/// return backstop that kills the process if the handle is truly dropped.
fn stop_child(child: &mut Option<Box<dyn CoreHandle>>) -> bool {
    let Some(mut handle) = child.take() else {
        return true;
    };
    if handle.try_wait().unwrap_or(false) {
        return true;
    }
    if handle.stop().is_ok() {
        return true;
    }
    if handle.try_wait().unwrap_or(false) {
        return true;
    }
    *child = Some(handle);
    false
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
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
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
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
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
            if let Ok(_permit) = state.probe_permit() {
                let cancel = state.start_probe();
                let _ = probe::run_impl(&state, true, cancel).await;
                state.finish_probe();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;
    use crate::anti_dpi;
    use crate::commands::connection::{connect_impl, select_connect_profile};
    use crate::commands::probe::{apply_reconnect, candidates};
    use crate::platform::proxy::{
        override_string, proxy_string, InMemoryProxy, PreviousProxy, ProxyBackend, ProxyError,
    };
    use crate::state::{set_event_sink, RecordingEventSink};
    use crate::{invoke_handler, COMMAND_NAMES};
    use crate::storage::models::{Profile, ServiceProfile, Subscription};
    use crate::storage::repos::{profile_repo, service_profile_repo, settings_repo};

    struct FakeLauncher {
        stopped: Arc<AtomicBool>,
    }

    impl CoreLauncher for FakeLauncher {
        fn launch(
            &self,
            _snapshot: &ConfigSnapshot,
        ) -> std::result::Result<Box<dyn CoreHandle>, String> {
            Ok(Box::new(FakeHandle {
                exited: false,
                stopped: self.stopped.clone(),
            }))
        }
    }

    struct FakeHandle {
        exited: bool,
        stopped: Arc<AtomicBool>,
    }

    impl CoreHandle for FakeHandle {
        fn stop(&mut self) -> std::result::Result<(), String> {
            self.stopped.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn try_wait(&mut self) -> std::result::Result<bool, String> {
            Ok(self.exited)
        }
    }

    struct FailingStopHandle;

    impl CoreHandle for FailingStopHandle {
        fn stop(&mut self) -> std::result::Result<(), String> {
            Err("stop failed".to_string())
        }

        fn try_wait(&mut self) -> std::result::Result<bool, String> {
            Ok(false)
        }
    }

    struct FailingRestoreProxy {
        current: std::sync::Mutex<PreviousProxy>,
    }

    impl FailingRestoreProxy {
        fn new(previous: PreviousProxy) -> Self {
            Self {
                current: std::sync::Mutex::new(previous),
            }
        }
    }

    impl ProxyBackend for FailingRestoreProxy {
        fn current(&self) -> std::result::Result<PreviousProxy, ProxyError> {
            Ok(self
                .current
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone())
        }

        fn apply(&self, port: u16, exclusions: &[String]) -> std::result::Result<(), ProxyError> {
            *self
                .current
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = PreviousProxy::Manual {
                server: proxy_string(port),
                override_list: override_string(exclusions),
            };
            Ok(())
        }

        fn restore(&self, _previous: &PreviousProxy) -> std::result::Result<(), ProxyError> {
            Err(ProxyError::Unsupported)
        }
    }

    /// Observes restore calls and shares state with the launcher so a switch
    /// can be inspected while the replacement core is starting.
    struct SharedProxy {
        inner: Arc<InMemoryProxy>,
        restores: Arc<AtomicUsize>,
    }

    impl ProxyBackend for SharedProxy {
        fn current(&self) -> std::result::Result<PreviousProxy, ProxyError> {
            self.inner.current()
        }

        fn apply(&self, port: u16, exclusions: &[String]) -> std::result::Result<(), ProxyError> {
            self.inner.apply(port, exclusions)
        }

        fn restore(&self, previous: &PreviousProxy) -> std::result::Result<(), ProxyError> {
            self.restores.fetch_add(1, Ordering::SeqCst);
            self.inner.restore(previous)
        }
    }

    #[derive(Clone)]
    struct SamplingLauncher {
        stopped: Arc<AtomicBool>,
        proxy: Arc<InMemoryProxy>,
        samples: Arc<std::sync::Mutex<Vec<PreviousProxy>>>,
    }

    impl CoreLauncher for SamplingLauncher {
        fn launch(
            &self,
            _snapshot: &ConfigSnapshot,
        ) -> std::result::Result<Box<dyn CoreHandle>, String> {
            self.samples
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(self.proxy.state());
            Ok(Box::new(FakeHandle {
                exited: false,
                stopped: self.stopped.clone(),
            }))
        }
    }

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
    fn connect_none_prefers_best_eligible() {
        let conn = seeded_conn();
        let chosen = select_connect_profile(&conn, None)
            .expect("select best eligible")
            .expect("a passing profile exists");
        assert_eq!(chosen.id, "p-fast");
    }

    #[test]
    fn connect_none_falls_back_only_to_enabled_subscriptions() {
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
        .expect("insert enabled subscription");
        subscription_repo::insert(
            &conn,
            &Subscription {
                id: "sub-disabled".to_string(),
                name: "disabled".to_string(),
                url: "https://example.test/disabled".to_string(),
                enabled: false,
                ..Default::default()
            },
        )
        .expect("insert disabled subscription");
        let profile =
            |id: &str, subscription_id: &str, sort_index: i64, latency: Option<i64>| Profile {
                id: id.to_string(),
                subscription_id: subscription_id.to_string(),
                server_id: id.to_string(),
                link: format!("vless://uuid@node.example.com:443#{id}"),
                latency_ms: latency,
                sort_index,
                ..Default::default()
            };
        // No cached passing results: every enabled profile is unmeasured, so
        // `best_eligible_profile` has no candidate and selection must fall back
        // to the first enabled-present row by (sort_index, id).
        profile_repo::upsert_many(
            &conn,
            &[
                profile("p-slow", "sub-1", 2, None),
                profile("p-fast", "sub-1", -1, None),
                profile("p-mid", "sub-1", 0, None),
                profile("p-disabled", "sub-disabled", -100, Some(1)),
            ],
        )
        .expect("insert profiles");

        let chosen = select_connect_profile(&conn, None)
            .expect("select fallback")
            .expect("an enabled profile exists");
        assert_eq!(chosen.id, "p-fast");
        assert_ne!(chosen.id, "p-disabled");
    }

    #[test]
    fn connect_some_ignores_service_gating() {
        let conn = seeded_conn();
        // p-slow is present+enabled but fails the active service profile;
        // an explicit selection must still resolve it.
        let chosen = select_connect_profile(&conn, Some("p-slow".to_string()))
            .expect("select by id")
            .expect("present enabled profile selected by id");
        assert_eq!(chosen.id, "p-slow");
        assert!(
            select_connect_profile(&conn, Some("missing".to_string()))
                .expect("select by id")
                .is_none()
        );
    }

    #[test]
    fn anti_dpi_stub_unsupported() {
        assert_eq!(
            anti_dpi::status().status,
            anti_dpi::AntiDpiMode::Unsupported
        );
    }

    fn state_with_fake_launcher() -> AppState {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        AppState::new(
            conn,
            PathBuf::new(),
            Box::new(InMemoryProxy::new(PreviousProxy::Direct)),
            Box::new(FakeLauncher {
                stopped: Arc::new(AtomicBool::new(false)),
            }),
        )
    }

    #[tokio::test]
    async fn probe_guard_does_not_block_connection_guard() {
        let state = state_with_fake_launcher();
        let probe = state.probe_permit().expect("probe permit");
        assert!(state.connection_permit().is_ok());
        assert_eq!(state.probe_permit().err(), Some(CommandError::ProbeBusy));
        drop(probe);
        assert!(state.probe_permit().is_ok());
    }

    #[tokio::test]
    async fn connection_guard_does_not_block_mutation_guard() {
        let state = state_with_fake_launcher();
        let connection = state.connection_permit().expect("connection permit");
        assert_eq!(
            state.connection_permit().err(),
            Some(CommandError::ConnectionBusy)
        );
        assert!(state.permit().is_ok());
        drop(connection);
        assert!(state.connection_permit().is_ok());
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
                "probe_cancel",
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

    #[test]
    fn disabled_subscription_profiles_excluded() {
        let conn = seeded_conn();
        subscription_repo::insert(
            &conn,
            &Subscription {
                id: "sub-disabled".to_string(),
                name: "disabled".to_string(),
                url: "https://example.test/disabled".to_string(),
                enabled: false,
                ..Default::default()
            },
        )
        .expect("insert disabled subscription");
        profile_repo::upsert_many(
            &conn,
            &[Profile {
                id: "p-disabled".to_string(),
                subscription_id: "sub-disabled".to_string(),
                server_id: "p-disabled".to_string(),
                link: "vless://uuid@node.example.com:443#disabled".to_string(),
                latency_ms: Some(1),
                ..Default::default()
            }],
        )
        .expect("insert disabled profile");
        profile_repo::upsert_service_result(
            &conn,
            "p-disabled",
            "svc-active",
            true,
            chrono::Utc::now(),
        )
        .expect("insert service result");

        let ids: Vec<String> = candidates(&conn)
            .expect("candidates")
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert!(
            !ids.contains(&"p-disabled".to_string()),
            "disabled subscription must not enter probe/connect candidates"
        );

        // p-disabled is faster (1 ms) but must never win over the enabled p-fast.
        let chosen = best_eligible_profile(&conn)
            .expect("select best eligible")
            .expect("a passing profile exists");
        assert_eq!(chosen.id, "p-fast");
    }

    #[tokio::test]
    async fn child_exit_sets_faulted_and_restores_proxy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");

        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: Arc::new(AtomicBool::new(false)),
            }),
        );
        let config_path = dir.path().join("xray-p-1-10808.json");
        std::fs::write(&config_path, "{}").expect("write temp config");
        {
            let mut runtime = state.inner.runtime.lock().await;
            runtime.proxy.apply(10808, &[]).expect("apply proxy");
            runtime.child = Some(Box::new(FakeHandle {
                exited: true,
                stopped: Arc::new(AtomicBool::new(false)),
            }));
            runtime.previous = Some(original.clone());
            runtime.config_path = Some(config_path.clone());
            runtime.connection = ConnectionStatePayload {
                status: ConnectionStatus::Connected,
                profile_id: Some("p-1".to_string()),
                proxy_port: Some(10808),
            };
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert_ne!(runtime.proxy.current().expect("current proxy"), original);
        }

        let _permit = state.permit().expect("permit");
        assert!(
            handle_exited_child(&state)
                .await
                .expect("child exit transition")
        );

        let runtime = state.inner.runtime.lock().await;
        assert_eq!(runtime.connection.status, ConnectionStatus::Faulted);
        assert!(runtime.child.is_none());
        assert!(runtime.previous.is_none());
        assert!(runtime.config_path.is_none());
        assert_eq!(runtime.proxy.current().expect("restored proxy"), original);
        assert!(!config_path.exists());
    }

    fn conn_with_profiles() -> Connection {
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
        let profile = |id: &str| Profile {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            server_id: id.to_string(),
            link: format!("vless://uuid@node.example.com:443#{id}"),
            protocol: Some("vless".to_string()),
            address: Some("node.example.com".to_string()),
            port: Some(443),
            raw_config: json!({"security": "none", "type": "raw"}).to_string(),
            ..Default::default()
        };
        profile_repo::upsert_many(&conn, &[profile("p-1"), profile("p-best")])
            .expect("insert profiles");
        conn
    }

    async fn prepare_connected(
        state: &AppState,
        profile_id: &str,
        previous: PreviousProxy,
        port: u16,
        stopped: Arc<AtomicBool>,
        exited: bool,
    ) -> std::path::PathBuf {
        prepare_connected_child(
            state,
            profile_id,
            previous,
            port,
            Box::new(FakeHandle { exited, stopped }),
        )
        .await
    }

    async fn prepare_connected_child(
        state: &AppState,
        profile_id: &str,
        previous: PreviousProxy,
        port: u16,
        child: Box<dyn CoreHandle>,
    ) -> std::path::PathBuf {
        let config_path = state
            .inner
            .config_dir
            .join(format!("xray-{profile_id}-{port}.json"));
        std::fs::write(&config_path, "{}").expect("write temp config");
        let mut runtime = state.inner.runtime.lock().await;
        runtime.proxy.apply(port, &[]).expect("apply proxy");
        runtime.child = Some(child);
        runtime.previous = Some(previous);
        runtime.config_path = Some(config_path.clone());
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Connected,
            profile_id: Some(profile_id.to_string()),
            proxy_port: Some(port),
        };
        config_path
    }

    #[tokio::test]
    async fn switch_while_connected_stops_old_core_and_restores_proxy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = true;
        settings_repo::update(&conn, &settings).expect("update settings");

        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: stopped.clone(),
            }),
        );
        let old_config =
            prepare_connected(&state, "p-1", original.clone(), port, stopped.clone(), false).await;

        {
            let _permit = state.permit().expect("permit");
            connect_impl(&state, Some("p-best".to_string()))
                .await
                .expect("switch");
        }

        assert!(stopped.load(Ordering::SeqCst), "old core must be stopped");
        assert!(!old_config.exists(), "old temp config must be removed");
        let runtime = state.inner.runtime.lock().await;
        assert_eq!(
            runtime.previous.as_ref(),
            Some(&original),
            "original proxy must stay owned, not be replaced with our own proxy"
        );
        assert_eq!(runtime.connection.status, ConnectionStatus::Connected);
        assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-best"));
        assert_eq!(runtime.proxy.current().expect("current proxy"), PreviousProxy::Manual {
            server: proxy_string(port),
            override_list: override_string(&[]),
        });
        assert!(runtime.child.is_some());
    }

    #[tokio::test]
    async fn switch_keeps_local_proxy_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = true;
        settings_repo::update(&conn, &settings).expect("update settings");

        // ProxyEnable=0 analogue: the machine is direct before connecting.
        let original = PreviousProxy::Direct;
        let proxy = Arc::new(InMemoryProxy::new(original.clone()));
        let restores = Arc::new(AtomicUsize::new(0));
        let samples = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(SharedProxy {
                inner: proxy.clone(),
                restores: restores.clone(),
            }),
            Box::new(SamplingLauncher {
                stopped: stopped.clone(),
                proxy: proxy.clone(),
                samples: samples.clone(),
            }),
        );
        prepare_connected(&state, "p-1", original.clone(), port, stopped.clone(), false).await;

        {
            let _permit = state.permit().expect("permit");
            connect_impl(&state, Some("p-best".to_string()))
                .await
                .expect("switch");
        }

        let local = PreviousProxy::Manual {
            server: proxy_string(port),
            override_list: override_string(&[]),
        };
        assert_eq!(
            restores.load(Ordering::SeqCst),
            0,
            "switch must not restore the user's original proxy"
        );
        let observed = samples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        assert!(!observed.is_empty(), "launcher must sample proxy state");
        assert!(
            observed.iter().all(|value| value == &local),
            "proxy must stay pointed at the local proxy across the switch: {observed:?}"
        );
        let runtime = state.inner.runtime.lock().await;
        assert_eq!(runtime.proxy.current().expect("current proxy"), local);
        assert_eq!(
            runtime.previous.as_ref(),
            Some(&original),
            "original state must stay owned across the switch"
        );
    }

    #[tokio::test]
    async fn reconnect_gate_records_last_switch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = true;
        settings_repo::update(&conn, &settings).expect("update settings");

        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: stopped.clone(),
            }),
        );
        prepare_connected(&state, "p-1", original, port, stopped, false).await;

        let now: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-11-25T00:00:00.000Z")
            .expect("parse timestamp")
            .with_timezone(&Utc);
        let candidates = vec![
            ("p-1".to_string(), 20, true),
            ("p-best".to_string(), 10, true),
        ];
        let mut reconnect_settings = settings.clone();
        reconnect_settings.auto_reconnect_enabled = false;
        reconnect_settings.reconnect_cooldown_seconds = 60;

        {
            apply_reconnect(&state, &reconnect_settings, &candidates, now)
                .await
                .expect("disabled gate");
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-1"));
            assert!(runtime.last_switch.is_none());
        }

        reconnect_settings.auto_reconnect_enabled = true;
        {
            let mut runtime = state.inner.runtime.lock().await;
            runtime.last_switch = Some(now);
        }
        {
            apply_reconnect(&state, &reconnect_settings, &candidates, now)
                .await
                .expect("cooldown gate");
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-1"));
        }

        {
            let mut runtime = state.inner.runtime.lock().await;
            runtime.last_switch = Some(now - ChronoDuration::seconds(120));
        }
        {
            apply_reconnect(&state, &reconnect_settings, &candidates, now)
                .await
                .expect("switch after cooldown");
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-best"));
            assert_eq!(runtime.last_switch, Some(now));
        }
    }

    #[tokio::test]
    async fn apply_reconnect_skips_when_connection_busy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = true;
        settings.auto_reconnect_enabled = true;
        settings.reconnect_cooldown_seconds = 0;
        settings_repo::update(&conn, &settings).expect("update settings");

        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: stopped.clone(),
            }),
        );
        prepare_connected(&state, "p-1", original, port, stopped, false).await;

        let candidates = vec![
            ("p-1".to_string(), 20, true),
            ("p-best".to_string(), 10, true),
        ];
        // A manual connect owns the connection slot; the probe tick must skip
        // instead of stealing it or surfacing a busy error.
        let _permit = state.connection_permit().expect("connection permit");
        apply_reconnect(&state, &settings, &candidates, Utc::now())
            .await
            .expect("busy connection slot is skipped, not an error");

        let runtime = state.inner.runtime.lock().await;
        assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-1"));
        assert!(runtime.last_switch.is_none());
    }

    #[tokio::test]
    async fn child_crash_restore_failure_is_surfaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(FailingRestoreProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: Arc::new(AtomicBool::new(false)),
            }),
        );
        let config_path = prepare_connected(
            &state,
            "p-1",
            original,
            10808,
            Arc::new(AtomicBool::new(false)),
            true,
        )
        .await;

        {
            let _permit = state.permit().expect("permit");
            let result = handle_exited_child(&state).await;
            assert_eq!(
                result.expect_err("restore failure must surface"),
                CommandError::Proxy
            );
        }

        let runtime = state.inner.runtime.lock().await;
        assert_eq!(runtime.connection.status, ConnectionStatus::Faulted);
        assert!(runtime.child.is_none());
        assert!(runtime.previous.is_none());
        assert!(runtime.config_path.is_none());
        assert_eq!(
            runtime.proxy.current().expect("proxy still ours"),
            PreviousProxy::Manual {
                server: proxy_string(10808),
                override_list: override_string(&[]),
            }
        );
        assert!(!config_path.exists());
    }

    #[test]
    fn connection_state_event_emitted() {
        let recorder = Arc::new(RecordingEventSink::new());
        set_event_sink(recorder.clone());

        emit_connection(&ConnectionStatePayload {
            status: ConnectionStatus::Faulted,
            profile_id: Some("p-1".to_string()),
            proxy_port: None,
        });

        let expected = json!({
            "status": "faulted",
            "profileId": "p-1",
            "proxyPort": null,
        });
        assert!(
            recorder
                .events()
                .iter()
                .any(|(event, payload)| event == "connection-state" && *payload == expected),
            "connection-state event with camelCase payload must be emitted"
        );
    }

    #[tokio::test]
    async fn switch_failure_preserves_old_core_and_proxy() {
        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };

        // Failed stop: the old child guard must stay owned, not be dropped.
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = true;
        settings_repo::update(&conn, &settings).expect("update settings");

        let stopped = Arc::new(AtomicBool::new(false));
        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: stopped.clone(),
            }),
        );
        let config_path = prepare_connected_child(
            &state,
            "p-1",
            original.clone(),
            port,
            Box::new(FailingStopHandle),
        )
        .await;
        {
            let _permit = state.permit().expect("permit");
            let error = connect_impl(&state, Some("p-best".to_string()))
                .await
                .expect_err("stop failure must surface");
            assert_eq!(error, CommandError::Core);
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert!(runtime.child.is_some(), "old core guard must stay owned");
            assert_eq!(runtime.previous.as_ref(), Some(&original));
            assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-1"));
            assert_eq!(
                runtime.proxy.current().expect("proxy still ours"),
                PreviousProxy::Manual {
                    server: proxy_string(port),
                    override_list: override_string(&[]),
                }
            );
        }
        assert!(config_path.exists(), "old config must remain for recovery");

        // Failed restore: previous proxy and config stay recoverable. A switch
        // with the system proxy disabled is the only switch that restores; a
        // switch with it enabled keeps the local proxy applied (no leak).
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = conn_with_profiles();
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let mut settings = settings_repo::get(&conn).expect("settings");
        settings.local_proxy_port = i64::from(port);
        settings.system_proxy_enabled = false;
        settings_repo::update(&conn, &settings).expect("update settings");

        let state = AppState::new(
            conn,
            dir.path().to_path_buf(),
            Box::new(FailingRestoreProxy::new(original.clone())),
            Box::new(FakeLauncher {
                stopped: Arc::new(AtomicBool::new(false)),
            }),
        );
        let config_path = prepare_connected(
            &state,
            "p-1",
            original.clone(),
            port,
            Arc::new(AtomicBool::new(false)),
            false,
        )
        .await;
        {
            let _permit = state.permit().expect("permit");
            let error = connect_impl(&state, Some("p-best".to_string()))
                .await
                .expect_err("restore failure must surface");
            assert_eq!(error, CommandError::Proxy);
        }
        {
            let runtime = state.inner.runtime.lock().await;
            assert_eq!(
                runtime.previous.as_ref(),
                Some(&original),
                "previous proxy must stay recoverable"
            );
            assert_eq!(runtime.connection.profile_id.as_deref(), Some("p-1"));
            assert_eq!(
                runtime.proxy.current().expect("proxy still ours"),
                PreviousProxy::Manual {
                    server: proxy_string(port),
                    override_list: override_string(&[]),
                }
            );
        }
        assert!(config_path.exists(), "old config must remain for recovery");
    }
}
