//! Routing profile/rule command surface. All validation is authoritative in
//! Rust; payloads are camelCase to match `packages/contracts`. Mutations take
//! the shared mutation permit so subscriptions/settings/routing cannot race.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use super::{AppState, CommandError};
use crate::storage::models::{RoutingProfile, RoutingRule};
use crate::storage::routing_repo;

/// Command payload exposes canonical camelCase keys; the DB stays snake_case
/// and `domains`/`ips` stay JSON text internally.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingProfilePayload {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub is_active: bool,
    pub sort: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<RoutingProfile> for RoutingProfilePayload {
    fn from(value: RoutingProfile) -> Self {
        Self {
            id: value.id,
            name: value.name,
            enabled: value.enabled,
            is_active: value.is_active,
            sort: value.sort,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRulePayload {
    pub id: String,
    pub profile_id: String,
    pub remark: String,
    pub enabled: bool,
    pub outbound: String,
    pub domains: Vec<String>,
    pub ips: Vec<String>,
    pub sort: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<RoutingRule> for RoutingRulePayload {
    fn from(value: RoutingRule) -> Self {
        Self {
            id: value.id,
            profile_id: value.profile_id,
            remark: value.remark,
            enabled: value.enabled,
            outbound: value.outbound,
            domains: serde_json::from_str::<Vec<String>>(&value.domains).unwrap_or_default(),
            ips: serde_json::from_str::<Vec<String>>(&value.ips).unwrap_or_default(),
            sort: value.sort,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

fn new_id(prefix: &str, seed: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(nanos.to_le_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{prefix}-{digest}")
}

/// Trim a profile name; empty/whitespace-only is `InvalidInput`.
fn validated_name(name: &str) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::InvalidInput);
    }
    Ok(name.to_string())
}

/// `proxy|direct|block` only; no rewriting of `geosite:`/`geoip:` prefixes.
fn validated_outbound(outbound: &str) -> Result<String, CommandError> {
    match outbound {
        "proxy" | "direct" | "block" => Ok(outbound.to_string()),
        _ => Err(CommandError::InvalidInput),
    }
}

/// Trim every entry and require at least one non-blank matcher across both
/// arrays. Blank entries are dropped; `geosite:`/`geoip:` stay intact.
fn validated_matchers(
    domains: &[String],
    ips: &[String],
) -> Result<(Vec<String>, Vec<String>), CommandError> {
    let trim_nonblank = |entries: &[String]| -> Vec<String> {
        entries
            .iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect()
    };
    let domains = trim_nonblank(domains);
    let ips = trim_nonblank(ips);
    if domains.is_empty() && ips.is_empty() {
        return Err(CommandError::InvalidInput);
    }
    Ok((domains, ips))
}

pub(crate) async fn profile_list_impl(
    state: &AppState,
) -> Result<Vec<RoutingProfilePayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = routing_repo::list_profiles(&conn)?;
    Ok(rows.into_iter().map(RoutingProfilePayload::from).collect())
}

pub(crate) async fn profile_create_impl(
    state: &AppState,
    name: String,
) -> Result<RoutingProfilePayload, CommandError> {
    let _permit = state.permit()?;
    let name = validated_name(&name)?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::exists_profile_by_name(&conn, &name, None)? {
        return Err(CommandError::InvalidInput);
    }
    let value = RoutingProfile {
        id: new_id("rtp", &name),
        name,
        sort: routing_repo::next_profile_sort(&conn)?,
        ..Default::default()
    };
    routing_repo::insert_profile(&conn, &value)?;
    Ok(value.into())
}

pub(crate) async fn profile_update_impl(
    state: &AppState,
    id: String,
    name: String,
    enabled: bool,
) -> Result<RoutingProfilePayload, CommandError> {
    let _permit = state.permit()?;
    let name = validated_name(&name)?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_profile(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    if routing_repo::exists_profile_by_name(&conn, &name, Some(&id))? {
        return Err(CommandError::InvalidInput);
    }
    routing_repo::update_profile(&conn, &id, &name, enabled)?;
    Ok(routing_repo::get_profile(&conn, &id)?
        .ok_or(CommandError::NotFound)?
        .into())
}

pub(crate) async fn profile_activate_impl(
    state: &AppState,
    id: String,
) -> Result<RoutingProfilePayload, CommandError> {
    let _permit = state.permit()?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_profile(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    if !routing_repo::activate_profile(&conn, &id)? {
        return Err(CommandError::InvalidInput);
    }
    Ok(routing_repo::get_profile(&conn, &id)?
        .ok_or(CommandError::NotFound)?
        .into())
}

pub(crate) async fn profile_remove_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    let _permit = state.permit()?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_profile(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    routing_repo::remove_profile(&conn, &id)?;
    Ok(())
}

pub(crate) async fn rule_list_impl(
    state: &AppState,
    profile_id: String,
) -> Result<Vec<RoutingRulePayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = routing_repo::list_rules(&conn, &profile_id)?;
    Ok(rows.into_iter().map(RoutingRulePayload::from).collect())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn rule_create_impl(
    state: &AppState,
    profile_id: String,
    remark: String,
    enabled: bool,
    outbound: String,
    domains: Vec<String>,
    ips: Vec<String>,
) -> Result<RoutingRulePayload, CommandError> {
    let _permit = state.permit()?;
    let outbound = validated_outbound(&outbound)?;
    let (domains, ips) = validated_matchers(&domains, &ips)?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_profile(&conn, &profile_id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    let sort = routing_repo::next_rule_sort(&conn, &profile_id)?;
    let value = RoutingRule {
        id: new_id("rtr", &profile_id),
        profile_id,
        remark: remark.trim().to_string(),
        enabled,
        outbound,
        domains: serde_json::to_string(&domains).map_err(|_| CommandError::InvalidInput)?,
        ips: serde_json::to_string(&ips).map_err(|_| CommandError::InvalidInput)?,
        sort,
        ..Default::default()
    };
    routing_repo::insert_rule(&conn, &value)?;
    Ok(value.into())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn rule_update_impl(
    state: &AppState,
    id: String,
    remark: String,
    enabled: bool,
    outbound: String,
    domains: Vec<String>,
    ips: Vec<String>,
) -> Result<RoutingRulePayload, CommandError> {
    let _permit = state.permit()?;
    let outbound = validated_outbound(&outbound)?;
    let (domains, ips) = validated_matchers(&domains, &ips)?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_rule(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    routing_repo::update_rule(
        &conn,
        &id,
        remark.trim(),
        enabled,
        &outbound,
        &domains,
        &ips,
    )?;
    Ok(routing_repo::get_rule(&conn, &id)?
        .ok_or(CommandError::NotFound)?
        .into())
}

pub(crate) async fn rule_remove_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    let _permit = state.permit()?;

    let conn = state.inner.conn.lock().await;
    if routing_repo::get_rule(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    routing_repo::remove_rule(&conn, &id)?;
    Ok(())
}

pub(crate) async fn rule_reorder_impl(
    state: &AppState,
    id: String,
    direction: i32,
) -> Result<(), CommandError> {
    let _permit = state.permit()?;

    // Reject invalid directions instead of silently no-op'ing. The repo clamps
    // at list edges (missing neighbor keeps the current order).
    if direction != -1 && direction != 1 {
        return Err(CommandError::InvalidInput);
    }
    let conn = state.inner.conn.lock().await;
    if routing_repo::get_rule(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    routing_repo::reorder_rule(&conn, &id, direction)?;
    Ok(())
}

#[tauri::command]
pub async fn routing_profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<RoutingProfilePayload>, CommandError> {
    profile_list_impl(state.inner()).await
}

#[tauri::command]
pub async fn routing_profile_create(
    state: State<'_, AppState>,
    name: String,
) -> Result<RoutingProfilePayload, CommandError> {
    profile_create_impl(state.inner(), name).await
}

#[tauri::command]
pub async fn routing_profile_update(
    state: State<'_, AppState>,
    id: String,
    name: String,
    enabled: bool,
) -> Result<RoutingProfilePayload, CommandError> {
    profile_update_impl(state.inner(), id, name, enabled).await
}

#[tauri::command]
pub async fn routing_profile_activate(
    state: State<'_, AppState>,
    id: String,
) -> Result<RoutingProfilePayload, CommandError> {
    profile_activate_impl(state.inner(), id).await
}

#[tauri::command]
pub async fn routing_profile_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    profile_remove_impl(state.inner(), id).await
}

#[tauri::command]
pub async fn routing_rule_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<RoutingRulePayload>, CommandError> {
    rule_list_impl(state.inner(), profile_id).await
}

#[tauri::command]
pub async fn routing_rule_create(
    state: State<'_, AppState>,
    profile_id: String,
    remark: String,
    enabled: bool,
    outbound: String,
    domains: Vec<String>,
    ips: Vec<String>,
) -> Result<RoutingRulePayload, CommandError> {
    rule_create_impl(state.inner(), profile_id, remark, enabled, outbound, domains, ips).await
}

#[tauri::command]
pub async fn routing_rule_update(
    state: State<'_, AppState>,
    id: String,
    remark: String,
    enabled: bool,
    outbound: String,
    domains: Vec<String>,
    ips: Vec<String>,
) -> Result<RoutingRulePayload, CommandError> {
    rule_update_impl(state.inner(), id, remark, enabled, outbound, domains, ips).await
}

#[tauri::command]
pub async fn routing_rule_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    rule_remove_impl(state.inner(), id).await
}

#[tauri::command]
pub async fn routing_rule_reorder(
    state: State<'_, AppState>,
    id: String,
    direction: i32,
) -> Result<(), CommandError> {
    rule_reorder_impl(state.inner(), id, direction).await
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use crate::commands::routing::{
        profile_activate_impl, profile_create_impl, profile_list_impl, profile_remove_impl,
        profile_update_impl, rule_create_impl, rule_list_impl, rule_remove_impl, rule_reorder_impl,
        rule_update_impl, RoutingProfilePayload, RoutingRulePayload,
    };
    use crate::commands::{AppState, CommandError, CoreHandle, CoreLauncher};
    use crate::core::snapshot::ConfigSnapshot;
    use crate::platform::proxy::{InMemoryProxy, PreviousProxy};
    use crate::storage::migrate;

    struct NoopHandle;

    impl CoreHandle for NoopHandle {
        fn stop(&mut self) -> std::result::Result<(), String> {
            Ok(())
        }

        fn try_wait(&mut self) -> std::result::Result<bool, String> {
            Ok(true)
        }
    }

    struct NoopLauncher;

    impl CoreLauncher for NoopLauncher {
        fn launch(&self, _snapshot: &ConfigSnapshot) -> std::result::Result<Box<dyn CoreHandle>, String> {
            Ok(Box::new(NoopHandle))
        }
    }

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        AppState::new(
            conn,
            std::path::PathBuf::new(),
            Box::new(InMemoryProxy::new(PreviousProxy::Direct)),
            Box::new(NoopLauncher),
        )
    }

    #[test]
    fn payloads_are_camel_case() {
        let now = chrono::Utc::now();
        let profile = RoutingProfilePayload {
            id: "rtp-1".to_string(),
            name: "alpha".to_string(),
            enabled: true,
            is_active: false,
            sort: 10,
            created_at: now,
            updated_at: now,
        };
        let value = serde_json::to_value(&profile).expect("serialize profile payload");
        let mut keys: Vec<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["createdAt", "enabled", "id", "isActive", "name", "sort", "updatedAt"]
        );

        let rule = RoutingRulePayload {
            id: "rtr-1".to_string(),
            profile_id: "rtp-1".to_string(),
            remark: "block ads".to_string(),
            enabled: true,
            outbound: "block".to_string(),
            domains: vec!["geosite:private".to_string()],
            ips: vec!["geoip:private".to_string()],
            sort: 10,
            created_at: now,
            updated_at: now,
        };
        let value = serde_json::to_value(&rule).expect("serialize rule payload");
        let mut keys: Vec<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "createdAt", "domains", "enabled", "id", "ips", "outbound", "profileId", "remark",
                "sort", "updatedAt"
            ]
        );
        assert_eq!(value["domains"], json!(["geosite:private"]));
    }

    #[tokio::test]
    async fn profile_crud_commands() {
        let state = test_state();
        let created = profile_create_impl(&state, "alpha".to_string())
            .await
            .expect("create profile");
        assert!(created.id.starts_with("rtp-"));
        assert_eq!(created.name, "alpha");
        assert!(created.enabled);
        assert!(!created.is_active);

        let listed = profile_list_impl(&state).await.expect("list profiles");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let updated = profile_update_impl(&state, created.id.clone(), "beta".to_string(), true)
            .await
            .expect("update profile");
        assert_eq!(updated.name, "beta");

        let activated = profile_activate_impl(&state, created.id.clone())
            .await
            .expect("activate profile");
        assert!(activated.is_active);

        let active_count = profile_list_impl(&state)
            .await
            .expect("list profiles")
            .iter()
            .filter(|profile| profile.is_active)
            .count();
        assert_eq!(active_count, 1);

        profile_remove_impl(&state, created.id.clone())
            .await
            .expect("remove profile");
        assert!(profile_list_impl(&state)
            .await
            .expect("list profiles")
            .is_empty());
    }

    #[tokio::test]
    async fn profile_validation_errors() {
        let state = test_state();
        assert_eq!(
            profile_create_impl(&state, "".to_string())
                .await
                .expect_err("empty name rejected"),
            CommandError::InvalidInput
        );
        assert_eq!(
            profile_create_impl(&state, "   ".to_string())
                .await
                .expect_err("whitespace name rejected"),
            CommandError::InvalidInput
        );

        let created = profile_create_impl(&state, "alpha".to_string())
            .await
            .expect("create profile");
        assert_eq!(
            profile_create_impl(&state, "alpha".to_string())
                .await
                .expect_err("duplicate name rejected"),
            CommandError::InvalidInput
        );
        assert_eq!(
            profile_activate_impl(&state, "missing".to_string())
                .await
                .expect_err("missing id rejected"),
            CommandError::NotFound
        );

        profile_update_impl(&state, created.id.clone(), "alpha".to_string(), false)
            .await
            .expect("disable profile");
        assert_eq!(
            profile_activate_impl(&state, created.id.clone())
                .await
                .expect_err("disabled profile rejected"),
            CommandError::InvalidInput
        );
    }

    #[tokio::test]
    async fn rule_crud_and_validation() {
        let state = test_state();
        let profile = profile_create_impl(&state, "alpha".to_string())
            .await
            .expect("create profile");

        let created = rule_create_impl(
            &state,
            profile.id.clone(),
            "block private".to_string(),
            true,
            "block".to_string(),
            vec!["geosite:private".to_string()],
            vec!["geoip:private".to_string(), "10.0.0.0/8".to_string()],
        )
        .await
        .expect("create rule");
        assert!(created.id.starts_with("rtr-"));
        assert_eq!(created.profile_id, profile.id);
        assert_eq!(created.outbound, "block");

        let listed = rule_list_impl(&state, profile.id.clone())
            .await
            .expect("list rules");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].domains, vec!["geosite:private".to_string()]);
        assert_eq!(
            listed[0].ips,
            vec!["geoip:private".to_string(), "10.0.0.0/8".to_string()]
        );

        let second = rule_create_impl(
            &state,
            profile.id.clone(),
            "proxy rest".to_string(),
            true,
            "proxy".to_string(),
            vec!["example.com".to_string()],
            vec![],
        )
        .await
        .expect("create second rule");

        let updated = rule_update_impl(
            &state,
            created.id.clone(),
            "updated remark".to_string(),
            false,
            "direct".to_string(),
            vec!["example.org".to_string()],
            vec![],
        )
        .await
        .expect("update rule");
        assert_eq!(updated.remark, "updated remark");
        assert!(!updated.enabled);
        assert_eq!(updated.outbound, "direct");
        assert_eq!(updated.domains, vec!["example.org".to_string()]);

        rule_reorder_impl(&state, second.id.clone(), -1)
            .await
            .expect("reorder rule");
        let order: Vec<String> = rule_list_impl(&state, profile.id.clone())
            .await
            .expect("list rules")
            .into_iter()
            .map(|rule| rule.id)
            .collect();
        assert_eq!(order, vec![second.id.clone(), created.id.clone()]);

        assert_eq!(
            rule_create_impl(
                &state,
                profile.id.clone(),
                "bad".to_string(),
                true,
                "tunnel".to_string(),
                vec!["example.com".to_string()],
                vec![],
            )
            .await
            .expect_err("invalid outbound rejected"),
            CommandError::InvalidInput
        );
        assert_eq!(
            rule_create_impl(
                &state,
                profile.id.clone(),
                "blank".to_string(),
                true,
                "proxy".to_string(),
                vec!["   ".to_string()],
                vec![],
            )
            .await
            .expect_err("blank-only matchers rejected"),
            CommandError::InvalidInput
        );
        assert_eq!(
            rule_create_impl(
                &state,
                profile.id.clone(),
                "empty".to_string(),
                true,
                "proxy".to_string(),
                vec![],
                vec![],
            )
            .await
            .expect_err("no matchers rejected"),
            CommandError::InvalidInput
        );
        assert_eq!(
            rule_create_impl(
                &state,
                "missing".to_string(),
                "orphan".to_string(),
                true,
                "proxy".to_string(),
                vec!["example.com".to_string()],
                vec![],
            )
            .await
            .expect_err("missing profile rejected"),
            CommandError::NotFound
        );

        rule_remove_impl(&state, created.id.clone())
            .await
            .expect("remove rule");
        assert_eq!(
            rule_list_impl(&state, profile.id)
                .await
                .expect("list rules")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn mutation_commands_require_permit() {
        let state = test_state();
        {
            let _permit = state.permit().expect("hold mutation permit");
            assert_eq!(
                profile_create_impl(&state, "alpha".to_string())
                    .await
                    .expect_err("busy mutation guard"),
                CommandError::MutationBusy
            );
        }
        assert!(
            profile_create_impl(&state, "alpha".to_string())
                .await
                .is_ok(),
            "retry after permit drop succeeds"
        );
    }
}
