use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, types::Type, Connection, Error, OptionalExtension, Result, Row, ToSql};

use super::models::{Profile, ProfileServiceResult, ServiceProfile, Settings, Subscription};

fn ts_string(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn ts_param(value: &Option<DateTime<Utc>>) -> Option<String> {
    value.as_ref().map(ts_string)
}

fn read_ts_opt(row: &Row<'_>, column: &str) -> Result<Option<DateTime<Utc>>> {
    let raw: Option<String> = row.get(column)?;
    match raw {
        None => Ok(None),
        Some(value) => DateTime::parse_from_rfc3339(&value)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .map_err(|_| Error::InvalidColumnType(0, column.to_string(), Type::Text)),
    }
}

fn read_ts(row: &Row<'_>, column: &str) -> Result<DateTime<Utc>> {
    read_ts_opt(row, column)?
        .ok_or_else(|| Error::InvalidColumnType(0, column.to_string(), Type::Null))
}

fn subscription_from_row(row: &Row<'_>) -> Result<Subscription> {
    Ok(Subscription {
        id: row.get("id")?,
        name: row.get("name")?,
        url: row.get("url")?,
        enabled: row.get("enabled")?,
        status: row.get("status")?,
        auto_refetch_minutes: row.get("auto_refetch_minutes")?,
        last_fetched_at: read_ts_opt(row, "last_fetched_at")?,
        last_error: row.get("last_error")?,
        user_agent: row.get("user_agent")?,
        request_headers: row.get("request_headers")?,
        sort: row.get("sort")?,
        created_at: read_ts(row, "created_at")?,
        updated_at: read_ts(row, "updated_at")?,
    })
}

fn profile_from_row(row: &Row<'_>) -> Result<Profile> {
    Ok(Profile {
        id: row.get("id")?,
        subscription_id: row.get("subscription_id")?,
        server_id: row.get("server_id")?,
        link: row.get("link")?,
        remarks: row
            .get::<_, Option<String>>("remarks")?
            .map(|value| crate::subscription::decode::percent_decode(&value)),
        protocol: row.get("protocol")?,
        address: row.get("address")?,
        port: row.get("port")?,
        raw_config: row.get("raw_config")?,
        present: row.get("present")?,
        latency_ms: row.get("latency_ms")?,
        reachable: row.get("reachable")?,
        checked_at: read_ts_opt(row, "checked_at")?,
        sort_index: row.get("sort_index")?,
        created_at: read_ts(row, "created_at")?,
        updated_at: read_ts(row, "updated_at")?,
    })
}

fn service_profile_from_row(row: &Row<'_>) -> Result<ServiceProfile> {
    Ok(ServiceProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        test_urls: row.get("test_urls")?,
        is_enabled: row.get("is_enabled")?,
        is_default: row.get("is_default")?,
        created_at: read_ts(row, "created_at")?,
        updated_at: read_ts(row, "updated_at")?,
    })
}

fn service_result_from_row(row: &Row<'_>) -> Result<ProfileServiceResult> {
    Ok(ProfileServiceResult {
        profile_id: row.get("profile_id")?,
        service_profile_id: row.get("service_profile_id")?,
        reachable: row.get("reachable")?,
        checked_at: read_ts(row, "checked_at")?,
    })
}

fn settings_from_row(row: &Row<'_>) -> Result<Settings> {
    Ok(Settings {
        auto_refetch_enabled: row.get("auto_refetch_enabled")?,
        refetch_interval_minutes: row.get("refetch_interval_minutes")?,
        auto_test_enabled: row.get("auto_test_enabled")?,
        test_interval_minutes: row.get("test_interval_minutes")?,
        test_timeout_ms: row.get("test_timeout_ms")?,
        test_concurrency: row.get("test_concurrency")?,
        auto_reconnect_enabled: row.get("auto_reconnect_enabled")?,
        reconnect_cooldown_seconds: row.get("reconnect_cooldown_seconds")?,
        ping_url: row.get("ping_url")?,
        local_proxy_port: row.get("local_proxy_port")?,
        proxy_exclusions: row.get("proxy_exclusions")?,
        system_proxy_enabled: row.get("system_proxy_enabled")?,
    })
}

pub mod subscription_repo {
    use super::*;

    pub fn insert(conn: &Connection, value: &Subscription) -> Result<()> {
        conn.execute(
            "INSERT INTO subscriptions (id, name, url, enabled, status, auto_refetch_minutes, \
             last_fetched_at, last_error, user_agent, request_headers, sort, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                value.id,
                value.name,
                value.url,
                value.enabled,
                value.status,
                value.auto_refetch_minutes,
                ts_param(&value.last_fetched_at),
                value.last_error,
                value.user_agent,
                value.request_headers,
                value.sort,
                ts_string(&value.created_at),
                ts_string(&value.updated_at),
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<Subscription>> {
        conn.query_row(
            "SELECT * FROM subscriptions WHERE id = ?1",
            params![id],
            subscription_from_row,
        )
        .optional()
    }

    pub fn list(conn: &Connection) -> Result<Vec<Subscription>> {
        let mut stmt = conn.prepare("SELECT * FROM subscriptions ORDER BY sort, id")?;
        let rows = stmt.query_map([], subscription_from_row)?;
        rows.collect()
    }

    pub fn update(conn: &Connection, value: &Subscription) -> Result<()> {
        conn.execute(
            "UPDATE subscriptions SET name = ?2, url = ?3, enabled = ?4, status = ?5, \
             auto_refetch_minutes = ?6, last_fetched_at = ?7, last_error = ?8, user_agent = ?9, \
             request_headers = ?10, sort = ?11, updated_at = ?12 WHERE id = ?1",
            params![
                value.id,
                value.name,
                value.url,
                value.enabled,
                value.status,
                value.auto_refetch_minutes,
                ts_param(&value.last_fetched_at),
                value.last_error,
                value.user_agent,
                value.request_headers,
                value.sort,
                ts_string(&Utc::now()),
            ],
        )?;
        Ok(())
    }

    pub fn remove(conn: &Connection, id: &str) -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM profile_service_results WHERE profile_id IN \
             (SELECT id FROM profiles WHERE subscription_id = ?1)",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM profiles WHERE subscription_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM subscriptions WHERE id = ?1", params![id])?;
        tx.commit()
    }
}

pub mod profile_repo {
    use super::*;

    pub fn upsert_many(conn: &Connection, profiles: &[Profile]) -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO profiles (id, subscription_id, server_id, link, remarks, protocol, \
                 address, port, raw_config, present, latency_ms, reachable, checked_at, sort_index, \
                 created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16) \
                 ON CONFLICT(subscription_id, server_id) DO UPDATE SET \
                 link = excluded.link, remarks = excluded.remarks, protocol = excluded.protocol, \
                 address = excluded.address, port = excluded.port, raw_config = excluded.raw_config, \
                 present = excluded.present, sort_index = excluded.sort_index, \
                 updated_at = excluded.updated_at",
            )?;
            for value in profiles {
                stmt.execute(params![
                    value.id,
                    value.subscription_id,
                    value.server_id,
                    value.link,
                    value.remarks,
                    value.protocol,
                    value.address,
                    value.port,
                    value.raw_config,
                    value.present,
                    value.latency_ms,
                    value.reachable,
                    ts_param(&value.checked_at),
                    value.sort_index,
                    ts_string(&value.created_at),
                    ts_string(&value.updated_at),
                ])?;
            }
        }
        tx.commit()
    }

    pub fn list_present(conn: &Connection) -> Result<Vec<Profile>> {
        let mut stmt =
            conn.prepare("SELECT * FROM profiles WHERE present = 1 ORDER BY sort_index, id")?;
        let rows = stmt.query_map([], profile_from_row)?;
        rows.collect()
    }

    /// Present profiles whose subscription is enabled. Probe selection and
    /// auto-connect must never use servers from a disabled subscription;
    /// import and UI paths keep `list_present` to see every stored profile.
    pub fn list_present_enabled(conn: &Connection) -> Result<Vec<Profile>> {
        let mut stmt = conn.prepare(
            "SELECT p.* FROM profiles p \
             JOIN subscriptions s ON s.id = p.subscription_id \
             WHERE p.present = 1 AND s.enabled = 1 \
             ORDER BY p.sort_index, p.id",
        )?;
        let rows = stmt.query_map([], profile_from_row)?;
        rows.collect()
    }

    /// Mark the given server IDs as absent for a subscription.
    pub fn mark_absent(
        conn: &Connection,
        subscription_id: &str,
        server_ids: &[String],
    ) -> Result<()> {
        if server_ids.is_empty() {
            return Ok(());
        }
        let placeholders = (0..server_ids.len())
            .map(|index| format!("?{}", index + 3))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE profiles SET present = 0, updated_at = ?1 \
             WHERE subscription_id = ?2 AND server_id IN ({placeholders})"
        );
        let now = ts_string(&Utc::now());
        let mut params: Vec<&dyn ToSql> = vec![&now, &subscription_id];
        for server_id in server_ids {
            params.push(server_id);
        }
        conn.execute(&sql, params.as_slice())?;
        Ok(())
    }

    pub fn sort_all(conn: &Connection, profile_ids: &[String]) -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        for (index, id) in profile_ids.iter().enumerate() {
            tx.execute(
                "UPDATE profiles SET sort_index = ?2 WHERE id = ?1",
                params![id, ((index + 1) * 10) as i64],
            )?;
        }
        tx.commit()
    }

    /// Cache a delay probe result on the profile row.
    pub fn set_result(
        conn: &Connection,
        profile_id: &str,
        latency_ms: Option<u64>,
        reachable: bool,
        checked_at: DateTime<Utc>,
    ) -> Result<()> {
        conn.execute(
            "UPDATE profiles SET latency_ms = ?2, reachable = ?3, checked_at = ?4, \
             updated_at = ?5 WHERE id = ?1",
            params![
                profile_id,
                latency_ms.map(|value| value as i64),
                reachable,
                ts_string(&checked_at),
                ts_string(&Utc::now()),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_service_result(
        conn: &Connection,
        profile_id: &str,
        service_profile_id: &str,
        reachable: bool,
        checked_at: DateTime<Utc>,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO profile_service_results (profile_id, service_profile_id, reachable, checked_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(profile_id, service_profile_id) DO UPDATE SET \
             reachable = excluded.reachable, checked_at = excluded.checked_at",
            params![profile_id, service_profile_id, reachable, ts_string(&checked_at)],
        )?;
        Ok(())
    }

    pub fn list_service_results(conn: &Connection) -> Result<Vec<ProfileServiceResult>> {
        let mut stmt = conn.prepare("SELECT * FROM profile_service_results")?;
        let rows = stmt.query_map([], service_result_from_row)?;
        rows.collect()
    }
}

pub mod service_profile_repo {
    use super::*;

    pub fn insert(conn: &Connection, value: &ServiceProfile) -> Result<()> {
        conn.execute(
            "INSERT INTO service_profiles (id, name, test_urls, is_enabled, is_default, \
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                value.id,
                value.name,
                value.test_urls,
                value.is_enabled,
                value.is_default,
                ts_string(&value.created_at),
                ts_string(&value.updated_at),
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<ServiceProfile>> {
        conn.query_row(
            "SELECT * FROM service_profiles WHERE id = ?1",
            params![id],
            service_profile_from_row,
        )
        .optional()
    }

    pub fn exists_by_name(conn: &Connection, name: &str) -> Result<bool> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_profiles WHERE name = ?1)",
            params![name],
            |row| row.get::<_, bool>(0),
        )
    }

    pub fn list(conn: &Connection) -> Result<Vec<ServiceProfile>> {
        let mut stmt = conn.prepare("SELECT * FROM service_profiles ORDER BY name, id")?;
        let rows = stmt.query_map([], service_profile_from_row)?;
        rows.collect()
    }

    pub fn list_enabled(conn: &Connection) -> Result<Vec<ServiceProfile>> {
        let mut stmt =
            conn.prepare("SELECT * FROM service_profiles WHERE is_enabled = 1 ORDER BY name, id")?;
        let rows = stmt.query_map([], service_profile_from_row)?;
        rows.collect()
    }

    pub fn update(conn: &Connection, value: &ServiceProfile) -> Result<()> {
        conn.execute(
            "UPDATE service_profiles SET name = ?2, test_urls = ?3, is_enabled = ?4, \
             is_default = ?5, updated_at = ?6 WHERE id = ?1",
            params![
                value.id,
                value.name,
                value.test_urls,
                value.is_enabled,
                value.is_default,
                ts_string(&Utc::now()),
            ],
        )?;
        Ok(())
    }

    pub fn remove(conn: &Connection, id: &str) -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM profile_service_results WHERE service_profile_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM service_profiles WHERE id = ?1", params![id])?;
        tx.commit()
    }
}

pub mod settings_repo {
    use super::*;

    fn insert_row(conn: &Connection, value: &Settings) -> Result<()> {
        conn.execute(
            "INSERT INTO settings (id, auto_refetch_enabled, refetch_interval_minutes, \
             auto_test_enabled, test_interval_minutes, test_timeout_ms, test_concurrency, \
             auto_reconnect_enabled, reconnect_cooldown_seconds, ping_url, local_proxy_port, \
             proxy_exclusions, system_proxy_enabled) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                value.auto_refetch_enabled,
                value.refetch_interval_minutes,
                value.auto_test_enabled,
                value.test_interval_minutes,
                value.test_timeout_ms,
                value.test_concurrency,
                value.auto_reconnect_enabled,
                value.reconnect_cooldown_seconds,
                value.ping_url,
                value.local_proxy_port,
                value.proxy_exclusions,
                value.system_proxy_enabled,
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection) -> Result<Settings> {
        let existing = conn
            .query_row("SELECT * FROM settings WHERE id = 1", [], settings_from_row)
            .optional()?;
        if let Some(value) = existing {
            return Ok(value);
        }
        let value = Settings::default();
        insert_row(conn, &value)?;
        Ok(value)
    }

    pub fn update(conn: &Connection, value: &Settings) -> Result<()> {
        let changed = conn.execute(
            "UPDATE settings SET auto_refetch_enabled = ?1, refetch_interval_minutes = ?2, \
             auto_test_enabled = ?3, test_interval_minutes = ?4, test_timeout_ms = ?5, \
             test_concurrency = ?6, auto_reconnect_enabled = ?7, reconnect_cooldown_seconds = ?8, \
             ping_url = ?9, local_proxy_port = ?10, proxy_exclusions = ?11, \
             system_proxy_enabled = ?12 \
             WHERE id = 1",
            params![
                value.auto_refetch_enabled,
                value.refetch_interval_minutes,
                value.auto_test_enabled,
                value.test_interval_minutes,
                value.test_timeout_ms,
                value.test_concurrency,
                value.auto_reconnect_enabled,
                value.reconnect_cooldown_seconds,
                value.ping_url,
                value.local_proxy_port,
                value.proxy_exclusions,
                value.system_proxy_enabled,
            ],
        )?;
        if changed == 0 {
            insert_row(conn, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rusqlite::Connection;

    use crate::storage::migrate;
    use crate::storage::models::{Profile, ServiceProfile, Subscription};
    use crate::storage::repos::{profile_repo, service_profile_repo, subscription_repo};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        conn
    }

    fn test_subscription(id: &str) -> Subscription {
        Subscription {
            id: id.to_string(),
            name: "test".to_string(),
            url: "https://example.com/sub".to_string(),
            ..Default::default()
        }
    }

    fn test_profile(subscription_id: &str, server_id: &str, raw_config: &str) -> Profile {
        Profile {
            id: format!("p-{server_id}"),
            subscription_id: subscription_id.to_string(),
            server_id: server_id.to_string(),
            link: format!("vmess://{server_id}"),
            raw_config: raw_config.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn migrate_creates_all_tables() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
                 AND name IN ('subscriptions','profiles','service_profiles', \
                 'profile_service_results','settings')",
                [],
                |row| row.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(count, 5);
    }

    #[test]
    fn profile_upsert_preserves_latency() {
        let conn = test_conn();
        subscription_repo::insert(&conn, &test_subscription("sub-1")).expect("insert subscription");

        let checked_at = DateTime::parse_from_rfc3339("2026-11-24T00:00:00.000Z")
            .expect("parse timestamp")
            .with_timezone(&Utc);
        let mut profile = test_profile("sub-1", "server-1", "{\"port\":1}");
        profile.latency_ms = Some(42);
        profile.reachable = true;
        profile.checked_at = Some(checked_at);
        profile_repo::upsert_many(&conn, &[profile]).expect("insert profile");

        profile_repo::upsert_many(&conn, &[test_profile("sub-1", "server-1", "{\"port\":2}")])
            .expect("upsert profile");

        let profiles = profile_repo::list_present(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].latency_ms, Some(42));
        assert!(profiles[0].reachable);
        assert_eq!(profiles[0].checked_at, Some(checked_at));
        assert_eq!(profiles[0].raw_config, "{\"port\":2}");
    }

    #[test]
    fn list_decodes_percent_encoded_remarks() {
        let conn = test_conn();
        subscription_repo::insert(&conn, &test_subscription("sub-1")).expect("insert subscription");

        let mut profile = test_profile("sub-1", "server-1", "{}");
        profile.remarks = Some(
            "%F0%9F%87%B8%F0%9F%87%AA%20Sweden%2C%20Stockholm%20%7C%20%5BBL%5D".to_string(),
        );
        profile_repo::upsert_many(&conn, &[profile]).expect("insert profile");

        let profiles = profile_repo::list_present(&conn).expect("list profiles");
        assert_eq!(profiles.len(), 1);
        assert_eq!(
            profiles[0].remarks.as_deref(),
            Some("🇸🇪 Sweden, Stockholm | [BL]")
        );
    }

    #[test]
    fn profile_mark_absent() {
        let conn = test_conn();
        subscription_repo::insert(&conn, &test_subscription("sub-1")).expect("insert subscription");
        profile_repo::upsert_many(
            &conn,
            &[
                test_profile("sub-1", "server-1", "{}"),
                test_profile("sub-1", "server-2", "{}"),
            ],
        )
        .expect("insert profiles");

        profile_repo::mark_absent(&conn, "sub-1", &["server-2".to_string()]).expect("mark absent");

        let profiles = profile_repo::list_present(&conn).expect("list present");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].server_id, "server-1");
    }

    #[test]
    fn service_profile_unique_name() {
        let conn = test_conn();
        service_profile_repo::insert(
            &conn,
            &ServiceProfile {
                id: "sp-1".to_string(),
                name: "YouTube".to_string(),
                ..Default::default()
            },
        )
        .expect("insert service profile");

        let duplicate = service_profile_repo::insert(
            &conn,
            &ServiceProfile {
                id: "sp-2".to_string(),
                name: "YouTube".to_string(),
                ..Default::default()
            },
        );
        assert!(duplicate.is_err());
    }
}
