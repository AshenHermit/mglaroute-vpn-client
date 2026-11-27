use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Per-subscription refetch cadence assigned on add. `0` disables a row, so a
/// fresh subscription must start above zero for periodic refetch to work.
pub const DEFAULT_REFETCH_MINUTES: i64 = 15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
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

impl Default for Subscription {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            url: String::new(),
            enabled: true,
            status: "inactive".to_string(),
            auto_refetch_minutes: DEFAULT_REFETCH_MINUTES,
            last_fetched_at: None,
            last_error: None,
            user_agent: None,
            request_headers: None,
            sort: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
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

impl Default for Profile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            subscription_id: String::new(),
            server_id: String::new(),
            link: String::new(),
            remarks: None,
            protocol: None,
            address: None,
            port: None,
            raw_config: "{}".to_string(),
            present: true,
            latency_ms: None,
            reachable: false,
            checked_at: None,
            sort_index: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceProfile {
    pub id: String,
    pub name: String,
    pub test_urls: String,
    pub is_enabled: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ServiceProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            test_urls: "[]".to_string(),
            is_enabled: true,
            is_default: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileServiceResult {
    pub profile_id: String,
    pub service_profile_id: String,
    pub reachable: bool,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_refetch_enabled: false,
            refetch_interval_minutes: 15,
            auto_test_enabled: false,
            test_interval_minutes: 15,
            test_timeout_ms: 5000,
            test_concurrency: 10,
            auto_reconnect_enabled: false,
            reconnect_cooldown_seconds: 60,
            ping_url: "https://www.google.com/generate_204".to_string(),
            local_proxy_port: 10808,
            proxy_exclusions: "[]".to_string(),
            system_proxy_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub is_active: bool,
    pub sort: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for RoutingProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            is_active: false,
            sort: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub profile_id: String,
    pub remark: String,
    pub enabled: bool,
    pub outbound: String,
    pub domains: String,
    pub ips: String,
    pub sort: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for RoutingRule {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            profile_id: String::new(),
            remark: String::new(),
            enabled: true,
            outbound: "proxy".to_string(),
            domains: "[]".to_string(),
            ips: "[]".to_string(),
            sort: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_subscription_has_usable_refetch_interval() {
        assert!(
            Subscription::default().auto_refetch_minutes > 0,
            "a fresh subscription must be eligible for periodic refetch"
        );
        assert_eq!(DEFAULT_REFETCH_MINUTES, 15);
    }
}
