use std::collections::HashSet;

use chrono::{DateTime, Utc};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use super::connection::{connect_impl, disconnect_impl};
use super::{AppState, CommandError, ConnectionStatus};
use crate::probe::cycle::{
    persist_probe_samples, probe_cycle_detached, ProbeOutcome, TauriProgress, XrayProbeTransport,
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

/// Caller holds the mutation permit. The `auto_reconnect_enabled` gate, the
/// `Connected` gate and recording `last_switch` belong here; the pure decision
/// function owns neither.
pub(crate) async fn apply_reconnect(
    state: &AppState,
    settings: &Settings,
    candidates: &[(String, u64, bool)],
    now: DateTime<Utc>,
) -> Result<(), CommandError> {
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
        ReconnectDecision::Disconnect => {
            disconnect_impl(state).await?;
        }
        ReconnectDecision::None => {}
    }
    Ok(())
}

/// Caller holds the mutation permit. Auto-reconnect is gated here, and the
/// last-switch timestamp is recorded here after a successful switch — the pure
/// `decide_reconnect` function owns neither.
pub(crate) async fn run_impl(state: &AppState) -> Result<ProbeOutcome, CommandError> {
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
        &TauriProgress,
    )
    .await?;

    let outcome = {
        let mut conn = state.inner.conn.lock().await;
        persist_probe_samples(&mut conn, &samples, &TauriProgress)?
    };

    if settings.auto_reconnect_enabled {
        let reconnect_candidates = {
            let conn = state.inner.conn.lock().await;
            candidates(&conn)?
        };
        let _ = apply_reconnect(state, &settings, &reconnect_candidates, Utc::now()).await;
    }

    Ok(outcome)
}

#[tauri::command]
pub async fn probe_run(state: State<'_, AppState>) -> Result<ProbeOutcome, CommandError> {
    let _permit = state.permit()?;
    run_impl(&state).await
}

#[tauri::command]
pub async fn profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<ProfilePayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = profile_repo::list_present(&conn)?;
    Ok(rows.into_iter().map(ProfilePayload::from).collect())
}
