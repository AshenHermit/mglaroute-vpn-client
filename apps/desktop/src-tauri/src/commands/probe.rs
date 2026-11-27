use std::collections::HashSet;

use chrono::{DateTime, Utc};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use tauri::State;
use tokio::sync::watch;

use super::connection::connect_impl;
use super::{AppState, CommandError, ConnectionStatus};
use crate::probe::cycle::{
    persist_probe_samples, probe_cycle_detached, PersistOptions, ProbeOutcome, ProbeProgress,
    ProbeServerEvent, ProbeServerStatus, ProbeServiceResult, TauriProgress, XrayProbeTransport,
};
use crate::reconnect::{decide_reconnect, ReconnectDecision};
use crate::storage::models::{Profile, Settings};
use crate::storage::repos::{profile_repo, service_profile_repo, settings_repo};

/// CamelCase mirror of the TS `Profile` contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePayload {
    pub id: String,
    pub subscription_id: String,
    pub server_id: String,
    pub link: String,
    pub remarks: Option<String>,
    pub protocol: Option<String>,
    pub address: Option<String>,
    pub port: Option<i64>,
    pub raw_config: String,
    pub present: bool,
    pub latency_ms: Option<i64>,
    pub reachable: bool,
    pub checked_at: Option<DateTime<Utc>>,
    pub sort_index: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Profile> for ProfilePayload {
    fn from(value: Profile) -> Self {
        Self {
            id: value.id,
            subscription_id: value.subscription_id,
            server_id: value.server_id,
            link: value.link,
            remarks: value.remarks,
            protocol: value.protocol,
            address: value.address,
            port: value.port,
            raw_config: value.raw_config,
            present: value.present,
            latency_ms: value.latency_ms,
            reachable: value.reachable,
            checked_at: value.checked_at,
            sort_index: value.sort_index,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// Reconnect candidates from persisted probe results: `(id, delay, passes_all)`.
/// A candidate needs a positive delay and a reachable result for every active
/// service profile. An empty active set means delay alone decides.
pub fn candidates(conn: &Connection) -> rusqlite::Result<Vec<(String, u64, bool)>> {
    let profiles = profile_repo::list_present_enabled(conn)?;
    let services = service_profile_repo::list_enabled(conn)?;
    let results = profile_repo::list_service_results(conn)?;
    let active: HashSet<&str> = services.iter().map(|service| service.id.as_str()).collect();

    Ok(profiles
        .into_iter()
        .map(|profile| {
            let latency = profile
                .latency_ms
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let passes = latency.is_some()
                && active.iter().all(|service_id| {
                    results.iter().any(|result| {
                        result.profile_id == profile.id
                            && result.service_profile_id == *service_id
                            && result.reachable
                    })
                });
            (
                profile.id,
                latency.unwrap_or(u64::MAX),
                passes,
            )
        })
        .collect())
}

/// Emergency target when the connected server vanished from the active feed
/// and a fresh probe result is usable. Uses the current sample for the probed
/// server and persisted samples for everyone else; no cooldown and no
/// disconnect — that belongs to the caller.
pub(crate) fn missing_server_target(
    conn: &Connection,
    current_id: Option<&str>,
    event: &ProbeServerEvent,
) -> rusqlite::Result<Option<String>> {
    let Some(current_id) = current_id else {
        return Ok(None);
    };
    if profile_repo::list_present_enabled(conn)?
        .iter()
        .any(|profile| profile.id == current_id)
    {
        return Ok(None);
    }
    if event.status != ProbeServerStatus::Done {
        return Ok(None);
    }
    let Some(latency) = event.latency_ms.filter(|value| *value > 0) else {
        return Ok(None);
    };
    let services = service_profile_repo::list_enabled(conn)?;
    if !services.iter().all(|service| {
        event.service_results.iter().any(|result: &ProbeServiceResult| {
            result.service_profile_id == service.id && result.reachable
        })
    }) {
        return Ok(None);
    }

    let mut pool = candidates(conn)?;
    if let Some(entry) = pool.iter_mut().find(|(id, _, _)| id == &event.profile_id) {
        *entry = (event.profile_id.clone(), latency, true);
    }
    Ok(pool
        .into_iter()
        .filter(|(_, latency, passes)| *passes && *latency < u64::MAX)
        .min_by(|left, right| (left.1, &left.0).cmp(&(right.1, &right.0)))
        .map(|(id, _, _)| id))
}

/// Best passing server by ascending delay, then id; `None` if nothing passes.
pub fn best_eligible_profile(conn: &Connection) -> rusqlite::Result<Option<Profile>> {
    let passing: HashSet<String> = candidates(conn)?
        .into_iter()
        .filter(|(_, _, passes)| *passes)
        .map(|(id, _, _)| id)
        .collect();

    Ok(profile_repo::list_present_enabled(conn)?
        .into_iter()
        .filter(|profile| passing.contains(&profile.id))
        .min_by(|left, right| {
            (
                left.latency_ms.unwrap_or(i64::MAX),
                left.id.as_str(),
            )
                .cmp(&(
                    right.latency_ms.unwrap_or(i64::MAX),
                    right.id.as_str(),
                ))
        }))
}

/// Takes the connection permit itself so a probe tick can never race a manual
/// Connect: when the slot is busy the tick is skipped with `Ok(())`. The
/// `auto_reconnect_enabled` gate, the `Connected` gate and recording
/// `last_switch` belong here; the pure decision function owns neither.
pub(crate) async fn apply_reconnect(
    state: &AppState,
    settings: &Settings,
    candidates: &[(String, u64, bool)],
    now: DateTime<Utc>,
) -> Result<(), CommandError> {
    let Some(_permit) = state.try_connection_permit() else {
        return Ok(());
    };
    if !settings.auto_reconnect_enabled {
        return Ok(());
    }

    let (current_id, current_status, last_switch) = {
        let runtime = state.inner.runtime.lock().await;
        (
            runtime.connection.profile_id.clone(),
            runtime.connection.status,
            runtime.last_switch,
        )
    };
    if current_status != ConnectionStatus::Connected {
        return Ok(());
    }

    match decide_reconnect(
        current_id.as_deref(),
        candidates,
        settings,
        now,
        last_switch,
    ) {
        ReconnectDecision::Switch { profile_id } => {
            connect_impl(state, Some(profile_id)).await?;
            let mut runtime = state.inner.runtime.lock().await;
            runtime.last_switch = Some(now);
        }
        ReconnectDecision::None => {}
    }
    Ok(())
}

/// Switch immediately when the connected server has disappeared from the
/// active feed and this probe sample can name a usable replacement. Takes the
/// connection permit itself (skipping the tick when a manual connect owns it),
/// never disconnects first and never records `last_switch` — the cooldown is
/// for periodic reconnection, not for recovering from a vanished server.
pub(crate) async fn switch_if_current_missing(
    state: &AppState,
    settings: &Settings,
    event: &ProbeServerEvent,
) -> Result<(), CommandError> {
    let Some(_permit) = state.try_connection_permit() else {
        return Ok(());
    };
    if !settings.auto_reconnect_enabled {
        return Ok(());
    }

    let current_id = {
        let runtime = state.inner.runtime.lock().await;
        if runtime.connection.status != ConnectionStatus::Connected {
            return Ok(());
        }
        runtime.connection.profile_id.clone()
    };
    let target = {
        let conn = state.inner.conn.lock().await;
        missing_server_target(&conn, current_id.as_deref(), event)?
    };
    if let Some(target) = target {
        connect_impl(state, Some(target)).await?;
    }
    Ok(())
}

/// Adapts a probe cycle to trigger the vanished-server switch on every usable
/// per-server event (not only at `probe-finished`), while keeping the exact
/// Tauri event payloads. The switch runs detached from the probe so it can
/// take the connection permit without delaying the cycle.
pub(crate) struct ReconnectProgress {
    state: AppState,
    settings: Settings,
}

impl ReconnectProgress {
    pub(crate) fn new(state: AppState, settings: Settings) -> Self {
        Self { state, settings }
    }
}

impl ProbeProgress for ReconnectProgress {
    fn on_server(&self, event: &ProbeServerEvent) {
        ProbeProgress::on_server(&TauriProgress, event);
        if event.status == ProbeServerStatus::Done {
            let state = self.state.clone();
            let settings = self.settings.clone();
            let event = event.clone();
            tokio::spawn(async move {
                let _ = switch_if_current_missing(&state, &settings, &event).await;
            });
        }
    }

    fn on_progress(&self, tested: usize, total: usize) {
        ProbeProgress::on_progress(&TauriProgress, tested, total);
    }

    fn on_cancelled(&self) {
        ProbeProgress::on_cancelled(&TauriProgress);
    }

    fn on_finished(&self, outcome: &ProbeOutcome) {
        ProbeProgress::on_finished(&TauriProgress, outcome);
    }
}

/// Caller holds the probe permit. Auto-reconnect is gated by `apply_reconnect`
/// (manual runs pass `false`); the last-switch timestamp is recorded here after
/// a successful switch — the pure `decide_reconnect` function owns neither.
/// On any error a terminal `probe-finished` is emitted so the UI cannot remain
/// stuck in Testing... with Run disabled; success emits it in
/// `persist_probe_samples`, so the two paths never duplicate the event.
pub(crate) async fn run_impl(
    state: &AppState,
    apply_reconnect: bool,
    cancel: watch::Receiver<bool>,
    progress: &dyn ProbeProgress,
) -> Result<ProbeOutcome, CommandError> {
    let result = run_impl_inner(state, apply_reconnect, cancel, progress).await;
    if result.is_err() {
        progress.on_finished(&ProbeOutcome {
            tested: 0,
            sorted: 0,
            finished_at: Utc::now(),
            cancelled: false,
        });
    }
    result
}

async fn run_impl_inner(
    state: &AppState,
    apply_reconnect: bool,
    cancel: watch::Receiver<bool>,
    progress: &dyn ProbeProgress,
) -> Result<ProbeOutcome, CommandError> {
    // Read inputs in a short lock scope. The probe itself can take seconds per
    // server; holding the shared SQLite mutex across it makes `settings_get`
    // block forever and leaves the Settings UI on "Loading settings…".
    let (settings, profiles, services) = {
        let conn = state.inner.conn.lock().await;
        (
            settings_repo::get(&conn)?,
            profile_repo::list_present_enabled(&conn)?,
            service_profile_repo::list_enabled(&conn)?,
        )
    };

    let samples = probe_cycle_detached(
        &profiles,
        &services,
        &settings,
        &XrayProbeTransport,
        progress,
        cancel.clone(),
    )
    .await?;
    let cancelled = *cancel.borrow();

    let outcome = {
        let mut conn = state.inner.conn.lock().await;
        persist_probe_samples(
            &mut conn,
            &samples,
            progress,
            PersistOptions {
                sort: !cancelled,
                cancelled,
            },
        )?
    };

    if apply_reconnect && settings.auto_reconnect_enabled {
        let reconnect_candidates = {
            let conn = state.inner.conn.lock().await;
            candidates(&conn)?
        };
        let _ = super::probe::apply_reconnect(state, &settings, &reconnect_candidates, Utc::now())
            .await;
    }

    Ok(outcome)
}

#[tauri::command]
pub async fn probe_run(state: State<'_, AppState>) -> Result<ProbeOutcome, CommandError> {
    let _permit = state.probe_permit()?;
    let cancel = state.start_probe();
    let result = async {
        let settings = {
            let conn = state.inner.conn.lock().await;
            settings_repo::get(&conn)?
        };
        run_impl(
            &state,
            false,
            cancel,
            &ReconnectProgress::new(AppState::clone(&state), settings),
        )
        .await
    }
    .await;
    state.finish_probe();
    result
}

/// Accept a stop request only while a probe cycle owns the probe permit.
/// Flips the watch flag and returns `true`; `probe-cancelled` is emitted here
/// exactly once, on the first accepted request of this cycle. Idle callers get
/// `false` and no event.
// Sync on purpose: Tauri rejects an async command with a borrowed `State`
// unless it returns `Result`. The IPC payload stays a plain boolean.
#[tauri::command]
pub fn probe_cancel(state: State<'_, AppState>) -> bool {
    // Hold the running-flag lock across the check and the send so a stop
    // observed for cycle A cannot land after A's `finish_probe()` and B's
    // `start_probe()`, cancelling the freshly started cycle B.
    let running = state
        .inner
        .probe_running
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !*running {
        return false;
    }
    // `send_replace` is atomic: racing stop requests emit at most one event.
    let already_cancelled = state.inner.probe_cancel_tx.send_replace(true);
    if !already_cancelled {
        crate::state::emit("probe-cancelled", json!({}));
    }
    true
}

#[tauri::command]
pub async fn profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<ProfilePayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = profile_repo::list_present(&conn)?;
    Ok(rows.into_iter().map(ProfilePayload::from).collect())
}
