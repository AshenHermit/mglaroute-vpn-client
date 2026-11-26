use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use super::{AppState, CommandError};
use crate::service_profiles::validate_test_urls;
use crate::storage::models::ServiceProfile;
use crate::storage::repos::service_profile_repo;

/// Command payload exposes a canonical `string[]`; the DB still stores JSON
/// text internally.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfilePayload {
    pub id: String,
    pub name: String,
    pub test_urls: Vec<String>,
    pub is_enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ServiceProfile> for ServiceProfilePayload {
    fn from(value: ServiceProfile) -> Self {
        Self {
            id: value.id,
            name: value.name,
            test_urls: serde_json::from_str::<Vec<String>>(&value.test_urls).unwrap_or_default(),
            is_enabled: value.is_enabled,
            is_default: value.is_default,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

fn new_id(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(nanos.to_le_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("svc-{digest}")
}

#[tauri::command]
pub async fn service_profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<ServiceProfilePayload>, CommandError> {
    let conn = state.inner.conn.lock().await;
    let rows = service_profile_repo::list(&conn)?;
    Ok(rows.into_iter().map(ServiceProfilePayload::from).collect())
}

#[tauri::command]
pub async fn service_profile_create(
    state: State<'_, AppState>,
    name: String,
    test_urls: Vec<String>,
) -> Result<ServiceProfilePayload, CommandError> {
    let _permit = state.permit()?;
    if name.is_empty() || !validate_test_urls(&test_urls) {
        return Err(CommandError::InvalidInput);
    }

    let conn = state.inner.conn.lock().await;
    if service_profile_repo::exists_by_name(&conn, &name)? {
        return Err(CommandError::InvalidInput);
    }
    let value = ServiceProfile {
        id: new_id(&name),
        name,
        test_urls: serde_json::to_string(&test_urls).map_err(|_| CommandError::InvalidInput)?,
        is_enabled: true,
        is_default: false,
        ..Default::default()
    };
    service_profile_repo::insert(&conn, &value)?;
    Ok(value.into())
}

#[tauri::command]
pub async fn service_profile_update(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    test_urls: Option<Vec<String>>,
    is_enabled: Option<bool>,
    is_default: Option<bool>,
) -> Result<ServiceProfilePayload, CommandError> {
    let _permit = state.permit()?;
    if name.as_deref().is_some_and(str::is_empty) {
        return Err(CommandError::InvalidInput);
    }
    if let Some(urls) = test_urls.as_deref() {
        if !validate_test_urls(urls) {
            return Err(CommandError::InvalidInput);
        }
    }

    let conn = state.inner.conn.lock().await;
    let mut value = service_profile_repo::get(&conn, &id)?.ok_or(CommandError::NotFound)?;
    if let Some(name) = name {
        value.name = name;
    }
    if let Some(urls) = test_urls {
        value.test_urls = serde_json::to_string(&urls).map_err(|_| CommandError::InvalidInput)?;
    }
    if let Some(enabled) = is_enabled {
        value.is_enabled = enabled;
    }
    if let Some(default) = is_default {
        value.is_default = default;
    }
    let duplicate_name = service_profile_repo::list(&conn)?
        .iter()
        .any(|other| other.id != value.id && other.name == value.name);
    if duplicate_name {
        return Err(CommandError::InvalidInput);
    }
    service_profile_repo::update(&conn, &value)?;
    Ok(value.into())
}

#[tauri::command]
pub async fn service_profile_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    let _permit = state.permit()?;
    let conn = state.inner.conn.lock().await;
    if service_profile_repo::get(&conn, &id)?.is_none() {
        return Err(CommandError::NotFound);
    }
    service_profile_repo::remove(&conn, &id)?;
    Ok(())
}
