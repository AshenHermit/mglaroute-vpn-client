pub mod models;
pub mod repos;

use rusqlite::{Connection, Result};

pub fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("schema.sql"))?;
    tx.commit()
}
