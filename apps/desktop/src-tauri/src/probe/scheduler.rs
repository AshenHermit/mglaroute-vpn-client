//! Scheduler gates for auto-refetch and auto-test ticks.

use chrono::{DateTime, Duration, Utc};

use crate::storage::models::Subscription;

/// Eligibility gate for one subscription on a periodic refetch tick.
/// A subscription never fetched is eligible when both flags permit it.
pub fn should_refetch(sub: &Subscription, now: DateTime<Utc>, enabled: bool) -> bool {
    if !enabled || sub.auto_refetch_minutes <= 0 {
        return false;
    }
    match sub.last_fetched_at {
        Some(last_fetched_at) => {
            now.signed_duration_since(last_fetched_at)
                >= Duration::minutes(sub.auto_refetch_minutes)
        }
        None => true,
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

    fn subscription(interval_minutes: i64, last_fetched_at: Option<DateTime<Utc>>) -> Subscription {
        Subscription {
            id: "sub-1".to_string(),
            name: "test".to_string(),
            url: "https://example.test/sub".to_string(),
            enabled: true,
            auto_refetch_minutes: interval_minutes,
            last_fetched_at,
            ..Default::default()
        }
    }

    #[test]
    fn refetch_gate_requires_both_flags() {
        let now = utc(NOW);
        let ten_minutes_ago = now - Duration::minutes(10);
        let fourteen_minutes_ago = now - Duration::minutes(14);
        let fifteen_minutes_ago = now - Duration::minutes(15);

        assert!(!should_refetch(
            &subscription(15, Some(ten_minutes_ago)),
            now,
            false
        ));
        assert!(!should_refetch(
            &subscription(0, Some(ten_minutes_ago)),
            now,
            true
        ));
        assert!(!should_refetch(
            &subscription(15, Some(fourteen_minutes_ago)),
            now,
            true
        ));
        assert!(should_refetch(
            &subscription(15, Some(fifteen_minutes_ago)),
            now,
            true
        ));
        assert!(should_refetch(&subscription(15, None), now, true));
    }
}
