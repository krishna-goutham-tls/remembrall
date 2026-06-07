//! Database schema creation and migrations
//!
//! Implements forward-only migrations via PRAGMA user_version with ordered SQL scripts
//! in migrations/. All migrations are idempotent (safe to run twice).

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

// Import sqlite-vec's init function to register the extension
use sqlite_vec::sqlite3_vec_init;

/// Current schema version - must match the highest migration number
const CURRENT_SCHEMA_VERSION: u32 = 6;

/// Migration files in order
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_initial_schema.sql",
        include_str!("../../../migrations/001_initial_schema.sql"),
    ),
    (
        "002_add_fts5.sql",
        include_str!("../../../migrations/002_add_fts5.sql"),
    ),
    (
        "003_add_vec0.sql",
        include_str!("../../../migrations/003_add_vec0.sql"),
    ),
    (
        "004_seed_memory_types.sql",
        include_str!("../../../migrations/004_seed_memory_types.sql"),
    ),
    (
        "005_add_session_message_count.sql",
        include_str!("../../../migrations/005_add_session_message_count.sql"),
    ),
    (
        "006_add_app_settings_and_update_vec.sql",
        include_str!("../../../migrations/006_add_app_settings_and_update_vec.sql"),
    ),
];

/// Initialize database with WAL mode and run all pending migrations
pub fn initialize_database(path: &Path) -> Result<Connection> {
    // Register sqlite-vec extension with SQLite before opening any connections
    // This must happen before running migrations that use vec0
    // The transmute is safe here because sqlite3_vec_init has a known fixed signature
    #[allow(clippy::missing_transmute_annotations)]
    unsafe {
        use rusqlite::ffi::sqlite3_auto_extension;
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    log::info!("sqlite-vec extension registered");

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create database directory")?;
    }

    // Open connection
    let conn = Connection::open(path).context("Failed to open database")?;

    // Enable WAL mode for concurrent reads
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .context("Failed to enable WAL mode")?;

    // Run pending migrations
    run_migrations(&conn)?;

    log::info!(
        "Database initialized at {:?} with schema version {}",
        path,
        CURRENT_SCHEMA_VERSION
    );

    Ok(conn)
}

/// Run all pending migrations (forward-only, idempotent)
fn run_migrations(conn: &Connection) -> Result<()> {
    let current_version = get_user_version(conn)?;

    log::info!(
        "Current schema version: {}, target: {}",
        current_version,
        CURRENT_SCHEMA_VERSION
    );

    for (i, (filename, sql)) in MIGRATIONS.iter().enumerate() {
        let migration_version = (i + 1) as u32;

        if migration_version > current_version {
            // Backup before migration (for safety)
            // Note: In production, we'd backup to brain.db.bak
            log::info!("Running migration {} ({})", migration_version, filename);

            // Execute migration in transaction
            conn.execute_batch("BEGIN IMMEDIATE;")
                .context("Failed to begin migration transaction")?;

            match execute_migration(conn, sql) {
                Ok(()) => {
                    // Update user_version within the same transaction
                    set_user_version(conn, migration_version)?;

                    conn.execute_batch("COMMIT;")
                        .context("Failed to commit migration")?;

                    log::info!("Migration {} completed successfully", migration_version);
                }
                Err(e) => {
                    conn.execute_batch("ROLLBACK;")
                        .context("Failed to rollback migration")?;
                    anyhow::bail!("Migration {} failed: {}", migration_version, e);
                }
            }
        }
    }

    Ok(())
}

/// Execute a single migration's SQL
/// Note: PRAGMA user_version is handled separately in run_migrations()
fn execute_migration(conn: &Connection, sql: &str) -> Result<()> {
    // Execute all statements in the migration as a batch
    // PRAGMA user_version lines in SQL files are ignored (handled separately)
    conn.execute_batch(sql)
        .map_err(|e| anyhow::anyhow!("Failed to execute migration SQL: {}", e))?;
    Ok(())
}

/// Get current user_version
fn get_user_version(conn: &Connection) -> Result<u32> {
    let mut stmt = conn.prepare("PRAGMA user_version")?;
    let version: u32 = stmt.query_row([], |row| row.get(0))?;
    Ok(version)
}

/// Set user_version
fn set_user_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(&format!("PRAGMA user_version = {}", version), [])?;
    Ok(())
}

/// Verify all 7 tables exist
pub fn verify_tables(conn: &Connection) -> Result<()> {
    let expected_tables = [
        "projects",
        "memory_types",
        "memories",
        "memory_vectors",
        "memory_access_log",
        "archived_memories",
        "active_sessions",
    ];

    for table in &expected_tables {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            params![table],
            |row| row.get(0),
        )?;

        if count == 0 {
            anyhow::bail!("Table '{}' does not exist", table);
        }
        log::info!("Table '{}' exists", table);
    }

    Ok(())
}

/// Verify FTS5 virtual table exists
pub fn verify_fts5(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='fts_memories'",
        params![],
        |row| row.get(0),
    )?;

    if count == 0 {
        anyhow::bail!("FTS5 table 'fts_memories' does not exist");
    }
    log::info!("FTS5 table 'fts_memories' exists");
    Ok(())
}

/// Verify sqlite-vec virtual table exists
pub fn verify_vec_table(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='vec_memory_embeddings'",
        params![],
        |row| row.get(0),
    )?;

    if count == 0 {
        anyhow::bail!("sqlite-vec table 'vec_memory_embeddings' does not exist");
    }
    log::info!("sqlite-vec table 'vec_memory_embeddings' exists");
    Ok(())
}

/// Verify 13 memory types are seeded
pub fn verify_memory_types(conn: &Connection) -> Result<u32> {
    let count: u32 = conn.query_row("SELECT COUNT(*) FROM memory_types", [], |row| row.get(0))?;

    if count != 13 {
        anyhow::bail!("Expected 13 memory types, found {}", count);
    }
    log::info!("13 memory types seeded correctly");
    Ok(count)
}

/// Verify FTS5 triggers exist
pub fn verify_fts5_triggers(conn: &Connection) -> Result<()> {
    let expected_triggers = ["memories_ai", "memories_ad", "memories_au"];

    for trigger_name in &expected_triggers {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
            params![trigger_name],
            |row| row.get(0),
        )?;

        if count == 0 {
            anyhow::bail!("Trigger '{}' does not exist", trigger_name);
        }
        log::info!("Trigger '{}' exists", trigger_name);
    }

    Ok(())
}

/// Get data directory path
pub fn get_data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("Could not determine data directory")?;
    Ok(base.join("Remembrall"))
}

/// Get database path
pub fn get_database_path() -> Result<PathBuf> {
    Ok(get_data_dir()?.join("brain.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (TempDir, Connection) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = initialize_database(&db_path).unwrap();
        (temp_dir, conn)
    }

    #[test]
    fn test_all_tables_created() {
        let (_dir, conn) = create_test_db();
        verify_tables(&conn).unwrap();
    }

    #[test]
    fn test_fts5_created() {
        let (_dir, conn) = create_test_db();
        verify_fts5(&conn).unwrap();
        verify_fts5_triggers(&conn).unwrap();
    }

    #[test]
    fn test_vec_table_created() {
        let (_dir, conn) = create_test_db();
        verify_vec_table(&conn).unwrap();
    }

    #[test]
    fn test_memory_types_seeded() {
        let (_dir, conn) = create_test_db();
        let count = verify_memory_types(&conn).unwrap();
        assert_eq!(count, 13);
    }

    #[test]
    fn test_idempotent_migration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Initialize once
        let conn1 = initialize_database(&db_path).unwrap();
        let version1 = get_user_version(&conn1).unwrap();

        // Initialize again (should be idempotent)
        let conn2 = initialize_database(&db_path).unwrap();
        let version2 = get_user_version(&conn2).unwrap();

        assert_eq!(version1, version2);
        assert_eq!(version1, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let (_dir, conn) = create_test_db();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_memory_types_content() {
        let (_dir, conn) = create_test_db();

        // Check durable family
        let durable_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE family='durable'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(durable_count, 4);

        // Check operational family
        let operational_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE family='operational'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(operational_count, 6);

        // Check ephemeral family
        let ephemeral_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE family='ephemeral'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ephemeral_count, 3);

        // Check decay bands match families
        let slow_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE decay_band='slow'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(slow_count, 4);

        let mid_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE decay_band='mid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mid_count, 6);

        let fast_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_types WHERE decay_band='fast'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fast_count, 3);
    }

    #[test]
    fn test_fts5_search_works() {
        let (_dir, conn) = create_test_db();

        // Insert a test memory
        conn.execute(
            "INSERT INTO memories (project_id, type_id, source_tool, summary_text, keywords, scope)
             VALUES (NULL, 1, 'droid', 'Uses Tailwind CSS for styling', 'tailwind,css,styling', 'project')",
            [],
        ).unwrap();

        // Search using FTS5
        let results: Vec<i64> = conn.prepare(
            "SELECT m.id FROM memories m JOIN fts_memories f ON m.id = f.rowid WHERE fts_memories MATCH 'tailwind'"
        ).unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(results.len(), 1);
    }
}
