use serde::Serialize;
use tauri::State;

use super::{AppState, CommandError};
use crate::storage::models::Settings;
use crate::storage::repos::settings_repo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub auto_refetch_enabled: bool,
    pub refetch_interval_minutes: i64,
    pub auto_test_enabled: bool,
    pub test_interval_minutes: i64,
    pub test_timeout_ms: i64,
    pub test_concurrency: i64,
    pub auto_reconnect_enabled: bool,
    pub reconnect_cooldown_seconds: i64,
    pub ping_url: String,
    pub local_proxy_port: i64,
    pub proxy_exclusions: String,
    pub system_proxy_enabled: bool,
}

impl From<Settings> for SettingsPayload {
    fn from(value: Settings) -> Self {
        Self {
            auto_refetch_enabled: value.auto_refetch_enabled,
            refetch_interval_minutes: value.refetch_interval_minutes,
            auto_test_enabled: value.auto_test_enabled,
            test_interval_minutes: value.test_interval_minutes,
            test_timeout_ms: value.test_timeout_ms,
            test_concurrency: value.test_concurrency,
            auto_reconnect_enabled: value.auto_reconnect_enabled,
            reconnect_cooldown_seconds: value.reconnect_cooldown_seconds,
            ping_url: value.ping_url,
            local_proxy_port: value.local_proxy_port,
            proxy_exclusions: value.proxy_exclusions,
            system_proxy_enabled: value.system_proxy_enabled,
        }
    }
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<SettingsPayload, CommandError> {
    let conn = state.inner.conn.lock().await;
    Ok(settings_repo::get(&conn)?.into())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn settings_update(
    state: State<'_, AppState>,
    auto_refetch_enabled: Option<bool>,
    refetch_interval_minutes: Option<i64>,
    auto_test_enabled: Option<bool>,
    test_interval_minutes: Option<i64>,
    test_timeout_ms: Option<i64>,
    test_concurrency: Option<i64>,
    auto_reconnect_enabled: Option<bool>,
    reconnect_cooldown_seconds: Option<i64>,
    ping_url: Option<String>,
    local_proxy_port: Option<i64>,
    proxy_exclusions: Option<String>,
    system_proxy_enabled: Option<bool>,
) -> Result<SettingsPayload, CommandError> {
    let _permit = state.permit()?;

    for value in [
        refetch_interval_minutes,
        test_interval_minutes,
        test_timeout_ms,
        reconnect_cooldown_seconds,
    ] {
        if value.is_some_and(|value| value <= 0) {
            return Err(CommandError::InvalidInput);
        }
    }
    if test_concurrency.is_some_and(|value| !(1..=20).contains(&value)) {
        return Err(CommandError::InvalidInput);
    }
    if local_proxy_port.is_some_and(|port| !(1..=65535).contains(&port)) {
        return Err(CommandError::InvalidInput);
    }

    let conn = state.inner.conn.lock().await;
    let mut value = settings_repo::get(&conn)?;
    if let Some(input) = auto_refetch_enabled {
        value.auto_refetch_enabled = input;
    }
    if let Some(input) = refetch_interval_minutes {
        value.refetch_interval_minutes = input;
    }
    if let Some(input) = auto_test_enabled {
        value.auto_test_enabled = input;
    }
    if let Some(input) = test_interval_minutes {
        value.test_interval_minutes = input;
    }
    if let Some(input) = test_timeout_ms {
        value.test_timeout_ms = input;
    }
    if let Some(input) = test_concurrency {
        value.test_concurrency = input;
    }
    if let Some(input) = auto_reconnect_enabled {
        value.auto_reconnect_enabled = input;
    }
    if let Some(input) = reconnect_cooldown_seconds {
        value.reconnect_cooldown_seconds = input;
    }
    if let Some(input) = ping_url {
        value.ping_url = input;
    }
    if let Some(input) = local_proxy_port {
        value.local_proxy_port = input;
    }
    if let Some(input) = proxy_exclusions {
        value.proxy_exclusions = input;
    }
    if let Some(input) = system_proxy_enabled {
        value.system_proxy_enabled = input;
    }
    settings_repo::update(&conn, &value)?;
    Ok(value.into())
}
