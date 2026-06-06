pub mod queries;
pub mod schema;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

use schema::{get_database_path, initialize_database};

/// Open database connection with WAL mode and run migrations
pub fn open_database() -> Result<Connection> {
    let path = get_database_path().context("Failed to get database path")?;
    initialize_database(&path)
}

/// Get the database path (for use by MCP server)
pub fn get_db_path() -> Result<PathBuf> {
    get_database_path()
}
