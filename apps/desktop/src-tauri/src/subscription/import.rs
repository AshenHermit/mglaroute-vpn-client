use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, Connection, Error, Result};
use sha2::{Digest, Sha256};

use crate::storage::models::Profile;
use crate::storage::repos::{profile_repo, subscription_repo};

use super::decode_subscription;

/// Result of one subscription import cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
    pub absent: usize,
    pub bad_lines: usize,
}

/// Stable server identity: lowercase hex sha256 of the exact link.
fn server_id(link: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(link.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Decode a fetched subscription body and reconcile it into storage.
///
/// Existing profiles keep their cached delay/reachability (handled by
/// `upsert_many`); links missing from the feed are marked absent. No URL,
/// credential or raw config is logged.
pub fn import_subscription_text(
    conn: &mut Connection,
    sub_id: &str,
    body: &str,
) -> Result<ImportSummary> {
    let decoded = decode_subscription(body);
    let now = Utc::now();

    // Include absent rows so a link that disappears and returns is "updated".
    // Keep each stored rank: upsert_many writes sort_index, so a re-import must
    // feed the existing value back or auto-sort-by-delay order is wiped.
    let mut stmt =
        conn.prepare("SELECT server_id, sort_index FROM profiles WHERE subscription_id = ?1")?;
    let existing: HashMap<String, i64> = stmt
        .query_map(params![sub_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_>>()?;
    drop(stmt);

    let mut feed_ids = HashSet::with_capacity(decoded.profiles.len());
    let mut added = 0usize;
    let mut updated = 0usize;
    let mut profiles = Vec::with_capacity(decoded.profiles.len());
    for parsed in &decoded.profiles {
        let sid = server_id(&parsed.link);
        feed_ids.insert(sid.clone());
        let previous_sort_index = existing.get(&sid).copied();
        if previous_sort_index.is_some() {
            updated += 1;
        } else {
            added += 1;
        }
        profiles.push(Profile {
            id: format!("{sub_id}-{sid}"),
            subscription_id: sub_id.to_string(),
            server_id: sid,
            link: parsed.link.clone(),
            remarks: Some(parsed.remarks.clone()),
            protocol: Some(parsed.protocol.clone()),
            address: Some(parsed.address.clone()),
            port: Some(i64::from(parsed.port)),
            raw_config: parsed.raw_config.to_string(),
            present: true,
            latency_ms: None,
            reachable: false,
            checked_at: None,
            sort_index: previous_sort_index.unwrap_or(0),
            created_at: now,
            updated_at: now,
        });
    }

    profile_repo::upsert_many(conn, &profiles)?;

    let absent_ids: Vec<String> = existing
        .keys()
        .filter(|sid| !feed_ids.contains(*sid))
        .cloned()
        .collect();
    profile_repo::mark_absent(conn, sub_id, &absent_ids)?;

    let mut subscription = subscription_repo::get(conn, sub_id)?
        .ok_or(Error::QueryReturnedNoRows)?;
    subscription.status = "active".to_string();
    subscription.last_fetched_at = Some(now);
    subscription.last_error = None;
    subscription_repo::update(conn, &subscription)?;

    Ok(ImportSummary {
        added,
        updated,
        absent: absent_ids.len(),
        bad_lines: decoded.bad_lines,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rusqlite::Connection;

    use super::*;
    use crate::storage::migrate;
    use crate::storage::models::{Profile, Subscription};
    use crate::storage::repos::{profile_repo, subscription_repo};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        subscription_repo::insert(
            &conn,
            &Subscription {
                id: "sub-1".to_string(),
                ..Default::default()
            },
        )
        .expect("insert subscription");
        conn
    }

    fn vless_link(remarks: &str) -> String {
        format!("vless://uuid@example.com:443?security=tls#{remarks}")
    }

    #[test]
    fn upsert_existing_keeps_delay() {
        let mut conn = test_conn();
        let link = vless_link("One");
        let sid = server_id(&link);
        let checked_at = DateTime::parse_from_rfc3339("2026-11-24T00:00:00.000Z")
            .expect("parse timestamp")
            .with_timezone(&Utc);
        let profile = Profile {
            id: format!("sub-1-{sid}"),
            subscription_id: "sub-1".to_string(),
            server_id: sid,
            link: link.clone(),
            raw_config: "{}".to_string(),
            latency_ms: Some(42),
            reachable: true,
            checked_at: Some(checked_at),
            ..Default::default()
        };
        profile_repo::upsert_many(&conn, &[profile]).expect("insert profile");

        let summary = import_subscription_text(&mut conn, "sub-1", &link).expect("import");
        assert_eq!(
            summary,
            ImportSummary {
                added: 0,
                updated: 1,
                absent: 0,
                bad_lines: 0,
            }
        );

        let profiles = profile_repo::list_present(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].latency_ms, Some(42));
        assert!(profiles[0].reachable);
        assert_eq!(profiles[0].checked_at, Some(checked_at));
        assert_eq!(profiles[0].raw_config, "{\"security\":\"tls\"}");
    }

    #[test]
    fn upsert_existing_preserves_sort_index() {
        let mut conn = test_conn();
        let link = vless_link("One");
        let sid = server_id(&link);
        let profile = Profile {
            id: format!("sub-1-{sid}"),
            subscription_id: "sub-1".to_string(),
            server_id: sid,
            link: link.clone(),
            raw_config: "{}".to_string(),
            sort_index: 70,
            ..Default::default()
        };
        profile_repo::upsert_many(&conn, &[profile]).expect("insert profile");

        import_subscription_text(&mut conn, "sub-1", &link).expect("import");

        let profiles = profile_repo::list_present(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].sort_index, 70);
    }

    #[test]
    fn missing_links_marked_absent() {
        let mut conn = test_conn();
        let a = vless_link("One");
        let b = vless_link("Two");
        let first = import_subscription_text(&mut conn, "sub-1", &format!("{a}\n{b}"))
            .expect("first import");
        assert_eq!(first.added, 2);
        assert_eq!(first.absent, 0);

        let second = import_subscription_text(&mut conn, "sub-1", &a).expect("second import");
        assert_eq!(
            second,
            ImportSummary {
                added: 0,
                updated: 1,
                absent: 1,
                bad_lines: 0,
            }
        );

        let profiles = profile_repo::list_present(&conn).expect("list present");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].link, a);
    }

    #[test]
    fn bad_lines_counted() {
        let mut conn = test_conn();
        let summary =
            import_subscription_text(&mut conn, "sub-1", "garbage line").expect("import");
        assert_eq!(
            summary,
            ImportSummary {
                added: 0,
                updated: 0,
                absent: 0,
                bad_lines: 1,
            }
        );
        assert!(profile_repo::list_present(&conn)
            .expect("list present")
            .is_empty());
    }

    #[test]
    fn successful_import_clears_error_and_sets_timestamp() {
        let mut conn = test_conn();
        let mut sub = subscription_repo::get(&conn, "sub-1")
            .expect("get")
            .expect("subscription exists");
        sub.status = "error".to_string();
        sub.last_error = Some("boom".to_string());
        subscription_repo::update(&conn, &sub).expect("update subscription");

        let link = vless_link("One");
        import_subscription_text(&mut conn, "sub-1", &link).expect("import");

        let updated = subscription_repo::get(&conn, "sub-1")
            .expect("get")
            .expect("subscription exists");
        assert_eq!(updated.status, "active");
        assert_eq!(updated.last_error, None);
        assert!(updated.last_fetched_at.is_some());
    }
}
