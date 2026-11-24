//! Probe cycle: batched xray speed-test, delay + service checks, result
//! persistence and sorting. Testable via [`ProbeTransport`] injection.

use std::future::Future;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::config::build_speedtest_config;
use crate::core::snapshot::ConfigSnapshot;
use crate::core::supervisor::{build_command, wait_for_port, ChildGuard};
use crate::probe::{check_service_profile, measure_delay};
use crate::storage::models::{Profile, ServiceProfile, Settings};
use crate::storage::repos::{profile_repo, service_profile_repo};

/// Profiles probed per temporary xray core.
pub const PROBE_BATCH_SIZE: usize = 20;

/// Loopback inbounds must accept connections before any HTTP probe starts.
const PORT_READY_TIMEOUT: Duration = Duration::from_secs(2);

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

/// Command result; serialized to the TS `ProbeResult` (`finishedAt`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    pub tested: usize,
    pub sorted: usize,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeServiceResult {
    pub service_profile_id: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSample {
    pub profile_id: String,
    pub latency_ms: Option<u64>,
    pub service_results: Vec<ProbeServiceResult>,
}

pub type ProbeFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<ProbeSample>>> + Send + 'a>>;

/// Boundary between the cycle and the slow/external work. Production probes
/// one batch through a temporary xray core; tests inject a deterministic double.
pub trait ProbeTransport: Send + Sync {
    fn probe_batch<'a>(
        &'a self,
        profiles: &'a [Profile],
        services: &'a [ServiceProfile],
        settings: &'a Settings,
    ) -> ProbeFuture<'a>;
}

pub trait ProbeProgress: Send + Sync {
    fn on_batch(&self, tested: usize, total: usize);
    fn on_finished(&self, outcome: &ProbeOutcome);
}

pub struct SilentProgress;

impl ProbeProgress for SilentProgress {
    fn on_batch(&self, _tested: usize, _total: usize) {}
    fn on_finished(&self, _outcome: &ProbeOutcome) {}
}

pub async fn run_probe_cycle(conn: &mut Connection, settings: &Settings) -> Result<ProbeOutcome> {
    run_probe_cycle_with(conn, settings, &XrayProbeTransport, &TauriProgress).await
}

pub async fn run_probe_cycle_with(
    conn: &mut Connection,
    settings: &Settings,
    transport: &dyn ProbeTransport,
    progress: &dyn ProbeProgress,
) -> Result<ProbeOutcome> {
    let profiles = profile_repo::list_present(conn)?;
    let services = service_profile_repo::list_enabled(conn)?;
    let total = profiles.len();

    let mut samples = Vec::with_capacity(total);
    for chunk in profiles.chunks(PROBE_BATCH_SIZE) {
        let mut batch = transport.probe_batch(chunk, &services, settings).await?;
        samples.append(&mut batch);
        progress.on_batch(samples.len(), total);
    }

    let checked_at = Utc::now();
    for sample in &samples {
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

    let outcome = ProbeOutcome {
        tested: samples.len(),
        sorted: ordered_ids.len(),
        finished_at: checked_at,
    };
    progress.on_finished(&outcome);
    Ok(outcome)
}

fn passes_all(sample: &ProbeSample) -> bool {
    sample.latency_ms.is_some() && sample.service_results.iter().all(|result| result.reachable)
}

/// Real transport: one temporary xray core per batch, explicit loopback proxy
/// for every HTTP request, core stopped and config removed even on error.
pub struct XrayProbeTransport;

impl ProbeTransport for XrayProbeTransport {
    fn probe_batch<'a>(
        &'a self,
        profiles: &'a [Profile],
        services: &'a [ServiceProfile],
        settings: &'a Settings,
    ) -> ProbeFuture<'a> {
        Box::pin(async move { probe_batch_with_core(profiles, services, settings).await })
    }
}

/// Emit progress/finished events through Tauri when the handle was installed.
pub struct TauriProgress;

impl ProbeProgress for TauriProgress {
    fn on_batch(&self, tested: usize, total: usize) {
        crate::state::emit(
            "probe-progress",
            json!({ "tested": tested, "total": total }),
        );
    }

    fn on_finished(&self, outcome: &ProbeOutcome) {
        crate::state::emit("probe-finished", outcome.clone());
    }
}

/// Reserve distinct ephemeral loopback ports without logging them.
fn free_ports(count: usize) -> Result<Vec<u16>> {
    let mut listeners = Vec::with_capacity(count);
    for _ in 0..count {
        listeners.push(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?);
    }
    listeners
        .iter()
        .map(|listener| Ok(listener.local_addr()?.port()))
        .collect()
}

fn temp_config_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mglaroute-probe-{}-{nanos}.json",
        std::process::id()
    ))
}

async fn probe_batch_with_core(
    profiles: &[Profile],
    services: &[ServiceProfile],
    settings: &Settings,
) -> Result<Vec<ProbeSample>> {
    if profiles.is_empty() {
        return Ok(Vec::new());
    }

    let ports = free_ports(profiles.len())?;
    let config = build_speedtest_config(profiles, &ports)?;
    let config_path = temp_config_path();
    let snapshot = ConfigSnapshot {
        subscription_id: profiles[0].subscription_id.clone(),
        profile: profiles[0].clone(),
        proxy_port: ports[0],
        config_path: config_path.clone(),
        anti_dpi: None,
    };
    // `build_command` null-redirects stdout/stderr so config credentials never
    // reach a log sink. It only needs the path, not the file itself.
    let mut command = build_command(&snapshot)?;
    std::fs::write(&config_path, serde_json::to_string(&config)?)?;

    let mut guard = match command.spawn() {
        Ok(child) => ChildGuard::new(child),
        Err(error) => {
            let _ = std::fs::remove_file(&config_path);
            return Err(error.into());
        }
    };
    let probed = probe_through_ports(profiles, services, settings, &ports).await;
    let stopped = guard.stop();
    let _ = std::fs::remove_file(&config_path);

    let samples = probed?;
    stopped?;
    Ok(samples)
}

async fn probe_through_ports(
    profiles: &[Profile],
    services: &[ServiceProfile],
    settings: &Settings,
    ports: &[u16],
) -> Result<Vec<ProbeSample>> {
    for &port in ports {
        wait_for_port(port, PORT_READY_TIMEOUT)?;
    }
    let timeout = Duration::from_millis(settings.test_timeout_ms.max(1) as u64);

    let mut samples = Vec::with_capacity(profiles.len());
    for (profile, port) in profiles.iter().zip(ports.iter()) {
        let proxy_url = format!("http://127.0.0.1:{port}");
        let latency_ms = measure_delay(&proxy_url, &settings.ping_url, timeout).await;

        let mut service_results = Vec::with_capacity(services.len());
        for service in services {
            // Malformed stored JSON yields no URLs; check_service_profile then
            // fails the service without exposing its contents.
            let urls = serde_json::from_str::<Vec<String>>(&service.test_urls).unwrap_or_default();
            let reachable = check_service_profile(&urls, &proxy_url, timeout).await;
            service_results.push(ProbeServiceResult {
                service_profile_id: service.id.clone(),
                reachable,
            });
        }
        samples.push(ProbeSample {
            profile_id: profile.id.clone(),
            latency_ms,
            service_results,
        });
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::storage::migrate;
    use crate::storage::models::Subscription;
    use crate::storage::repos::{profile_repo, service_profile_repo, subscription_repo};

    struct FakeTransport {
        delays: Mutex<VecDeque<Option<u64>>>,
    }

    impl ProbeTransport for FakeTransport {
        fn probe_batch<'a>(
            &'a self,
            profiles: &'a [Profile],
            services: &'a [ServiceProfile],
            _settings: &'a Settings,
        ) -> ProbeFuture<'a> {
            Box::pin(async move {
                let mut samples = Vec::with_capacity(profiles.len());
                for profile in profiles {
                    let latency_ms = self
                        .delays
                        .lock()
                        .expect("lock delays")
                        .pop_front()
                        .flatten();
                    let service_results = services
                        .iter()
                        .map(|service| ProbeServiceResult {
                            service_profile_id: service.id.clone(),
                            reachable: true,
                        })
                        .collect();
                    samples.push(ProbeSample {
                        profile_id: profile.id.clone(),
                        latency_ms,
                        service_results,
                    });
                }
                Ok(samples)
            })
        }
    }

    struct RecordingProgress {
        batches: Mutex<Vec<(usize, usize)>>,
        finished: Mutex<Vec<ProbeOutcome>>,
    }

    impl Default for RecordingProgress {
        fn default() -> Self {
            Self {
                batches: Mutex::new(Vec::new()),
                finished: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProbeProgress for RecordingProgress {
        fn on_batch(&self, tested: usize, total: usize) {
            self.batches
                .lock()
                .expect("lock batches")
                .push((tested, total));
        }

        fn on_finished(&self, outcome: &ProbeOutcome) {
            self.finished
                .lock()
                .expect("lock finished")
                .push(outcome.clone());
        }
    }

    struct CountingTransport {
        batch_sizes: Mutex<Vec<usize>>,
    }

    impl ProbeTransport for CountingTransport {
        fn probe_batch<'a>(
            &'a self,
            profiles: &'a [Profile],
            services: &'a [ServiceProfile],
            _settings: &'a Settings,
        ) -> ProbeFuture<'a> {
            Box::pin(async move {
                self.batch_sizes
                    .lock()
                    .expect("lock batch sizes")
                    .push(profiles.len());
                Ok(profiles
                    .iter()
                    .map(|profile| ProbeSample {
                        profile_id: profile.id.clone(),
                        latency_ms: Some(1),
                        service_results: services
                            .iter()
                            .map(|service| ProbeServiceResult {
                                service_profile_id: service.id.clone(),
                                reachable: true,
                            })
                            .collect(),
                    })
                    .collect())
            })
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

    #[tokio::test]
    async fn probe_cycle_sorts_failed_last() {
        let mut conn = seeded_conn();
        let transport = fake_transport();
        let outcome =
            run_probe_cycle_with(&mut conn, &Settings::default(), &transport, &SilentProgress)
                .await
                .expect("probe cycle");

        assert_eq!(outcome.tested, 3);
        assert_eq!(outcome.sorted, 3);

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
        let progress = RecordingProgress::default();
        let outcome = run_probe_cycle_with(
            &mut conn,
            &Settings::default(),
            &fake_transport(),
            &progress,
        )
        .await
        .expect("probe cycle");

        assert_eq!(
            progress.batches.lock().expect("lock batches").as_slice(),
            &[(3, 3)]
        );
        assert_eq!(
            progress.finished.lock().expect("lock finished").as_slice(),
            &[outcome]
        );
    }

    #[tokio::test]
    async fn probe_cycle_batches_twenty_profiles() {
        let mut conn = test_conn();
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
        let profiles: Vec<Profile> = (0..21)
            .map(|index| test_profile(&format!("p-{index}"), &format!("srv-{index}"), index))
            .collect();
        profile_repo::upsert_many(&conn, &profiles).expect("insert profiles");

        let transport = CountingTransport {
            batch_sizes: Mutex::new(Vec::new()),
        };
        let outcome =
            run_probe_cycle_with(&mut conn, &Settings::default(), &transport, &SilentProgress)
                .await
                .expect("probe cycle");

        assert_eq!(outcome.tested, 21);
        assert_eq!(
            transport
                .batch_sizes
                .lock()
                .expect("lock batches")
                .as_slice(),
            &[20, 1]
        );
    }
}
