pub mod models;
pub mod repos;
pub mod routing_repo;

use rusqlite::{Connection, Result};

pub fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 1 {
        tx.execute_batch(
            "ALTER TABLE settings ADD COLUMN test_concurrency INTEGER NOT NULL DEFAULT 10;
             PRAGMA user_version = 2;",
        )?;
    }
    tx.execute_batch(include_str!("schema.sql"))?;
    tx.commit()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::migrate;

    #[test]
    fn migrates_v1_settings_to_test_concurrency() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("v1.db")).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 1;
             CREATE TABLE settings (id INTEGER PRIMARY KEY CHECK (id = 1),
               auto_refetch_enabled INTEGER NOT NULL DEFAULT 0,
               refetch_interval_minutes INTEGER NOT NULL DEFAULT 15,
               auto_test_enabled INTEGER NOT NULL DEFAULT 0,
               test_interval_minutes INTEGER NOT NULL DEFAULT 15,
               test_timeout_ms INTEGER NOT NULL DEFAULT 5000,
               auto_reconnect_enabled INTEGER NOT NULL DEFAULT 0,
               reconnect_cooldown_seconds INTEGER NOT NULL DEFAULT 60,
               ping_url TEXT NOT NULL DEFAULT 'x',
               local_proxy_port INTEGER NOT NULL DEFAULT 10808,
               proxy_exclusions TEXT NOT NULL DEFAULT '[]',
               system_proxy_enabled INTEGER NOT NULL DEFAULT 1);
             INSERT INTO settings (id) VALUES (1);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, 2);
        let value: i64 = conn
            .query_row(
                "SELECT test_concurrency FROM settings WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, 10);
    }
}
