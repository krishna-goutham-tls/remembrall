pub mod schema;
pub mod queries;

use rusqlite::{Connection, Result};
use std::path::PathBuf;

/// Open database connection in WAL mode
pub fn open_database(path: PathBuf) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    Ok(conn)
}
