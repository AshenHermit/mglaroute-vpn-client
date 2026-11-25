//! Probe cycle: parallel per-server xray probes with per-server events and
//! watch-based cancellation, result persistence and sorting. Testable via
//! [`ProbeTransport`] injection.

use std::future::Future;
use std::net::{Ipv4Addr, TcpListener};
use std::pin::Pin;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::watch;

use crate::core::config::build_probe_config;
use crate::core::job::JobHandle;
use crate::core::snapshot::ConfigSnapshot;
use crate::core::supervisor::{wait_for_port, ChildGuard};
use crate::probe::temp::ProbeTempFiles;
use crate::probe::{check_service_profile, measure_delay};
use crate::storage::models::{Profile, ServiceProfile, Settings};
use crate::storage::repos::{profile_repo, service_profile_repo};

/// A temporary core must accept its loopback inbound before probing starts.
pub const XRAY_STARTUP_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("storage failure: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("config failure: {0}")]
    Config(#[from] crate::core::config::ConfigError),
    #[error("serialization failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("supervisor failure: {0}")]
    Supervisor(#[from] crate::core::supervisor::SupervisorError),
}

pub type Result<T> = std::result::Result<T, ProbeError>;

/// Command result; serialized to the TS `ProbeResult` (`finishedAt`, `cancelled`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    pub tested: usize,
    pub sorted: usize,
    pub finished_at: DateTime<Utc>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeServerStatus {
    Testing,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeServiceResult {
    pub service_profile_id: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeServerEvent {
    pub profile_id: String,
    pub status: ProbeServerStatus,
    pub latency_ms: Option<u64>,
    pub service_results: Vec<ProbeServiceResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSample {
    pub profile_id: String,
    pub latency_ms: Option<u64>,
    pub service_results: Vec<ProbeServiceResult>,
}

/// Shared, immutable inputs to one profile probe. `job` contains every
/// temporary core of this cycle; `cancel` is observed before spawn and after
/// the port wait so a stop never leaves a fresh core running.
#[derive(Clone, Copy)]
pub struct ProbeContext<'a> {
    pub cancel: &'a watch::Receiver<bool>,
    pub job: &'a JobHandle,
}

pub type ProbeFutureOne<'a> = Pin<Box<dyn Future<Output = Result<Option<ProbeSample>>> + Send + 'a>>;

/// Boundary between the cycle and the slow/external work. Production probes
/// one profile through its own temporary xray core; tests inject a double.
pub trait ProbeTransport: Send + Sync {
    fn probe_profile<'a>(
        &'a self,
        profile: &'a Profile,
        services: &'a [ServiceProfile],
        settings: &'a Settings,
        ctx: ProbeContext<'a>,
    ) -> ProbeFutureOne<'a>;
}

pub trait ProbeProgress: Send + Sync {
    fn on_server(&self, event: &ProbeServerEvent);
    fn on_progress(&self, tested: usize, total: usize);
    fn on_cancelled(&self);
    fn on_finished(&self, outcome: &ProbeOutcome);
}

pub struct SilentProgress;

impl ProbeProgress for SilentProgress {
    fn on_server(&self, _event: &ProbeServerEvent) {}
    fn on_progress(&self, _tested: usize, _total: usize) {}
    fn on_cancelled(&self) {}
    fn on_finished(&self, _outcome: &ProbeOutcome) {}
}

pub async fn run_probe_cycle(conn: &mut Connection, settings: &Settings) -> Result<ProbeOutcome> {
    run_probe_cycle_with(conn, settings, &XrayProbeTransport, &TauriProgress).await
}

/// Slow/network phase only. Takes owned inputs so callers can release the
/// SQLite mutex while xray is running; a stalled probe must never block
/// unrelated reads such as `settings_get`. Up to `test_concurrency` profiles
/// are probed in parallel; one bad profile never fails the whole cycle.
pub async fn probe_cycle_detached(
    profiles: &[Profile],
    services: &[ServiceProfile],
    settings: &Settings,
    transport: &dyn ProbeTransport,
    progress: &dyn ProbeProgress,
    cancel: watch::Receiver<bool>,
) -> Result<Vec<ProbeSample>> {
    let total = profiles.len();
    let job = JobHandle::create()?;
    let ctx = ProbeContext {
        cancel: &cancel,
        job: &job,
    };
    let limit = settings.test_concurrency.clamp(1, 20) as usize;
    let mut samples = Vec::with_capacity(total);
    let mut tested = 0usize;
    let mut cancellation_sent = false;

    // Futures are lazy: building them all up front costs nothing and gives
    // `buffer_unordered` a concrete future type (no higher-ranked closure).
    let futures: Vec<ProbeFutureOne<'_>> = profiles
        .iter()
        .map(|profile| tracked_probe(profile, services, settings, transport, progress, ctx))
        .collect();
    let mut ordered = stream::iter(futures).buffer_unordered(limit);

    while let Some(result) = ordered.next().await {
        if !cancellation_sent && *cancel.borrow() {
            progress.on_cancelled();
            cancellation_sent = true;
        }
        match result? {
            Some(sample) => samples.push(sample),
            None => {
                if !cancellation_sent {
                    progress.on_cancelled();
                    cancellation_sent = true;
                }
            }
        }
        tested += 1;
        progress.on_progress(tested, total);
    }
    if !cancellation_sent && *cancel.borrow() {
        progress.on_cancelled();
    }
    Ok(samples)
}

/// Wrap one profile probe with its `testing` and terminal events. Emitting
/// `testing` here (rather than at schedule time) keeps the per-profile order
/// correct even when the concurrency limit delays a future's first poll.
fn tracked_probe<'a>(
    profile: &'a Profile,
    services: &'a [ServiceProfile],
    settings: &'a Settings,
    transport: &'a dyn ProbeTransport,
    progress: &'a dyn ProbeProgress,
    ctx: ProbeContext<'a>,
) -> ProbeFutureOne<'a> {
    let profile_id = profile.id.clone();
    Box::pin(async move {
        progress.on_server(&ProbeServerEvent {
            profile_id: profile_id.clone(),
            status: ProbeServerStatus::Testing,
            latency_ms: None,
            service_results: Vec::new(),
        });
        match transport
            .probe_profile(profile, services, settings, ctx)
            .await
        {
            Ok(Some(sample)) => {
                let status = if sample.latency_ms.is_some() {
                    ProbeServerStatus::Done
                } else {
                    ProbeServerStatus::Failed
                };
                progress.on_server(&ProbeServerEvent {
                    profile_id,
                    status,
                    latency_ms: sample.latency_ms,
                    service_results: sample.service_results.clone(),
                });
                Ok(Some(sample))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    })
}

/// Persistence options. A stopped cycle writes partial samples but skips the
/// global re-sort so untested profiles keep their previous `sort_index`.
#[derive(Debug, Clone, Copy)]
pub struct PersistOptions {
    pub sort: bool,
    pub cancelled: bool,
}

/// Short DB phase: persist samples, optionally re-sort and emit `finished`.
pub fn persist_probe_samples(
    conn: &mut Connection,
    samples: &[ProbeSample],
    progress: &dyn ProbeProgress,
    options: PersistOptions,
) -> Result<ProbeOutcome> {
    let checked_at = Utc::now();
    for sample in samples {
        let passes = passes_all(sample);
        profile_repo::set_result(
            conn,
            &sample.profile_id,
            sample.latency_ms,
            passes,
            checked_at,
        )?;
        for result in &sample.service_results {
            profile_repo::upsert_service_result(
                conn,
                &sample.profile_id,
                &result.service_profile_id,
                result.reachable,
                checked_at,
            )?;
        }
    }

    let sorted = if options.sort {
        // Passing profiles first by ascending delay; every failed profile last.
        let mut ranked: Vec<(String, Option<u64>, bool)> = samples
            .iter()
            .map(|sample| {
                (
                    sample.profile_id.clone(),
                    sample.latency_ms,
                    passes_all(sample),
                )
            })
            .collect();
        ranked.sort_by(|left, right| {
            (!left.2, left.1.unwrap_or(u64::MAX), left.0.as_str()).cmp(&(
                !right.2,
                right.1.unwrap_or(u64::MAX),
                right.0.as_str(),
            ))
        });
        let ordered_ids: Vec<String> = ranked.into_iter().map(|entry| entry.0).collect();
        profile_repo::sort_all(conn, &ordered_ids)?;
        ordered_ids.len()
    } else {
        0
    };

    let outcome = ProbeOutcome {
        tested: samples.len(),
        sorted,
        finished_at: checked_at,
        cancelled: options.cancelled,
    };
    progress.on_finished(&outcome);
    Ok(outcome)
}

pub async fn run_probe_cycle_with(
    conn: &mut Connection,
    settings: &Settings,
    transport: &dyn ProbeTransport,
    progress: &dyn ProbeProgress,
) -> Result<ProbeOutcome> {
    let profiles = profile_repo::list_present_enabled(conn)?;
    let services = service_profile_repo::list_enabled(conn)?;
    let (_tx, rx) = watch::channel(false);
    let samples =
        probe_cycle_detached(&profiles, &services, settings, transport, progress, rx).await?;
    persist_probe_samples(
        conn,
        &samples,
        progress,
        PersistOptions {
            sort: true,
            cancelled: false,
        },
    )
}

fn passes_all(sample: &ProbeSample) -> bool {
    sample.latency_ms.is_some() && sample.service_results.iter().all(|result| result.reachable)
}

/// Real transport: one temporary xray core and one loopback port per profile,
/// explicit proxy for every HTTP request. The core is stopped and both
/// sidecars removed on every exit path.
pub struct XrayProbeTransport;

impl ProbeTransport for XrayProbeTransport {
    fn probe_profile<'a>(
        &'a self,
        profile: &'a Profile,
        services: &'a [ServiceProfile],
        settings: &'a Settings,
        ctx: ProbeContext<'a>,
    ) -> ProbeFutureOne<'a> {
        Box::pin(async move { probe_profile_with_core(profile, services, settings, ctx).await })
    }
}

/// Emit progress/server/cancel/finished events through Tauri when the handle
/// was installed. Payloads contain no link, credential, raw or generated config.
pub struct TauriProgress;

impl ProbeProgress for TauriProgress {
    fn on_server(&self, event: &ProbeServerEvent) {
        crate::state::emit("probe-server", event.clone());
    }

    fn on_progress(&self, tested: usize, total: usize) {
        crate::state::emit(
            "probe-progress",
            json!({ "tested": tested, "total": total }),
        );
    }

    fn on_cancelled(&self) {
        crate::state::emit("probe-cancelled", json!({}));
    }

    fn on_finished(&self, outcome: &ProbeOutcome) {
        crate::state::emit("probe-finished", outcome.clone());
    }
}

/// Reserve one distinct ephemeral loopback port without logging it.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}

fn failed_sample(profile: &Profile, services: &[ServiceProfile]) -> ProbeSample {
    ProbeSample {
        profile_id: profile.id.clone(),
        latency_ms: None,
        service_results: services
            .iter()
            .map(|service| ProbeServiceResult {
                service_profile_id: service.id.clone(),
                reachable: false,
            })
            .collect(),
    }
}

async fn probe_profile_with_core(
    profile: &Profile,
    services: &[ServiceProfile],
    settings: &Settings,
    ctx: ProbeContext<'_>,
) -> Result<Option<ProbeSample>> {
    if *ctx.cancel.borrow() {
        return Ok(None);
    }

    let port = free_port()?;
    // A profile with unusable credentials is one failed sample, not a cycle error.
    let config = match build_probe_config(profile, port) {
        Ok(config) => config,
        Err(_) => return Ok(Some(failed_sample(profile, services))),
    };

    let files = ProbeTempFiles::new()?;
    let config_json = serde_json::to_string(&config)?;
    if let Err(error) = std::fs::write(&files.config_path, config_json) {
        files.cleanup();
        return Err(error.into());
    }
    let snapshot = ConfigSnapshot {
        subscription_id: profile.subscription_id.clone(),
        profile: profile.clone(),
        proxy_port: port,
        config_path: files.config_path.clone(),
        anti_dpi: None,
    };
    // The cycle's job owns this core tree; the command builder null-redirects
    // stdout/stderr so config bytes never reach a log.
    let mut guard = match ChildGuard::spawn(&snapshot, Some(ctx.job)) {
        Ok(guard) => guard,
        Err(error) => {
            files.cleanup();
            return Err(error.into());
        }
    };

    // One cleanup scope for every exit path: stop the core, then remove both
    // sidecars (`.json` config and decimal-PID `.pid`).
    let probed: Result<Option<ProbeSample>> = async {
        files.write_pid(guard.id())?;
        wait_for_port(port, Duration::from_millis(XRAY_STARTUP_TIMEOUT_MS))?;
        if *ctx.cancel.borrow() {
            return Ok(None);
        }
        let timeout = Duration::from_millis(settings.test_timeout_ms.max(1) as u64);
        let proxy_url = format!("http://127.0.0.1:{port}");
        let latency_ms = measure_delay(&proxy_url, &settings.ping_url, timeout).await;

        let mut service_results = Vec::with_capacity(services.len());
        for service in services {
            // Malformed stored JSON yields no URLs; the check then fails the
            // service without exposing its contents.
            let urls = serde_json::from_str::<Vec<String>>(&service.test_urls).unwrap_or_default();
            let reachable = check_service_profile(&urls, &proxy_url, timeout).await;
            service_results.push(ProbeServiceResult {
                service_profile_id: service.id.clone(),
                reachable,
            });
        }
        Ok(Some(ProbeSample {
            profile_id: profile.id.clone(),
            latency_ms,
            service_results,
        }))
    }
    .await;

    let stopped = guard.stop();
    files.cleanup();
    let sample = probed?;
    stopped?;
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use serde_json::json;
    use tokio::sync::watch;

    use super::*;
    use crate::storage::migrate;
    use crate::storage::models::Subscription;
    use crate::storage::repos::{profile_repo, service_profile_repo, subscription_repo};

    struct FakeTransport {
        delays: Mutex<VecDeque<Option<u64>>>,
    }

    impl ProbeTransport for FakeTransport {
        fn probe_profile<'a>(
            &'a self,
            profile: &'a Profile,
            services: &'a [ServiceProfile],
            _settings: &'a Settings,
            _ctx: ProbeContext<'a>,
        ) -> ProbeFutureOne<'a> {
            Box::pin(async move {
                let latency_ms = self
                    .delays
                    .lock()
                    .expect("lock delays")
                    .pop_front()
                    .flatten();
                Ok(Some(ProbeSample {
                    profile_id: profile.id.clone(),
                    latency_ms,
                    service_results: services
                        .iter()
                        .map(|service| ProbeServiceResult {
                            service_profile_id: service.id.clone(),
                            reachable: true,
                        })
                        .collect(),
                }))
            })
        }
    }

    /// Tracks peak concurrency and fails `p-1` without failing the cycle.
    struct CountingTransport {
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
    }

    impl CountingTransport {
        fn new() -> Self {
            Self {
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
            }
        }

        fn max_in_flight(&self) -> usize {
            self.max_in_flight.load(Ordering::SeqCst)
        }
    }

    impl ProbeTransport for CountingTransport {
        fn probe_profile<'a>(
            &'a self,
            profile: &'a Profile,
            _services: &'a [ServiceProfile],
            _settings: &'a Settings,
            _ctx: ProbeContext<'a>,
        ) -> ProbeFutureOne<'a> {
            Box::pin(async move {
                let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(current, Ordering::SeqCst);
                tokio::task::yield_now().await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                Ok(Some(ProbeSample {
                    profile_id: profile.id.clone(),
                    latency_ms: if profile.id == "p-1" { None } else { Some(1) },
                    service_results: Vec::new(),
                }))
            })
        }
    }

    /// Simulates one temporary core per profile. Sidecars hold only a decimal
    /// PID; every started core is stopped exactly once and both files removed.
    #[derive(Default)]
    struct CancellingTransport {
        started: Mutex<Vec<(PathBuf, PathBuf)>>,
        stops: Mutex<Vec<String>>,
    }

    impl ProbeTransport for CancellingTransport {
        fn probe_profile<'a>(
            &'a self,
            profile: &'a Profile,
            _services: &'a [ServiceProfile],
            _settings: &'a Settings,
            ctx: ProbeContext<'a>,
        ) -> ProbeFutureOne<'a> {
            Box::pin(async move {
                if *ctx.cancel.borrow() {
                    return Ok(None);
                }
                let files = ProbeTempFiles::new().expect("temp files");
                std::fs::write(&files.config_path, "{}").expect("write config");
                files.write_pid(4242).expect("write pid");
                self.started
                    .lock()
                    .expect("lock started")
                    .push((files.config_path.clone(), files.pid_path.clone()));
                tokio::task::yield_now().await; // stand-in for the port wait
                let cancelled = *ctx.cancel.borrow();
                self.stops
                    .lock()
                    .expect("lock stops")
                    .push(profile.id.clone());
                files.cleanup();
                if cancelled {
                    Ok(None)
                } else {
                    Ok(Some(ProbeSample {
                        profile_id: profile.id.clone(),
                        latency_ms: Some(7),
                        service_results: Vec::new(),
                    }))
                }
            })
        }
    }

    struct RecordingProgress {
        servers: Mutex<Vec<ProbeServerEvent>>,
        progress: Mutex<Vec<(usize, usize)>>,
        cancelled: AtomicUsize,
        finished: Mutex<Vec<ProbeOutcome>>,
        cancel_on_done: Option<watch::Sender<bool>>,
    }

    impl RecordingProgress {
        fn new() -> Self {
            Self {
                servers: Mutex::new(Vec::new()),
                progress: Mutex::new(Vec::new()),
                cancelled: AtomicUsize::new(0),
                finished: Mutex::new(Vec::new()),
                cancel_on_done: None,
            }
        }

        fn cancelling(tx: watch::Sender<bool>) -> Self {
            Self {
                cancel_on_done: Some(tx),
                ..Self::new()
            }
        }
    }

    impl ProbeProgress for RecordingProgress {
        fn on_server(&self, event: &ProbeServerEvent) {
            if event.status == ProbeServerStatus::Done {
                if let Some(tx) = &self.cancel_on_done {
                    let _ = tx.send(true);
                }
            }
            self.servers
                .lock()
                .expect("lock servers")
                .push(event.clone());
        }

        fn on_progress(&self, tested: usize, total: usize) {
            self.progress
                .lock()
                .expect("lock progress")
                .push((tested, total));
        }

        fn on_cancelled(&self) {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }

        fn on_finished(&self, outcome: &ProbeOutcome) {
            self.finished
                .lock()
                .expect("lock finished")
                .push(outcome.clone());
        }
    }

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        conn
    }

    fn seeded_conn() -> Connection {
        let conn = test_conn();
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
        profile_repo::upsert_many(
            &conn,
            &[
                test_profile("p-failed", "failed", 10),
                test_profile("p-slow", "slow", 20),
                test_profile("p-fast", "fast", 30),
            ],
        )
        .expect("insert profiles");
        service_profile_repo::insert(
            &conn,
            &ServiceProfile {
                id: "svc-1".to_string(),
                name: "test-service".to_string(),
                test_urls: json!(["https://service.test"]).to_string(),
                is_enabled: true,
                ..Default::default()
            },
        )
        .expect("insert service profile");
        conn
    }

    fn fake_transport() -> FakeTransport {
        FakeTransport {
            delays: Mutex::new(VecDeque::from([None, Some(20), Some(10)])),
        }
    }

    fn test_profile(id: &str, server_id: &str, sort_index: i64) -> Profile {
        Profile {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            server_id: server_id.to_string(),
            link: format!("vless://{server_id}@node.example.com:443#node"),
            protocol: Some("vless".to_string()),
            address: Some("node.example.com".to_string()),
            port: Some(443),
            raw_config: json!({"security": "none", "type": "raw"}).to_string(),
            sort_index,
            ..Default::default()
        }
    }

    fn sort_indices(conn: &Connection) -> Vec<(String, i64)> {
        profile_repo::list_present(conn)
            .expect("list profiles")
            .iter()
            .map(|profile| (profile.id.clone(), profile.sort_index))
            .collect()
    }

    #[tokio::test]
    async fn parallel_cycle_caps_concurrency_and_isolates_failures() {
        let profiles: Vec<Profile> = (0..25)
            .map(|index| {
                test_profile(
                    &format!("p-{index}"),
                    &format!("srv-{index}"),
                    index as i64,
                )
            })
            .collect();
        let settings = Settings {
            test_concurrency: 3,
            ..Settings::default()
        };
        let transport = CountingTransport::new();
        let (tx, rx) = watch::channel(false);

        let samples = probe_cycle_detached(
            &profiles,
            &[],
            &settings,
            &transport,
            &SilentProgress,
            rx,
        )
        .await
        .expect("probe cycle");

        assert_eq!(transport.max_in_flight(), 3);
        assert_eq!(samples.len(), 25);
        assert_eq!(
            samples
                .iter()
                .find(|sample| sample.profile_id == "p-1")
                .expect("failed sample is still returned")
                .latency_ms,
            None
        );
        assert!(
            samples
                .iter()
                .filter(|sample| sample.profile_id != "p-1")
                .all(|sample| sample.latency_ms.is_some())
        );
        let _ = tx;
    }

    #[tokio::test]
    async fn cancelled_cycle_persists_partial_samples_without_sorting() {
        let mut conn = seeded_conn();
        let profiles = profile_repo::list_present_enabled(&conn).expect("list enabled");
        let services = service_profile_repo::list_enabled(&conn).expect("list services");
        let before = sort_indices(&conn);
        let settings = Settings {
            test_concurrency: 1,
            ..Settings::default()
        };
        let (tx, rx) = watch::channel(false);
        let transport = CancellingTransport::default();
        let progress = RecordingProgress::cancelling(tx);

        let samples = probe_cycle_detached(
            &profiles,
            &services,
            &settings,
            &transport,
            &progress,
            rx,
        )
        .await
        .expect("probe cycle");
        assert_eq!(samples.len(), 1, "only the completed profile is returned");

        let outcome = persist_probe_samples(
            &mut conn,
            &samples,
            &progress,
            PersistOptions {
                sort: false,
                cancelled: true,
            },
        )
        .expect("persist partial samples");
        assert_eq!(outcome.tested, 1);
        assert_eq!(outcome.sorted, 0);
        assert!(outcome.cancelled);
        assert_eq!(sort_indices(&conn), before, "sort must be skipped");
        assert_eq!(
            progress.cancelled.load(Ordering::SeqCst),
            1,
            "cancelled event must fire once"
        );

        let started = transport.started.lock().expect("lock started");
        assert_eq!(started.len(), 1, "only one core was started");
        for (config, pid) in started.iter() {
            assert!(!config.exists(), "config sidecar removed");
            assert!(!pid.exists(), "pid sidecar removed");
        }
        let stops = transport.stops.lock().expect("lock stops");
        assert_eq!(stops.len(), 1, "started core stopped exactly once");
    }

    #[tokio::test]
    async fn events_are_testing_then_done_and_progress_reaches_total() {
        let profiles: Vec<Profile> = (0..4)
            .map(|index| {
                test_profile(
                    &format!("p-{index}"),
                    &format!("srv-{index}"),
                    index as i64,
                )
            })
            .collect();
        let progress = RecordingProgress::new();
        let (_tx, rx) = watch::channel(false);

        let samples = probe_cycle_detached(
            &profiles,
            &[],
            &Settings::default(),
            &CountingTransport::new(),
            &progress,
            rx,
        )
        .await
        .expect("probe cycle");
        assert_eq!(samples.len(), 4);

        let events = progress.servers.lock().expect("lock servers");
        for profile in &profiles {
            let statuses: Vec<ProbeServerStatus> = events
                .iter()
                .filter(|event| event.profile_id == profile.id)
                .map(|event| event.status.clone())
                .collect();
            assert_eq!(statuses.len(), 2, "one testing and one terminal event");
            assert_eq!(statuses[0], ProbeServerStatus::Testing);
            assert!(
                statuses[1] == ProbeServerStatus::Done || statuses[1] == ProbeServerStatus::Failed
            );
        }
        assert_eq!(
            progress.progress.lock().expect("lock progress").last(),
            Some(&(4, 4))
        );
    }

    #[tokio::test]
    async fn probe_cycle_sorts_failed_last() {
        let mut conn = seeded_conn();
        let outcome =
            run_probe_cycle_with(&mut conn, &Settings::default(), &fake_transport(), &SilentProgress)
                .await
                .expect("probe cycle");

        assert_eq!(outcome.tested, 3);
        assert_eq!(outcome.sorted, 3);
        assert!(!outcome.cancelled);

        let profiles = profile_repo::list_present(&conn).expect("list profiles");
        let ids: Vec<&str> = profiles.iter().map(|profile| profile.id.as_str()).collect();
        assert_eq!(ids, vec!["p-fast", "p-slow", "p-failed"]);
        let sort_indices: Vec<i64> = profiles.iter().map(|profile| profile.sort_index).collect();
        assert_eq!(sort_indices, vec![10, 20, 30]);
        let latencies: Vec<Option<i64>> =
            profiles.iter().map(|profile| profile.latency_ms).collect();
        assert_eq!(latencies, vec![Some(10), Some(20), None]);
    }

    #[tokio::test]
    async fn probe_cycle_writes_service_results() {
        let mut conn = seeded_conn();
        run_probe_cycle_with(
            &mut conn,
            &Settings::default(),
            &fake_transport(),
            &SilentProgress,
        )
        .await
        .expect("probe cycle");

        let results = profile_repo::list_service_results(&conn).expect("list service results");
        assert_eq!(results.len(), 3);
        assert!(
            results
                .iter()
                .all(|result| result.service_profile_id == "svc-1" && result.reachable),
            "every probed profile must store its active service result"
        );
    }

    #[tokio::test]
    async fn probe_cycle_reports_progress() {
        let mut conn = seeded_conn();
        let progress = RecordingProgress::new();
        let outcome = run_probe_cycle_with(
            &mut conn,
            &Settings::default(),
            &fake_transport(),
            &progress,
        )
        .await
        .expect("probe cycle");

        assert_eq!(
            progress.progress.lock().expect("lock progress").as_slice(),
            &[(1, 3), (2, 3), (3, 3)]
        );
        assert_eq!(
            progress.finished.lock().expect("lock finished").as_slice(),
            &[outcome]
        );
    }
}
