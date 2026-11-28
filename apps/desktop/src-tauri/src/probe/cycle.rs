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
    Cancelled,
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
    /// Probe config/PID base. `None` uses the system temp dir (Windows);
    /// Android injects the app cache dir at bootstrap.
    pub temp_dir: Option<&'a std::path::Path>,
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
/// Progress counts real samples only — cancelled profiles are excluded.
pub async fn probe_cycle_detached(
    profiles: &[Profile],
    services: &[ServiceProfile],
    settings: &Settings,
    transport: &dyn ProbeTransport,
    progress: &dyn ProbeProgress,
    cancel: watch::Receiver<bool>,
    temp_dir: Option<&std::path::Path>,
) -> Result<Vec<ProbeSample>> {
    let total = profiles.len();
    let job = JobHandle::create()?;
    let ctx = ProbeContext {
        cancel: &cancel,
        job: &job,
        temp_dir,
    };
    let limit = settings.test_concurrency.clamp(1, 20) as usize;
    let mut samples = Vec::with_capacity(total);
    let mut tested = 0usize;

    // Futures are lazy: building them all up front costs nothing and gives
    // `buffer_unordered` a concrete future type (no higher-ranked closure).
    let futures: Vec<ProbeFutureOne<'_>> = profiles
        .iter()
        .map(|profile| tracked_probe(profile, services, settings, transport, progress, ctx))
        .collect();
    let mut ordered = stream::iter(futures).buffer_unordered(limit);

    while let Some(result) = ordered.next().await {
        // Only `JobHandle::create()` can fail at cycle level; per-profile
        // failures have already become failed samples in `tracked_probe`.
        if let Some(sample) = result? {
            samples.push(sample);
            tested += 1;
            progress.on_progress(tested, total);
        }
    }
    // `probe-cancelled` is emitted by `probe_cancel` when the stop is accepted,
    // so a late cancel cannot race this loop and lose the event.
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
        // Exactly one terminal event per started profile. A cancelled future
        // gets `cancelled`; any per-profile error becomes a failed sample so a
        // single bad core never aborts the cycle or discards prior samples.
        let (sample, status) = match transport
            .probe_profile(profile, services, settings, ctx)
            .await
        {
            Ok(Some(sample)) => {
                let status = if sample.latency_ms.is_some() {
                    ProbeServerStatus::Done
                } else {
                    ProbeServerStatus::Failed
                };
                (Some(sample), status)
            }
            Ok(None) => (None, ProbeServerStatus::Cancelled),
            Err(_) => (Some(failed_sample(profile, services)), ProbeServerStatus::Failed),
        };
        progress.on_server(&ProbeServerEvent {
            profile_id,
            status,
            latency_ms: sample.as_ref().and_then(|sample| sample.latency_ms),
            service_results: sample
                .as_ref()
                .map(|sample| sample.service_results.clone())
                .unwrap_or_default(),
        });
        Ok(sample)
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
        probe_cycle_detached(&profiles, &services, settings, transport, progress, rx, None).await?;
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
        // `probe_cancel` owns this event and emits it exactly once when the
        // stop is accepted. Keep the trait method but never emit here, so a
        // future caller cannot resurrect a duplicate `probe-cancelled`.
    }

    fn on_finished(&self, outcome: &ProbeOutcome) {
        crate::state::emit("probe-finished", outcome.clone());
    }
}

/// Reserve one ephemeral loopback port without logging it. The listener is
/// returned to the caller and kept bound while the config is prepared, so two
/// concurrent futures cannot be handed the same port. It is released just
/// before spawn so the core itself can bind.
fn reserve_port() -> Result<(TcpListener, u16)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
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

/// One profile's core work. Every error here is local to the profile; the
/// wrapper maps it to a failed sample. Cancellation stays `Ok(None)`.
async fn probe_profile_with_core(
    profile: &Profile,
    services: &[ServiceProfile],
    settings: &Settings,
    ctx: ProbeContext<'_>,
) -> Result<Option<ProbeSample>> {
    if *ctx.cancel.borrow() {
        return Ok(None);
    }
    match probe_core_once(profile, services, settings, ctx).await {
        Ok(sample) => Ok(sample),
        Err(_) => Ok(Some(failed_sample(profile, services))),
    }
}

/// HTTP half of one probe: the two delay GETs plus every active service check,
/// all through `proxy_url`. Cancellation is observed while requests are in
/// flight — spec §2 requires checking the flag between GETs — so `Stop test`
/// never waits out `test_timeout_ms` per core. Returns `None` on cancel; the
/// caller still stops the core and removes both sidecars.
async fn probe_http_phase(
    profile: &Profile,
    services: &[ServiceProfile],
    settings: &Settings,
    proxy_url: &str,
    mut cancel: watch::Receiver<bool>,
) -> Option<ProbeSample> {
    if *cancel.borrow() {
        return None;
    }
    let timeout = Duration::from_millis(settings.test_timeout_ms.max(1) as u64);
    let http = async {
        let latency_ms = measure_delay(proxy_url, &settings.ping_url, timeout).await;
        let mut service_results = Vec::with_capacity(services.len());
        for service in services {
            // Malformed stored JSON yields no URLs; the check then fails the
            // service without exposing its contents.
            let urls = serde_json::from_str::<Vec<String>>(&service.test_urls).unwrap_or_default();
            let reachable = check_service_profile(&urls, proxy_url, timeout).await;
            service_results.push(ProbeServiceResult {
                service_profile_id: service.id.clone(),
                reachable,
            });
        }
        ProbeSample {
            profile_id: profile.id.clone(),
            latency_ms,
            service_results,
        }
    };

    let mut http = Box::pin(http);
    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() {
                    // Sender gone: no further stop can arrive, finish HTTP.
                    return Some(http.await);
                }
                if *cancel.borrow() {
                    return None;
                }
            }
            sample = &mut http => return Some(sample),
        }
    }
}

async fn probe_core_once(
    profile: &Profile,
    services: &[ServiceProfile],
    settings: &Settings,
    ctx: ProbeContext<'_>,
) -> Result<Option<ProbeSample>> {
    let (listener, port) = reserve_port()?;
    let config = build_probe_config(profile, port)?;

    let system_temp = std::env::temp_dir();
    let files = ProbeTempFiles::new_in(ctx.temp_dir.unwrap_or(system_temp.as_path()))?;
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
    // The reservation ends here so xray can bind the port. It covered config
    // preparation throughout, so a concurrent probe could not claim this port.
    drop(listener);
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
        let proxy_url = format!("http://127.0.0.1:{port}");
        // A fresh receiver is registered with `changed()` inside the helper so
        // a stop landing at any point of the HTTP phase is observed immediately.
        Ok(probe_http_phase(
            profile,
            services,
            settings,
            &proxy_url,
            (*ctx.cancel).clone(),
        )
        .await)
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
                let system_temp = std::env::temp_dir();
                let files = ProbeTempFiles::new_in(
                    ctx.temp_dir.unwrap_or(system_temp.as_path()),
                )
                .expect("temp files");
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

    /// Every profile fails at the transport boundary (stand-in for a port
    /// timeout, spawn failure or stopped core).
    struct FailingTransport;

    impl ProbeTransport for FailingTransport {
        fn probe_profile<'a>(
            &'a self,
            _profile: &'a Profile,
            _services: &'a [ServiceProfile],
            _settings: &'a Settings,
            _ctx: ProbeContext<'a>,
        ) -> ProbeFutureOne<'a> {
            Box::pin(async { Err(ProbeError::Io(std::io::Error::other("core failed"))) })
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
            None,
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
    async fn per_profile_errors_become_failed_samples_without_aborting_cycle() {
        let profiles: Vec<Profile> = (0..3)
            .map(|index| {
                test_profile(
                    &format!("p-{index}"),
                    &format!("srv-{index}"),
                    index as i64,
                )
            })
            .collect();
        let services = vec![ServiceProfile {
            id: "svc-1".to_string(),
            name: "test-service".to_string(),
            test_urls: json!(["https://service.test"]).to_string(),
            is_enabled: true,
            ..Default::default()
        }];
        let progress = RecordingProgress::new();
        let (_tx, rx) = watch::channel(false);

        let samples = probe_cycle_detached(
            &profiles,
            &services,
            &Settings::default(),
            &FailingTransport,
            &progress,
            rx,
            None,
        )
        .await
        .expect("a failing profile must not abort the cycle");

        assert_eq!(samples.len(), 3);
        assert!(samples.iter().all(|sample| sample.latency_ms.is_none()));
        assert!(
            samples
                .iter()
                .all(|sample| sample.service_results.iter().all(|result| !result.reachable))
        );
        assert_eq!(
            progress.progress.lock().expect("lock progress").last(),
            Some(&(3, 3))
        );
        let events = progress.servers.lock().expect("lock servers");
        for profile in &profiles {
            let statuses: Vec<ProbeServerStatus> = events
                .iter()
                .filter(|event| event.profile_id == profile.id)
                .map(|event| event.status.clone())
                .collect();
            assert_eq!(
                statuses,
                vec![ProbeServerStatus::Testing, ProbeServerStatus::Failed]
            );
        }
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
            None,
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
            progress.progress.lock().expect("lock progress").as_slice(),
            &[(1, 3)],
            "progress counts real samples only"
        );
        assert_eq!(
            progress.cancelled.load(Ordering::SeqCst),
            0,
            "the cycle must not emit probe-cancelled; probe_cancel does"
        );

        let events = progress.servers.lock().expect("lock servers");
        let tested_id = profiles.first().expect("at least one profile").id.clone();
        for profile in &profiles {
            let statuses: Vec<ProbeServerStatus> = events
                .iter()
                .filter(|event| event.profile_id == profile.id)
                .map(|event| event.status.clone())
                .collect();
            let expected = if profile.id == tested_id {
                vec![ProbeServerStatus::Testing, ProbeServerStatus::Done]
            } else {
                vec![ProbeServerStatus::Testing, ProbeServerStatus::Cancelled]
            };
            assert_eq!(statuses, expected, "every started profile needs a terminal event");
        }
        drop(events);

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
            None,
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

    /// A stop fired while the delay/service GETs are in flight must abort the
    /// HTTP phase promptly instead of waiting out `test_timeout_ms` per core.
    #[tokio::test]
    async fn cancellation_during_http_phase_aborts_probe() {
        use crate::probe::test_support::spawn_mock_proxy;

        // `silent` proxy accepts the connection but never replies, so the HTTP
        // phase cannot finish on its own within the test's deadline.
        let proxy = spawn_mock_proxy(&[], true);
        let settings = Settings {
            test_timeout_ms: 5000,
            ..Settings::default()
        };
        let profile = test_profile("p-1", "srv-1", 0);
        let services: Vec<ServiceProfile> = Vec::new();
        let (tx, rx) = watch::channel(false);
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            tx.send(true).expect("send cancellation");
        });

        let result = tokio::time::timeout(
            Duration::from_millis(400),
            probe_http_phase(&profile, &services, &settings, &proxy, rx),
        )
        .await
        .expect("HTTP phase must observe cancellation promptly");
        assert!(result.is_none(), "cancelled HTTP phase yields no sample");
        stopper.await.expect("stopper task");
    }
}
