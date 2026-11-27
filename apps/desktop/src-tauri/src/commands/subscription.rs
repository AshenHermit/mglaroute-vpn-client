use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use super::{AppState, CommandError};
use crate::state::emit;
use crate::storage::models::Subscription;
use crate::storage::repos::subscription_repo;
use crate::subscription::fetch::{build_client, fetch_subscription};
use crate::subscription::import::import_subscription_text;

/// CamelCase mirror of the TS `Subscription` contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPayload {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub status: String,
    pub auto_refetch_minutes: i64,
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub user_agent: Option<String>,
    pub request_headers: Option<String>,
    pub sort: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Subscription> for SubscriptionPayload {
    fn from(value: Subscription) -> Self {
        Self {
            id: value.id,
            name: value.name,
            url: value.url,
            enabled: value.enabled,
            status: value.status,
            auto_refetch_minutes: value.auto_refetch_minutes,
            last_fetched_at: value.last_fetched_at,
            last_error: value.last_error,
            user_agent: value.user_agent,
            request_headers: value.request_headers,
            sort: value.sort,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResultPayload {
    pub added: usize,
    pub updated: usize,
    pub absent: usize,
    pub bad_lines: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionRefreshedEvent {
    subscription_id: String,
    added: usize,
    updated: usize,
    absent: usize,
    bad_lines: usize,
}

fn validate_url(url: &str) -> Result<(), CommandError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| CommandError::InvalidInput)?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(CommandError::InvalidInput)
    }
}

fn apply_subscription_patch(
    mut value: Subscription,
    name: Option<String>,
    url: Option<String>,
    enabled: Option<bool>,
    auto_refetch_minutes: Option<i64>,
) -> Subscription {
    if let Some(name) = name {
        value.name = name;
    }
    if let Some(url) = url {
        value.url = url;
    }
    if let Some(enabled) = enabled {
        value.enabled = enabled;
    }
    if let Some(minutes) = auto_refetch_minutes {
        value.auto_refetch_minutes = minutes;
    }
    value
}

fn new_id(url: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update(nanos.to_le_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sub-{digest}")
}

async fn refresh_body(
    state: &AppState,
    id: &str,
) -> Result<RefreshResultPayload, CommandError> {
    let (url, user_agent) = {
        let conn = state.inner.conn.lock().await;
        let sub = subscription_repo::get(&conn, id)?.ok_or(CommandError::NotFound)?;
        (sub.url, sub.user_agent)
    };

    let client = build_client();
    // Errors are intentionally discarded: reqwest messages can include the
    // complete feed URL with its token.
    let body = fetch_subscription(&client, &url, user_agent.as_deref())
        .await
        .map_err(|_| CommandError::Fetch)?;
    let summary = {
        let mut conn = state.inner.conn.lock().await;
        import_subscription_text(&mut conn, id, &body)?
    };

    emit(
        "subscription-refreshed",
        SubscriptionRefreshedEvent {
            subscription_id: id.to_string(),
            added: summary.added,
            updated: summary.updated,
            absent: summary.absent,
            bad_lines: summary.bad_lines,
        },
    );

    Ok(RefreshResultPayload {
        added: summary.added,
        updated: summary.updated,
        absent: summary.absent,
        bad_lines: summary.bad_lines,
    })
}

/// Caller holds the mutation permit. A failed fetch is recorded on the row so
/// the UI can show the error badge on a later `subscription_list`.
pub(crate) async fn refresh_impl(
    state: &AppState,
    id: &str,
) -> Result<RefreshResultPayload, CommandError> {
    let outcome = refresh_body(state, id).await;
    if outcome.is_err() {
        let conn = state.inner.conn.lock().await;
        if let Some(mut sub) = subscription_repo::get(&conn, id)? {
            sub.status = "error".to_string();
            sub.last_error = Some("refresh failed".to_string());
            subscription_repo::update(&conn, &sub)?;
        }
    }
    outcome
}

#[tauri::command]
pub async fn subscription_add(
    state: State<'_, AppState>,
    url: String,
    name: Option<String>,
) -> Result<SubscriptionPayload, CommandError> {
    let _permit = state.permit()?;
    validate_url(&url)?;

    let value = Subscription {
        id: new_id(&url),
        name: name.filter(|value| !value.is_empty()).unwrap_or_else(|| "Subscription".to_string()),
        url,
        ..Default::default()
    };
    {
        let conn = state.inner.conn.lock().await;
        subscription_repo::insert(&conn, &value)?;
    }

    // Import immediately; a network failure is captured in the row rather than
    // failing the add, so the UI can display the subscription's error status.
    let _ = refresh_impl(&state, &value.id).await;

    let conn = state.inner.conn.lock().await;
    let stored = subscription_repo::get(&conn, &value.id)?.ok_or(CommandError::NotFound)?;
    Ok(stored.into())
}

#[tauri::command]
pub async fn subscription_list(
    state: State<'_, AppState>,
) -> Result<Vec<SubscriptionPayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = subscription_repo::list(&conn)?;
    Ok(rows.into_iter().map(SubscriptionPayload::from).collect())
}

#[tauri::command]
pub async fn subscription_update(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    url: Option<String>,
    enabled: Option<bool>,
    auto_refetch_minutes: Option<i64>,
) -> Result<SubscriptionPayload, CommandError> {
    let _permit = state.permit()?;
    if name.as_deref().is_some_and(str::is_empty) {
        return Err(CommandError::InvalidInput);
    }
    if let Some(url) = url.as_deref() {
        validate_url(url)?;
    }
    if auto_refetch_minutes.is_some_and(|minutes| minutes < 0) {
        return Err(CommandError::InvalidInput);
    }

    let conn = state.inner.conn.lock().await;
    let value = subscription_repo::get(&conn, &id)?.ok_or(CommandError::NotFound)?;
    let value = apply_subscription_patch(value, name, url, enabled, auto_refetch_minutes);
    subscription_repo::update(&conn, &value)?;
    Ok(value.into())
}

#[tauri::command]
pub async fn subscription_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    let _permit = state.permit()?;
    let conn = state.inner.conn.lock().await;
    if subscription_repo::get(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    subscription_repo::remove(&conn, &id)?;
    Ok(())
}

#[tauri::command]
pub async fn subscription_refresh(
    state: State<'_, AppState>,
    id: String,
) -> Result<RefreshResultPayload, CommandError> {
    let _permit = state.permit()?;
    refresh_impl(&state, &id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_patch_preserves_omitted_fields() {
        let base = Subscription {
            id: "sub-1".to_string(),
            name: "old".to_string(),
            url: "https://old.example/sub".to_string(),
            enabled: true,
            auto_refetch_minutes: 60,
            ..Default::default()
        };
        let patched = apply_subscription_patch(
            base,
            Some("new".to_string()),
            Some("https://new.example/sub".to_string()),
            None,
            None,
        );
        assert_eq!(patched.name, "new");
        assert_eq!(patched.url, "https://new.example/sub");
        assert!(patched.enabled);
        assert_eq!(patched.auto_refetch_minutes, 60);
    }
}
