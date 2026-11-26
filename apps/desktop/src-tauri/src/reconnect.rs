//! Auto-reconnect decision after a probe cycle.

use chrono::{DateTime, Duration, Utc};

use crate::storage::models::Settings;

/// Action to take after a probe cycle when auto-connect is enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectDecision {
    /// Keep the current connection.
    None,
    /// Switch the active proxy to the named profile.
    Switch { profile_id: String },
    /// No profile passes the active service profiles; drop the connection.
    Disconnect,
}

/// Decide whether to stay, switch, or disconnect after a probe cycle.
///
/// `candidates` is `(profile_id, latency_ms, passes_all_active_profiles)`.
/// Only passing candidates are eligible. The best one has the lowest latency
/// (profile id breaks ties). A current profile that fails the filters is
/// replaced immediately, ignoring cooldown. Enabling the feature and
/// persisting the switch time are the caller's responsibility.
pub fn decide_reconnect(
    current_id: Option<&str>,
    candidates: &[(String, u64, bool)],
    settings: &Settings,
    now: DateTime<Utc>,
    last_switch: Option<DateTime<Utc>>,
) -> ReconnectDecision {
    if candidates.is_empty() {
        return ReconnectDecision::None;
    }

    let passing: Vec<&(String, u64, bool)> = candidates
        .iter()
        .filter(|(_, _, passes)| *passes)
        .collect();
    let best = passing
        .iter()
        .copied()
        .min_by(|left, right| (left.1, left.0.as_str()).cmp(&(right.1, right.0.as_str())));

    let current_passes = current_id
        .is_some_and(|id| passing.iter().any(|(profile_id, _, _)| profile_id == id));
    if !current_passes {
        return match best {
            Some((profile_id, _, _)) => ReconnectDecision::Switch {
                profile_id: profile_id.clone(),
            },
            None => ReconnectDecision::Disconnect,
        };
    }

    let Some((best_id, _, _)) = best else {
        return ReconnectDecision::None;
    };
    if best_id == current_id.expect("current profile passes") {
        return ReconnectDecision::None;
    }

    let cooldown = Duration::seconds(settings.reconnect_cooldown_seconds.max(0));
    let cooldown_elapsed = last_switch
        .map(|last| now.signed_duration_since(last) >= cooldown)
        .unwrap_or(true);
    if !cooldown_elapsed {
        return ReconnectDecision::None;
    }

    ReconnectDecision::Switch {
        profile_id: best_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-25T00:00:00.000Z";

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("parse timestamp")
            .with_timezone(&Utc)
    }

    fn settings(cooldown_seconds: i64) -> Settings {
        Settings {
            auto_reconnect_enabled: true,
            reconnect_cooldown_seconds: cooldown_seconds,
            ..Default::default()
        }
    }

    fn candidates(entries: &[(&str, u64, bool)]) -> Vec<(String, u64, bool)> {
        entries
            .iter()
            .map(|(id, latency, passes)| ((*id).to_string(), *latency, *passes))
            .collect()
    }

    #[test]
    fn no_better_candidate_none() {
        let now = utc(NOW);
        let entries = candidates(&[("current", 10, true), ("other", 20, true)]);
        assert_eq!(
            decide_reconnect(Some("current"), &entries, &settings(60), now, None),
            ReconnectDecision::None
        );
    }

    #[test]
    fn better_candidate_switches_after_cooldown() {
        let now = utc(NOW);
        let entries = candidates(&[("current", 20, true), ("best", 10, true)]);
        let last_switch = now - Duration::seconds(61);
        assert_eq!(
            decide_reconnect(
                Some("current"),
                &entries,
                &settings(60),
                now,
                Some(last_switch)
            ),
            ReconnectDecision::Switch {
                profile_id: "best".to_string()
            }
        );
    }

    #[test]
    fn cooldown_blocks_switch() {
        let now = utc(NOW);
        let entries = candidates(&[("current", 20, true), ("best", 10, true)]);
        let last_switch = now - Duration::seconds(59);
        assert_eq!(
            decide_reconnect(
                Some("current"),
                &entries,
                &settings(60),
                now,
                Some(last_switch)
            ),
            ReconnectDecision::None
        );
    }

    #[test]
    fn current_fails_filters_forces_switch_ignoring_cooldown() {
        let now = utc(NOW);
        let entries = candidates(&[("current", 1, false), ("best", 50, true)]);
        let last_switch = now - Duration::seconds(1);
        assert_eq!(
            decide_reconnect(
                Some("current"),
                &entries,
                &settings(60),
                now,
                Some(last_switch)
            ),
            ReconnectDecision::Switch {
                profile_id: "best".to_string()
            }
        );
    }

    #[test]
    fn no_candidate_passing_disconnects() {
        let now = utc(NOW);
        let entries = candidates(&[("current", 10, false)]);
        assert_eq!(
            decide_reconnect(Some("current"), &entries, &settings(60), now, None),
            ReconnectDecision::Disconnect
        );
    }

    #[test]
    fn no_active_profiles_uses_delay_only() {
        let now = utc(NOW);
        let entries = candidates(&[("slow", 50, true), ("fast", 10, true)]);
        assert_eq!(
            decide_reconnect(Some("slow"), &entries, &settings(60), now, None),
            ReconnectDecision::Switch {
                profile_id: "fast".to_string()
            }
        );
    }

    #[test]
    fn no_measurement_no_switch() {
        let now = utc(NOW);
        let entries: Vec<(String, u64, bool)> = Vec::new();
        assert_eq!(
            decide_reconnect(Some("current"), &entries, &settings(60), now, None),
            ReconnectDecision::None
        );
    }
}
