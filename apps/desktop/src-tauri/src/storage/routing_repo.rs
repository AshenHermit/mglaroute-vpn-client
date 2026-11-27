use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result, Row};

use super::models::{RoutingProfile, RoutingRule};
use super::repos::{read_ts, ts_string};

fn profile_from_row(row: &Row<'_>) -> Result<RoutingProfile> {
    Ok(RoutingProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        enabled: row.get("enabled")?,
        is_active: row.get("is_active")?,
        sort: row.get("sort")?,
        created_at: read_ts(row, "created_at")?,
        updated_at: read_ts(row, "updated_at")?,
    })
}

fn rule_from_row(row: &Row<'_>) -> Result<RoutingRule> {
    Ok(RoutingRule {
        id: row.get("id")?,
        profile_id: row.get("profile_id")?,
        remark: row.get("remark")?,
        enabled: row.get("enabled")?,
        outbound: row.get("outbound")?,
        domains: row.get("domains")?,
        ips: row.get("ips")?,
        sort: row.get("sort")?,
        created_at: read_ts(row, "created_at")?,
        updated_at: read_ts(row, "updated_at")?,
    })
}

pub fn insert_profile(conn: &Connection, value: &RoutingProfile) -> Result<()> {
    conn.execute(
        "INSERT INTO routing_profiles \
         (id, name, enabled, is_active, sort, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            value.id,
            value.name,
            value.enabled,
            value.is_active,
            value.sort,
            ts_string(&value.created_at),
            ts_string(&value.updated_at),
        ],
    )?;
    Ok(())
}

pub fn get_profile(conn: &Connection, id: &str) -> Result<Option<RoutingProfile>> {
    conn.query_row(
        "SELECT * FROM routing_profiles WHERE id = ?1",
        params![id],
        profile_from_row,
    )
    .optional()
}

pub fn list_profiles(conn: &Connection) -> Result<Vec<RoutingProfile>> {
    let mut stmt = conn.prepare("SELECT * FROM routing_profiles ORDER BY sort, id")?;
    let rows = stmt.query_map([], profile_from_row)?;
    rows.collect()
}

pub fn get_active_profile(conn: &Connection) -> Result<Option<RoutingProfile>> {
    conn.query_row(
        "SELECT * FROM routing_profiles WHERE is_active = 1 AND enabled = 1 ORDER BY sort, id LIMIT 1",
        [],
        profile_from_row,
    )
    .optional()
}

pub fn exists_profile_by_name(
    conn: &Connection,
    name: &str,
    except_id: Option<&str>,
) -> Result<bool> {
    match except_id {
        Some(id) => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM routing_profiles WHERE name = ?1 AND id <> ?2)",
            params![name, id],
            |row| row.get::<_, bool>(0),
        ),
        None => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM routing_profiles WHERE name = ?1)",
            params![name],
            |row| row.get::<_, bool>(0),
        ),
    }
}

pub fn next_profile_sort(conn: &Connection) -> Result<i64> {
    let max: Option<i64> = conn.query_row("SELECT MAX(sort) FROM routing_profiles", [], |row| {
        row.get(0)
    })?;
    Ok(max.unwrap_or(0) + 10)
}

pub fn update_profile(
    conn: &Connection,
    id: &str,
    name: &str,
    enabled: bool,
) -> Result<()> {
    let now = ts_string(&Utc::now());
    conn.execute(
        "UPDATE routing_profiles SET name = ?2, enabled = ?3, \
         is_active = CASE WHEN ?3 = 0 THEN 0 ELSE is_active END, \
         updated_at = ?4 WHERE id = ?1",
        params![id, name, enabled, now],
    )?;
    Ok(())
}

pub fn activate_profile(conn: &Connection, id: &str) -> Result<bool> {
    let enabled: Option<bool> = conn
        .query_row(
            "SELECT enabled FROM routing_profiles WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    if !enabled.unwrap_or(false) {
        return Ok(false);
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE routing_profiles SET is_active = 0", [])?;
    let now = ts_string(&Utc::now());
    let changed = tx.execute(
        "UPDATE routing_profiles SET is_active = 1, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(changed > 0)
}

pub fn remove_profile(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM routing_rules WHERE profile_id = ?1", params![id])?;
    tx.execute("DELETE FROM routing_profiles WHERE id = ?1", params![id])?;
    tx.commit()
}

pub fn insert_rule(conn: &Connection, value: &RoutingRule) -> Result<()> {
    conn.execute(
        "INSERT INTO routing_rules \
         (id, profile_id, remark, enabled, outbound, domains, ips, sort, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            value.id,
            value.profile_id,
            value.remark,
            value.enabled,
            value.outbound,
            value.domains,
            value.ips,
            value.sort,
            ts_string(&value.created_at),
            ts_string(&value.updated_at),
        ],
    )?;
    Ok(())
}

pub fn get_rule(conn: &Connection, id: &str) -> Result<Option<RoutingRule>> {
    conn.query_row(
        "SELECT * FROM routing_rules WHERE id = ?1",
        params![id],
        rule_from_row,
    )
    .optional()
}

pub fn list_rules(conn: &Connection, profile_id: &str) -> Result<Vec<RoutingRule>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM routing_rules WHERE profile_id = ?1 ORDER BY sort, id",
    )?;
    let rows = stmt.query_map(params![profile_id], rule_from_row)?;
    rows.collect()
}

pub fn list_active_rules(conn: &Connection) -> Result<Vec<RoutingRule>> {
    let mut stmt = conn.prepare(
        "SELECT r.* FROM routing_rules r \
         JOIN routing_profiles p ON p.id = r.profile_id \
         WHERE r.enabled = 1 AND p.enabled = 1 AND p.is_active = 1 \
         ORDER BY r.sort, r.id",
    )?;
    let rows = stmt.query_map([], rule_from_row)?;
    rows.collect()
}

pub fn next_rule_sort(conn: &Connection, profile_id: &str) -> Result<i64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(sort) FROM routing_rules WHERE profile_id = ?1",
        params![profile_id],
        |row| row.get(0),
    )?;
    Ok(max.unwrap_or(0) + 10)
}

pub fn update_rule(
    conn: &Connection,
    id: &str,
    remark: &str,
    enabled: bool,
    outbound: &str,
    domains: &[String],
    ips: &[String],
) -> Result<()> {
    let domains_json = serde_json::to_string(domains)
        .map_err(|_| rusqlite::Error::InvalidParameterName("domains".to_string()))?;
    let ips_json = serde_json::to_string(ips)
        .map_err(|_| rusqlite::Error::InvalidParameterName("ips".to_string()))?;
    let now = ts_string(&Utc::now());
    conn.execute(
        "UPDATE routing_rules SET remark = ?2, enabled = ?3, outbound = ?4, \
         domains = ?5, ips = ?6, updated_at = ?7 WHERE id = ?1",
        params![id, remark, enabled, outbound, domains_json, ips_json, now],
    )?;
    Ok(())
}

pub fn remove_rule(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM routing_rules WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn reorder_rule(conn: &Connection, id: &str, direction: i32) -> Result<()> {
    if direction != -1 && direction != 1 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let profile_id: Option<String> = tx
        .query_row(
            "SELECT profile_id FROM routing_rules WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(profile_id) = profile_id else {
        return Ok(());
    };
    let ordered: Vec<(String, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT id, sort FROM routing_rules WHERE profile_id = ?1 ORDER BY sort, id",
        )?;
        let rows = stmt.query_map(params![&profile_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<Result<Vec<_>>>()?
    };
    let Some(index) = ordered.iter().position(|(candidate, _)| candidate == id) else {
        return Ok(());
    };
    let neighbor_index = if direction < 0 {
        index.checked_sub(1)
    } else {
        ordered.get(index + 1).map(|_| index + 1)
    };
    let Some(neighbor_index) = neighbor_index else {
        return Ok(());
    };
    let now = ts_string(&Utc::now());
    let current_sort = ordered[index].1;
    let neighbor_sort = ordered[neighbor_index].1;
    tx.execute(
        "UPDATE routing_rules SET sort = ?2, updated_at = ?3 WHERE id = ?1",
        params![ordered[index].0, neighbor_sort, now],
    )?;
    tx.execute(
        "UPDATE routing_rules SET sort = ?2, updated_at = ?3 WHERE id = ?1",
        params![ordered[neighbor_index].0, current_sort, now],
    )?;
    tx.commit()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::storage::migrate;
    use crate::storage::models::{RoutingProfile, RoutingRule};
    use crate::storage::routing_repo;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        conn
    }

    fn profile(id: &str, name: &str, sort: i64) -> RoutingProfile {
        RoutingProfile {
            id: id.to_string(),
            name: name.to_string(),
            sort,
            ..Default::default()
        }
    }

    fn rule(id: &str, profile_id: &str, sort: i64, outbound: &str) -> RoutingRule {
        RoutingRule {
            id: id.to_string(),
            profile_id: profile_id.to_string(),
            sort,
            outbound: outbound.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn fresh_db_creates_routing_tables() {
        let conn = fresh_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
                 AND name IN ('routing_profiles', 'routing_rules')",
                [],
                |row| row.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(count, 2);
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, 2);
    }

    #[test]
    fn profile_crud_and_unique_name() {
        let conn = fresh_db();
        routing_repo::insert_profile(&conn, &profile("p1", "alpha", 10)).expect("insert p1");
        routing_repo::insert_profile(&conn, &profile("p2", "beta", 20)).expect("insert p2");

        let list = routing_repo::list_profiles(&conn).expect("list profiles");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "p1");
        assert_eq!(list[1].id, "p2");
        assert_eq!(
            routing_repo::get_profile(&conn, "p2")
                .expect("get p2")
                .expect("p2 exists")
                .name,
            "beta"
        );
        assert!(routing_repo::exists_profile_by_name(&conn, "alpha", None).expect("exists"));
        assert!(!routing_repo::exists_profile_by_name(&conn, "alpha", Some("p1")).expect("exists"));
        assert!(routing_repo::exists_profile_by_name(&conn, "alpha", Some("p2")).expect("exists"));

        routing_repo::update_profile(&conn, "p1", "gamma", true).expect("rename p1");
        assert!(!routing_repo::exists_profile_by_name(&conn, "alpha", None).expect("exists"));
        assert!(routing_repo::exists_profile_by_name(&conn, "gamma", None).expect("exists"));

        routing_repo::remove_profile(&conn, "p1").expect("remove p1");
        assert!(routing_repo::get_profile(&conn, "p1")
            .expect("get p1")
            .is_none());
    }

    #[test]
    fn only_one_active_profile_and_disabled_cannot_activate() {
        let conn = fresh_db();
        routing_repo::insert_profile(&conn, &profile("p1", "alpha", 10)).expect("insert p1");
        routing_repo::insert_profile(&conn, &profile("p2", "beta", 20)).expect("insert p2");

        assert!(routing_repo::activate_profile(&conn, "p1").expect("activate p1"));
        assert!(routing_repo::get_profile(&conn, "p1")
            .expect("get p1")
            .unwrap()
            .is_active);
        assert!(routing_repo::activate_profile(&conn, "p2").expect("activate p2"));
        assert!(!routing_repo::get_profile(&conn, "p1")
            .expect("get p1")
            .unwrap()
            .is_active);

        routing_repo::update_profile(&conn, "p1", "alpha", false).expect("disable p1");
        assert!(!routing_repo::get_profile(&conn, "p1")
            .expect("get p1")
            .unwrap()
            .is_active);
        assert!(!routing_repo::activate_profile(&conn, "p1").expect("activate disabled p1"));
        assert!(!routing_repo::activate_profile(&conn, "missing").expect("activate missing"));

        assert!(routing_repo::activate_profile(&conn, "p2").expect("activate p2 again"));
        routing_repo::update_profile(&conn, "p2", "beta", false).expect("disable p2");
        assert!(!routing_repo::get_profile(&conn, "p2")
            .expect("get p2")
            .unwrap()
            .is_active);
        assert!(routing_repo::get_active_profile(&conn)
            .expect("active profile")
            .is_none());

        routing_repo::update_profile(&conn, "p1", "alpha", true).expect("enable p1");
        assert!(routing_repo::activate_profile(&conn, "p1").expect("activate p1"));
        routing_repo::remove_profile(&conn, "p1").expect("remove active p1");
        assert!(routing_repo::get_active_profile(&conn)
            .expect("active profile")
            .is_none());
    }

    #[test]
    fn remove_profile_deletes_rules() {
        let conn = fresh_db();
        routing_repo::insert_profile(&conn, &profile("p1", "alpha", 10)).expect("insert p1");
        routing_repo::insert_rule(&conn, &rule("r1", "p1", 10, "proxy")).expect("insert r1");
        routing_repo::insert_rule(&conn, &rule("r2", "p1", 20, "direct")).expect("insert r2");

        routing_repo::remove_profile(&conn, "p1").expect("remove p1");
        assert!(routing_repo::get_rule(&conn, "r1")
            .expect("get r1")
            .is_none());
        assert!(routing_repo::get_rule(&conn, "r2")
            .expect("get r2")
            .is_none());
        assert!(routing_repo::list_rules(&conn, "p1")
            .expect("list rules")
            .is_empty());
    }

    #[test]
    fn rules_order_and_active_filter() {
        let conn = fresh_db();
        let mut p1 = profile("p1", "alpha", 10);
        p1.enabled = true;
        routing_repo::insert_profile(&conn, &p1).expect("insert p1");
        let mut p2 = profile("p2", "beta", 20);
        p2.enabled = false;
        routing_repo::insert_profile(&conn, &p2).expect("insert p2");
        routing_repo::activate_profile(&conn, "p1").expect("activate p1");

        routing_repo::insert_rule(&conn, &rule("r2", "p1", 20, "proxy")).expect("insert r2");
        routing_repo::insert_rule(&conn, &rule("r1", "p1", 10, "direct")).expect("insert r1");
        let mut disabled = rule("r3", "p1", 30, "block");
        disabled.enabled = false;
        routing_repo::insert_rule(&conn, &disabled).expect("insert r3");
        routing_repo::insert_rule(&conn, &rule("r4", "p2", 10, "proxy")).expect("insert r4");

        let p1_rules = routing_repo::list_rules(&conn, "p1").expect("list p1 rules");
        let ids: Vec<_> = p1_rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["r1", "r2", "r3"]);

        routing_repo::update_rule(
            &conn,
            "r1",
            "remark-1",
            true,
            "proxy",
            &["example.com".to_string()],
            &["192.0.2.1".to_string()],
        )
        .expect("update r1");
        let updated = routing_repo::get_rule(&conn, "r1")
            .expect("get r1")
            .unwrap();
        assert_eq!(updated.remark, "remark-1");
        assert_eq!(updated.outbound, "proxy");
        assert_eq!(updated.domains, "[\"example.com\"]");
        assert_eq!(updated.ips, "[\"192.0.2.1\"]");

        let active = routing_repo::list_active_rules(&conn).expect("list active rules");
        let active_ids: Vec<_> = active.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(active_ids, vec!["r1", "r2"]);
    }

    #[test]
    fn reorder_rule_swaps_neighbors_and_clamps() {
        let conn = fresh_db();
        routing_repo::insert_profile(&conn, &profile("p1", "alpha", 10)).expect("insert p1");
        routing_repo::insert_rule(&conn, &rule("first", "p1", 10, "proxy")).expect("insert first");
        routing_repo::insert_rule(&conn, &rule("middle", "p1", 20, "proxy")).expect("insert middle");
        routing_repo::insert_rule(&conn, &rule("last", "p1", 30, "proxy")).expect("insert last");

        routing_repo::reorder_rule(&conn, "middle", -1).expect("move middle up");
        let ids = |conn: &Connection| -> Vec<String> {
            routing_repo::list_rules(conn, "p1")
                .expect("list rules")
                .into_iter()
                .map(|r| r.id)
                .collect()
        };
        assert_eq!(ids(&conn), vec!["middle", "first", "last"]);

        routing_repo::reorder_rule(&conn, "middle", -1).expect("clamp first");
        assert_eq!(ids(&conn), vec!["middle", "first", "last"]);
        routing_repo::reorder_rule(&conn, "last", 1).expect("clamp last");
        assert_eq!(ids(&conn), vec!["middle", "first", "last"]);
        routing_repo::reorder_rule(&conn, "first", 1).expect("move first down");
        assert_eq!(ids(&conn), vec!["middle", "last", "first"]);
    }
}
