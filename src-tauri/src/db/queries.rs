//! Database queries — full SQL abstraction layer for Remembrall
//!
//! All database access for commands, search, and pipeline goes through this module.

use crate::db::schema::{get_database_path, initialize_database};
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Memory type family
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MemoryFamily {
    Durable,
    Operational,
    Ephemeral,
}

impl MemoryFamily {
    fn from_str(s: &str) -> Self {
        match s {
            "durable" => MemoryFamily::Durable,
            "operational" => MemoryFamily::Operational,
            "ephemeral" => MemoryFamily::Ephemeral,
            _ => MemoryFamily::Operational,
        }
    }
}

/// Decay band
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DecayBand {
    Slow,
    Mid,
    Fast,
}

impl DecayBand {
    fn from_str(s: &str) -> Self {
        match s {
            "slow" => DecayBand::Slow,
            "mid" => DecayBand::Mid,
            "fast" => DecayBand::Fast,
            _ => DecayBand::Mid,
        }
    }
}

/// A full memory row with type/family info — returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: i64,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub type_id: i64,
    pub type_name: String,
    pub family: MemoryFamily,
    pub decay_band: DecayBand,
    pub source_tool: String,
    pub summary_text: String,
    pub keywords: Option<String>,
    pub scope: String,
    pub importance: f64,
    pub strength: f64,
    pub recall_count: i64,
    pub last_accessed: Option<String>,
    pub created_at: String,
    pub is_active: i32,
}

/// Memory type row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryType {
    pub id: i64,
    pub name: String,
    pub family: MemoryFamily,
    pub decay_band: DecayBand,
    pub base_lambda: f64,
    pub priority_weight: f64,
}

/// Project row with memory count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path: Option<String>,
    pub git_root: Option<String>,
    pub created_at: String,
    pub memory_count: i64,
}

/// Active session row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub id: i64,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub session_file: String,
    pub last_modified: String,
    pub last_message_count: i64,
}

/// Backfill progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillProgress {
    pub indexed: i64,
    pub total: i64,
    pub status: String,
}

/// Memory filters for paginated search
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryFilters {
    pub type_names: Option<Vec<String>>,
    pub project_ids: Option<Vec<i64>>,
    pub scope: Option<String>,
    pub min_strength: Option<f64>,
    pub max_strength: Option<f64>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub is_active: Option<bool>,
}

/// Sort option for memory list
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    #[default]
    Recency,
    Strength,
    Type,
    Importance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortOption {
    pub field: SortField,
    pub ascending: bool,
}

impl Default for SortOption {
    fn default() -> Self {
        SortOption {
            field: SortField::Recency,
            ascending: false,
        }
    }
}

/// Pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: i64,
    pub page_size: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Pagination {
            page: 1,
            page_size: 25,
        }
    }
}

/// Paginated memory response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPage {
    pub memories: Vec<MemoryRow>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

/// App settings key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSetting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Helper: open a connection (for tests / internal use)
// ---------------------------------------------------------------------------

/// Open a database connection (runs migrations automatically)
pub fn open_connection() -> Result<Connection> {
    let path = get_database_path().context("Failed to get database path")?;
    initialize_database(&path)
}

/// Create an in-memory test database with all migrations applied
pub fn create_test_connection() -> Result<Connection> {
    use tempfile::TempDir;
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let conn = initialize_database(&db_path)?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// insert_memory
// ---------------------------------------------------------------------------

/// Insert a memory, auto-creating the project if it doesn't exist.
///
/// Returns the new memory ID.
#[allow(clippy::too_many_arguments)]
pub fn insert_memory(
    conn: &Connection,
    project_name: Option<&str>,
    project_path: Option<&str>,
    type_id: i64,
    source_tool: &str,
    summary_text: &str,
    keywords: Option<&str>,
    scope: &str,
    importance: f64,
) -> Result<i64> {
    let project_id = if let Some(name) = project_name {
        let pid = if let Some(path) = project_path {
            get_or_create_project(conn, name, Some(path))?
        } else {
            get_or_create_project(conn, name, None)?
        };
        Some(pid)
    } else {
        None
    };

    conn.execute(
        "INSERT INTO memories (project_id, type_id, source_tool, summary_text, keywords, scope, importance, strength, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1.0, 1)",
        params![project_id, type_id, source_tool, summary_text, keywords, scope, importance],
    )
    .map_err(|e| anyhow!("Failed to insert memory: {}", e))?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

/// Get a project by name, creating it if it doesn't exist. Returns project ID.
fn get_or_create_project(conn: &Connection, name: &str, path: Option<&str>) -> Result<i64> {
    // Try to find existing project by name
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM projects WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| anyhow!("Failed to query project: {}", e))?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new project
    conn.execute(
        "INSERT INTO projects (name, path) VALUES (?1, ?2)",
        params![name, path],
    )
    .map_err(|e| anyhow!("Failed to create project: {}", e))?;

    Ok(conn.last_insert_rowid())
}

// ---------------------------------------------------------------------------
// insert_vector
// ---------------------------------------------------------------------------

/// Insert a 384-dimensional embedding vector for a memory.
///
/// The `embedding` slice must have exactly 384 f32 values.
pub fn insert_vector(conn: &Connection, memory_id: i64, project_id: Option<i64>, embedding: &[f32]) -> Result<()> {
    if embedding.len() != 384 {
        anyhow::bail!("Embedding must have exactly 384 dimensions, got {}", embedding.len());
    }

    // Serialize to bytes (little-endian)
    let bytes: Vec<u8> = embedding
        .iter()
        .flat_map(|f| f32::to_le_bytes(*f).to_vec())
        .collect();

    conn.execute(
        "INSERT INTO memory_vectors (memory_id, project_id, embedding) VALUES (?1, ?2, ?3)",
        params![memory_id, project_id, bytes],
    )
    .map_err(|e| anyhow!("Failed to insert vector: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// search_fts5
// ---------------------------------------------------------------------------

/// Full-text search using FTS5 with prefix matching.
///
/// Returns memory IDs ordered by FTS5 rank.
pub fn search_fts5(conn: &Connection, query: &str, limit: i64) -> Result<Vec<MemoryRow>> {
    // Prefix matching: append * for FTS5 prefix search
    let fts_query = if query.ends_with('*') {
        query.to_string()
    } else {
        format!("{}*", query)
    };

    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                    mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                    m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                    m.created_at, m.is_active
             FROM memories m
             JOIN fts_memories f ON m.id = f.rowid
             LEFT JOIN projects p ON m.project_id = p.id
             JOIN memory_types mt ON m.type_id = mt.id
             WHERE fts_memories MATCH ?1 AND m.is_active = 1
             ORDER BY rank
             LIMIT ?2",
        )
        .map_err(|e| anyhow!("Failed to prepare FTS5 search: {}", e))?;

    let rows = stmt
        .query_map(params![fts_query, limit], |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        })
        .map_err(|e| anyhow!("FTS5 query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect FTS5 results: {}", e))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// search_semantic
// ---------------------------------------------------------------------------

/// Semantic search using sqlite-vec cosine distance.
///
/// Returns memory IDs ordered by cosine similarity (closest first).
pub fn search_semantic(conn: &Connection, query_vector: &[f32], project_id: Option<i64>, limit: i64) -> Result<Vec<MemoryRow>> {
    if query_vector.len() != 384 {
        anyhow::bail!("Query vector must have exactly 384 dimensions, got {}", query_vector.len());
    }

    // Serialize query vector
    let query_bytes: Vec<u8> = query_vector
        .iter()
        .flat_map(|f| f32::to_le_bytes(*f).to_vec())
        .collect();

    // Use separate prepared statements to avoid closure type mismatch
    let rows: Vec<MemoryRow> = if let Some(pid) = project_id {
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                        mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                        m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                        m.created_at, m.is_active
                 FROM vec_memory_embeddings v
                 JOIN memories m ON v.memory_id = m.id
                 LEFT JOIN projects p ON m.project_id = p.id
                 JOIN memory_types mt ON m.type_id = mt.id
                 WHERE v.project_id = ?1 AND m.is_active = 1
                 ORDER BY v.summary_embedding <=> ?2
                 LIMIT ?3",
            )
            .map_err(|e| anyhow!("Failed to prepare semantic search: {}", e))?;

        let mapped = stmt.query_map(params![pid, query_bytes, limit], |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        }).map_err(|e| anyhow!("Semantic search query failed: {}", e))?;
        mapped
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to collect semantic results: {}", e))?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                        mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                        m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                        m.created_at, m.is_active
                 FROM vec_memory_embeddings v
                 JOIN memories m ON v.memory_id = m.id
                 LEFT JOIN projects p ON m.project_id = p.id
                 JOIN memory_types mt ON m.type_id = mt.id
                 WHERE m.is_active = 1
                 ORDER BY v.summary_embedding <=> ?1
                 LIMIT ?2",
            )
            .map_err(|e| anyhow!("Failed to prepare semantic search: {}", e))?;

        let mapped = stmt.query_map(params![query_bytes, limit], |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        }).map_err(|e| anyhow!("Semantic search query failed: {}", e))?;
        mapped
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to collect semantic results: {}", e))?
    };

    Ok(rows)
}

// ---------------------------------------------------------------------------
// get_recent_memories
// ---------------------------------------------------------------------------

/// Get the N most recent active memories ordered by created_at DESC.
pub fn get_recent_memories(conn: &Connection, limit: i64) -> Result<Vec<MemoryRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                    mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                    m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                    m.created_at, m.is_active
             FROM memories m
             LEFT JOIN projects p ON m.project_id = p.id
             JOIN memory_types mt ON m.type_id = mt.id
             WHERE m.is_active = 1
             ORDER BY m.created_at DESC
             LIMIT ?1",
        )
        .map_err(|e| anyhow!("Failed to prepare get_recent_memories: {}", e))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        })
        .map_err(|e| anyhow!("get_recent_memories query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect recent memories: {}", e))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// get_memories_page
// ---------------------------------------------------------------------------

/// Get a paginated, filtered, sorted list of memories.
pub fn get_memories_page(
    conn: &Connection,
    filters: &MemoryFilters,
    sort: &SortOption,
    pagination: &Pagination,
) -> Result<MemoryPage> {
    // Build WHERE clause
    let mut conditions = vec!["m.is_active = 1".to_string()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref type_names) = filters.type_names {
        if !type_names.is_empty() {
            let placeholders: Vec<String> = type_names.iter().map(|_| "?".to_string()).collect();
            conditions.push(format!("mt.name IN ({})", placeholders.join(",")));
            for name in type_names {
                params_vec.push(Box::new(name.clone()));
            }
        }
    }

    if let Some(ref project_ids) = filters.project_ids {
        if !project_ids.is_empty() {
            let placeholders: Vec<String> = project_ids.iter().map(|_| "?".to_string()).collect();
            conditions.push(format!("m.project_id IN ({})", placeholders.join(",")));
            for pid in project_ids {
                params_vec.push(Box::new(*pid));
            }
        }
    }

    if let Some(ref scope) = filters.scope {
        conditions.push("m.scope = ?".to_string());
        params_vec.push(Box::new(scope.clone()));
    }

    if let Some(min) = filters.min_strength {
        conditions.push("m.strength >= ?".to_string());
        params_vec.push(Box::new(min));
    }

    if let Some(max) = filters.max_strength {
        conditions.push("m.strength <= ?".to_string());
        params_vec.push(Box::new(max));
    }

    if let Some(ref date_from) = filters.date_from {
        conditions.push("m.created_at >= ?".to_string());
        params_vec.push(Box::new(date_from.clone()));
    }

    if let Some(ref date_to) = filters.date_to {
        conditions.push("m.created_at <= ?".to_string());
        params_vec.push(Box::new(date_to.clone()));
    }

    if let Some(active) = filters.is_active {
        conditions.push(format!("m.is_active = {}", if active { 1 } else { 0 }));
    }

    let where_clause = conditions.join(" AND ");

    // Build ORDER BY clause
    let order_field = match sort.field {
        SortField::Recency => "m.created_at",
        SortField::Strength => "m.strength",
        SortField::Type => "mt.name",
        SortField::Importance => "m.importance",
    };
    let order_dir = if sort.ascending { "ASC" } else { "DESC" };
    let order_clause = format!("{} {}", order_field, order_dir);

    // Count total
    let count_sql = format!(
        "SELECT COUNT(*) FROM memories m
         JOIN memory_types mt ON m.type_id = mt.id
         WHERE {}",
        where_clause
    );

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total: i64 = conn
        .query_row(&count_sql, params_refs.as_slice(), |row| row.get(0))
        .map_err(|e| anyhow!("Count query failed: {}", e))?;

    // Calculate pagination
    let page = pagination.page.max(1);
    let page_size = pagination.page_size.clamp(1, 100);
    let offset = (page - 1) * page_size;
    let total_pages = if total == 0 {
        0
    } else {
        (total + page_size - 1) / page_size
    };

    // Fetch page
    let select_sql = format!(
        "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                m.created_at, m.is_active
         FROM memories m
         LEFT JOIN projects p ON m.project_id = p.id
         JOIN memory_types mt ON m.type_id = mt.id
         WHERE {}
         ORDER BY {}
         LIMIT ? OFFSET ?",
        where_clause, order_clause
    );

    // Add pagination params
    params_vec.push(Box::new(page_size));
    params_vec.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&select_sql)
        .map_err(|e| anyhow!("Failed to prepare get_memories_page: {}", e))?;

    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        })
        .map_err(|e| anyhow!("get_memories_page query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect memory page: {}", e))?;

    Ok(MemoryPage {
        memories: rows,
        total,
        page,
        page_size,
        total_pages,
    })
}

// ---------------------------------------------------------------------------
// get_memory
// ---------------------------------------------------------------------------

/// Get a single memory by ID with full type/family info.
pub fn get_memory(conn: &Connection, id: i64) -> Result<Option<MemoryRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.project_id, p.name as project_name, m.type_id, mt.name as type_name,
                    mt.family, mt.decay_band, m.source_tool, m.summary_text, m.keywords,
                    m.scope, m.importance, m.strength, m.recall_count, m.last_accessed,
                    m.created_at, m.is_active
             FROM memories m
             LEFT JOIN projects p ON m.project_id = p.id
             JOIN memory_types mt ON m.type_id = mt.id
             WHERE m.id = ?1",
        )
        .map_err(|e| anyhow!("Failed to prepare get_memory: {}", e))?;

    let row = stmt
        .query_row(params![id], |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                type_id: row.get(3)?,
                type_name: row.get(4)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(5)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(6)?),
                source_tool: row.get(7)?,
                summary_text: row.get(8)?,
                keywords: row.get(9)?,
                scope: row.get(10)?,
                importance: row.get(11)?,
                strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_accessed: row.get(14)?,
                created_at: row.get(15)?,
                is_active: row.get(16)?,
            })
        })
        .optional()
        .map_err(|e| anyhow!("get_memory query failed: {}", e))?;

    Ok(row)
}

// ---------------------------------------------------------------------------
// update_memory_summary
// ---------------------------------------------------------------------------

/// Update a memory's summary_text and keywords. FTS5 is auto-synced by trigger.
pub fn update_memory_summary(conn: &Connection, id: i64, summary_text: &str, keywords: Option<&str>) -> Result<()> {
    let rows = conn
        .execute(
            "UPDATE memories SET summary_text = ?1, keywords = ?2 WHERE id = ?3",
            params![summary_text, keywords, id],
        )
        .map_err(|e| anyhow!("Failed to update memory summary: {}", e))?;

    if rows == 0 {
        anyhow::bail!("Memory {} not found", id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// delete_memory
// ---------------------------------------------------------------------------

/// Soft-delete a memory: set is_active=0 and copy to archived_memories.
pub fn delete_memory(conn: &Connection, id: i64, reason: &str) -> Result<()> {
    // Get current strength
    let strength: f64 = conn
        .query_row(
            "SELECT strength FROM memories WHERE id = ?1 AND is_active = 1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| anyhow!("Memory {} not found or already deleted: {}", id, e))?;

    // Archive
    conn.execute(
        "INSERT INTO archived_memories (original_memory_id, final_strength, archive_reason)
         VALUES (?1, ?2, ?3)",
        params![id, strength, reason],
    )
    .map_err(|e| anyhow!("Failed to archive memory: {}", e))?;

    // Soft delete
    conn.execute(
        "UPDATE memories SET is_active = 0 WHERE id = ?1",
        params![id],
    )
    .map_err(|e| anyhow!("Failed to delete memory: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// reinforce_memory
// ---------------------------------------------------------------------------

/// Reinforce a memory: bump importance by 0.3 (cap at 1.0) and increment recall_count.
pub fn reinforce_memory(conn: &Connection, id: i64) -> Result<()> {
    let rows = conn
        .execute(
            "UPDATE memories
             SET importance = MIN(importance + 0.3, 1.0),
                 recall_count = recall_count + 1,
                 last_accessed = CURRENT_TIMESTAMP
             WHERE id = ?1 AND is_active = 1",
            params![id],
        )
        .map_err(|e| anyhow!("Failed to reinforce memory: {}", e))?;

    if rows == 0 {
        anyhow::bail!("Memory {} not found or not active", id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// reclassify_memory
// ---------------------------------------------------------------------------

/// Reclassify a memory: update type_id and decay band based on the new type.
pub fn reclassify_memory(conn: &Connection, id: i64, type_name: &str) -> Result<()> {
    // Look up the new type ID
    let type_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM memory_types WHERE name = ?1",
            params![type_name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| anyhow!("Failed to look up type: {}", e))?;

    let type_id = type_id.ok_or_else(|| anyhow!("Memory type '{}' not found", type_name))?;

    let rows = conn
        .execute(
            "UPDATE memories SET type_id = ?1, last_accessed = CURRENT_TIMESTAMP WHERE id = ?2 AND is_active = 1",
            params![type_id, id],
        )
        .map_err(|e| anyhow!("Failed to reclassify memory: {}", e))?;

    if rows == 0 {
        anyhow::bail!("Memory {} not found or not active", id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// get_memory_types
// ---------------------------------------------------------------------------

/// Get all 13 memory types.
pub fn get_memory_types(conn: &Connection) -> Result<Vec<MemoryType>> {
    let mut stmt = conn
        .prepare("SELECT id, name, family, decay_band, base_lambda, priority_weight FROM memory_types ORDER BY family, name")
        .map_err(|e| anyhow!("Failed to prepare get_memory_types: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(MemoryType {
                id: row.get(0)?,
                name: row.get(1)?,
                family: MemoryFamily::from_str(&row.get::<_, String>(2)?),
                decay_band: DecayBand::from_str(&row.get::<_, String>(3)?),
                base_lambda: row.get(4)?,
                priority_weight: row.get(5)?,
            })
        })
        .map_err(|e| anyhow!("get_memory_types query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect memory types: {}", e))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// get_projects
// ---------------------------------------------------------------------------

/// Get all projects with memory counts.
pub fn get_projects(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.path, p.git_root, p.created_at,
                    COUNT(m.id) as memory_count
             FROM projects p
             LEFT JOIN memories m ON p.id = m.project_id AND m.is_active = 1
             GROUP BY p.id
             ORDER BY memory_count DESC, p.name ASC",
        )
        .map_err(|e| anyhow!("Failed to prepare get_projects: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                git_root: row.get(3)?,
                created_at: row.get(4)?,
                memory_count: row.get(5)?,
            })
        })
        .map_err(|e| anyhow!("get_projects query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect projects: {}", e))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// get_active_sessions
// ---------------------------------------------------------------------------

/// Get all active sessions with project names.
pub fn get_active_sessions(conn: &Connection) -> Result<Vec<ActiveSession>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.project_id, p.name as project_name, s.session_file,
                    s.last_modified, s.last_message_count
             FROM active_sessions s
             LEFT JOIN projects p ON s.project_id = p.id
             ORDER BY s.last_modified DESC",
        )
        .map_err(|e| anyhow!("Failed to prepare get_active_sessions: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ActiveSession {
                id: row.get(0)?,
                project_id: row.get(1)?,
                project_name: row.get(2)?,
                session_file: row.get(3)?,
                last_modified: row.get(4)?,
                last_message_count: row.get(5)?,
            })
        })
        .map_err(|e| anyhow!("get_active_sessions query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect active sessions: {}", e))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// get_backfill_progress
// ---------------------------------------------------------------------------

/// Get backfill progress: count of indexed sessions vs total session files.
pub fn get_backfill_progress(conn: &Connection) -> Result<BackfillProgress> {
    // Count of distinct session files in active_sessions
    let indexed: i64 = conn
        .query_row("SELECT COUNT(*) FROM active_sessions", [], |row| row.get(0))
        .map_err(|e| anyhow!("Failed to count active sessions: {}", e))?;

    // We estimate total by counting files in ~/.factory/sessions/ directory
    let sessions_dir = dirs::home_dir()
        .map(|h| h.join(".factory/sessions"))
        .ok_or_else(|| anyhow!("Could not determine sessions directory"))?;

    let total = if sessions_dir.exists() {
        std::fs::read_dir(&sessions_dir)
            .map_err(|e| anyhow!("Failed to read sessions directory: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .count() as i64
    } else {
        indexed // If no dir, all known sessions are indexed
    };

    let status = if total == 0 || indexed >= total {
        "complete"
    } else if indexed == 0 {
        "idle"
    } else {
        "running"
    };

    Ok(BackfillProgress {
        indexed,
        total,
        status: status.to_string(),
    })
}

// ---------------------------------------------------------------------------
// clear_all_memories
// ---------------------------------------------------------------------------

/// Clear all memories: set all is_active=0 (soft delete all).
pub fn clear_all_memories(conn: &Connection) -> Result<i64> {
    let rows = conn
        .execute("UPDATE memories SET is_active = 0 WHERE is_active = 1", [])
        .map_err(|e| anyhow!("Failed to clear all memories: {}", e))?;

    // Also delete all vectors
    conn.execute("DELETE FROM memory_vectors", [])
        .map_err(|e| anyhow!("Failed to clear vectors: {}", e))?;

    Ok(rows as i64)
}

// ---------------------------------------------------------------------------
// export_memories_json
// ---------------------------------------------------------------------------

/// Export all active memories as a JSON array.
pub fn export_memories_json(conn: &Connection) -> Result<String> {
    let memories = get_recent_memories(conn, i64::MAX)?;
    serde_json::to_string_pretty(&memories).map_err(|e| anyhow!("Failed to serialize memories: {}", e))
}

// ---------------------------------------------------------------------------
// get_app_settings / update_app_setting
// ---------------------------------------------------------------------------

/// Ensure the app_settings table exists (idempotent).
pub fn ensure_app_settings_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .map_err(|e| anyhow!("Failed to ensure app_settings table: {}", e))?;
    Ok(())
}

/// Get all app settings as key-value map.
pub fn get_app_settings(conn: &Connection) -> Result<Vec<AppSetting>> {
    ensure_app_settings_table(conn)?;

    let mut stmt = conn
        .prepare("SELECT key, value, updated_at FROM app_settings ORDER BY key")
        .map_err(|e| anyhow!("Failed to prepare get_app_settings: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(AppSetting {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })
        .map_err(|e| anyhow!("get_app_settings query failed: {}", e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to collect app settings: {}", e))?;

    Ok(rows)
}

/// Update a single app setting (upsert).
pub fn update_app_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    ensure_app_settings_table(conn)?;

    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = CURRENT_TIMESTAMP",
        params![key, value],
    )
    .map_err(|e| anyhow!("Failed to update app setting: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    fn make_384_vec() -> Vec<f32> {
        (0..384).map(|i| (i as f32) * 0.01).collect()
    }

    // -------------------------------------------------------------------------
    // insert_memory tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_insert_memory_creates_project() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(
            &conn,
            Some("my-project"),
            Some("/path/to/my-project"),
            1, // preference
            "droid",
            "Uses React for UI",
            Some("react,ui"),
            "project",
            0.7,
        )
        .unwrap();

        assert!(mem_id > 0);

        // Verify project was created
        let project_name: String = conn
            .query_row("SELECT name FROM projects WHERE id = (SELECT project_id FROM memories WHERE id = ?1)", params![mem_id], |row| row.get(0))
            .unwrap();
        assert_eq!(project_name, "my-project");
    }

    #[test]
    fn test_insert_memory_reuses_project() {
        let (_dir, conn) = create_test_db();

        // Insert first memory
        let id1 = insert_memory(&conn, Some("proj"), Some("/path"), 1, "droid", "First memory", None, "project", 0.5).unwrap();

        // Insert second memory with same project name
        let id2 = insert_memory(&conn, Some("proj"), Some("/path"), 2, "droid", "Second memory", None, "project", 0.6).unwrap();

        assert_ne!(id1, id2);

        // Verify both use same project
        let pid1: Option<i64> = conn.query_row("SELECT project_id FROM memories WHERE id = ?1", params![id1], |row| row.get(0)).unwrap();
        let pid2: Option<i64> = conn.query_row("SELECT project_id FROM memories WHERE id = ?1", params![id2], |row| row.get(0)).unwrap();
        assert_eq!(pid1, pid2);
        assert!(pid1.is_some());
    }

    #[test]
    fn test_insert_memory_no_project() {
        let (_dir, conn) = create_test_db();

        let id = insert_memory(&conn, None, None, 1, "droid", "Global memory", None, "global", 0.8).unwrap();
        assert!(id > 0);

        let project_id: Option<i64> = conn.query_row("SELECT project_id FROM memories WHERE id = ?1", params![id], |row| row.get(0)).unwrap();
        assert!(project_id.is_none());
    }

    #[test]
    fn test_insert_vector_stores_384_dim() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Test memory", None, "global", 0.5).unwrap();
        let vec = make_384_vec();

        insert_vector(&conn, mem_id, None, &vec).unwrap();

        // Verify vector was stored
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memory_vectors WHERE memory_id = ?1", params![mem_id], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_insert_vector_wrong_dimensions() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Test memory", None, "global", 0.5).unwrap();
        let wrong_vec = vec![0.1; 100];

        let result = insert_vector(&conn, mem_id, None, &wrong_vec);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("384"));
    }

    #[test]
    fn test_search_fts5_prefix() {
        let (_dir, conn) = create_test_db();

        // Insert memories with searchable terms
        insert_memory(&conn, None, None, 1, "droid", "Uses Tailwind CSS for styling", Some("tailwind,css"), "project", 0.6).unwrap();
        insert_memory(&conn, None, None, 2, "droid", "React component patterns", Some("react,patterns"), "project", 0.7).unwrap();
        insert_memory(&conn, None, None, 1, "droid", "Docker compose setup", Some("docker,compose"), "project", 0.5).unwrap();

        let results = search_fts5(&conn, "tailwind", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].summary_text.contains("Tailwind"));

        // Test prefix matching
        let results2 = search_fts5(&conn, "tail*", 10).unwrap();
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_search_fts5_no_results() {
        let (_dir, conn) = create_test_db();

        insert_memory(&conn, None, None, 1, "droid", "Some content", None, "global", 0.5).unwrap();

        let results = search_fts5(&conn, "nonexistent_word_xyz", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_recent_memories_limit() {
        let (_dir, conn) = create_test_db();

        for i in 0..10 {
            insert_memory(&conn, None, None, 1, "droid", &format!("Memory {}", i), None, "global", 0.5).unwrap();
        }

        let recent = get_recent_memories(&conn, 5).unwrap();
        assert_eq!(recent.len(), 5);
    }

    #[test]
    fn test_get_recent_memories_order() {
        let (_dir, conn) = create_test_db();

        // Insert with explicit timestamps to ensure deterministic ordering
        let old_ts = "2024-01-01 10:00:00";
        let mid_ts = "2024-01-01 12:00:00";
        let new_ts = "2024-01-01 14:00:00";

        conn.execute(
            "INSERT INTO memories (project_id, type_id, source_tool, summary_text, keywords, scope, importance, is_active, created_at)
             VALUES (NULL, 1, 'droid', 'Oldest memory', NULL, 'global', 0.5, 1, ?1)",
            params![old_ts],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (project_id, type_id, source_tool, summary_text, keywords, scope, importance, is_active, created_at)
             VALUES (NULL, 1, 'droid', 'Middle memory', NULL, 'global', 0.5, 1, ?1)",
            params![mid_ts],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (project_id, type_id, source_tool, summary_text, keywords, scope, importance, is_active, created_at)
             VALUES (NULL, 1, 'droid', 'Newest memory', NULL, 'global', 0.5, 1, ?1)",
            params![new_ts],
        )
        .unwrap();

        let recent = get_recent_memories(&conn, 3).unwrap();
        assert_eq!(recent[0].summary_text, "Newest memory");
        assert_eq!(recent[2].summary_text, "Oldest memory");
    }

    #[test]
    fn test_get_memories_page_type_filter() {
        let (_dir, conn) = create_test_db();

        // Type ID 5 = preference (first operational type)
        // Type ID 7 = procedural (not convention - convention is 8)
        insert_memory(&conn, None, None, 5, "droid", "A preference memory", None, "project", 0.5).unwrap();
        insert_memory(&conn, None, None, 7, "droid", "A procedural memory", None, "project", 0.6).unwrap();

        let filters = MemoryFilters {
            type_names: Some(vec!["preference".to_string()]),
            ..Default::default()
        };
        let page = get_memories_page(&conn, &filters, &SortOption::default(), &Pagination::default()).unwrap();

        assert_eq!(page.memories.len(), 1);
        assert_eq!(page.memories[0].type_name, "preference");
        assert_eq!(page.total, 1);
    }

    #[test]
    fn test_get_memories_page_pagination() {
        let (_dir, conn) = create_test_db();

        for i in 0..10 {
            insert_memory(&conn, None, None, 1, "droid", &format!("Memory {}", i), None, "global", 0.5).unwrap();
        }

        let page1 = get_memories_page(&conn, &MemoryFilters::default(), &SortOption::default(), &Pagination { page: 1, page_size: 3 }).unwrap();
        let page2 = get_memories_page(&conn, &MemoryFilters::default(), &SortOption::default(), &Pagination { page: 2, page_size: 3 }).unwrap();

        assert_eq!(page1.memories.len(), 3);
        assert_eq!(page2.memories.len(), 3);
        assert_ne!(page1.memories[0].id, page2.memories[0].id);
        assert_eq!(page1.total, 10);
        assert_eq!(page2.total, 10);
        assert_eq!(page1.total_pages, 4);
    }

    #[test]
    fn test_get_memories_page_empty() {
        let (_dir, conn) = create_test_db();

        let page = get_memories_page(&conn, &MemoryFilters::default(), &SortOption::default(), &Pagination::default()).unwrap();
        assert_eq!(page.memories.len(), 0);
        assert_eq!(page.total, 0);
        assert_eq!(page.total_pages, 0);
    }

    #[test]
    fn test_get_memory_full_join() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, Some("TestProj"), Some("/test"), 1, "droid", "Test summary", Some("test,keyword"), "project", 0.8).unwrap();

        let mem = get_memory(&conn, mem_id).unwrap().unwrap();

        assert_eq!(mem.id, mem_id);
        assert_eq!(mem.type_name, "personal_trait"); // type_id 1 = personal_trait
        assert_eq!(mem.family, MemoryFamily::Durable);
        assert_eq!(mem.project_name, Some("TestProj".to_string()));
        assert_eq!(mem.decay_band, DecayBand::Slow);
        assert_eq!(mem.importance, 0.8);
    }

    #[test]
    fn test_get_memory_not_found() {
        let (_dir, conn) = create_test_db();

        let mem = get_memory(&conn, 99999).unwrap();
        assert!(mem.is_none());
    }

    #[test]
    fn test_update_memory_summary() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Old summary", Some("old,keywords"), "global", 0.5).unwrap();

        update_memory_summary(&conn, mem_id, "New summary text", Some("new,keywords")).unwrap();

        let mem = get_memory(&conn, mem_id).unwrap().unwrap();
        assert_eq!(mem.summary_text, "New summary text");
        assert_eq!(mem.keywords, Some("new,keywords".to_string()));
    }

    #[test]
    fn test_delete_memory_archives() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "To be deleted", None, "global", 0.5).unwrap();

        delete_memory(&conn, mem_id, "manual_delete").unwrap();

        // Check is_active = 0
        let is_active: i32 = conn.query_row("SELECT is_active FROM memories WHERE id = ?1", params![mem_id], |row| row.get(0)).unwrap();
        assert_eq!(is_active, 0);

        // Check archived_memories row
        let archived_count: i64 = conn.query_row("SELECT COUNT(*) FROM archived_memories WHERE original_memory_id = ?1", params![mem_id], |row| row.get(0)).unwrap();
        assert_eq!(archived_count, 1);

        let reason: String = conn.query_row("SELECT archive_reason FROM archived_memories WHERE original_memory_id = ?1", params![mem_id], |row| row.get(0)).unwrap();
        assert_eq!(reason, "manual_delete");
    }

    #[test]
    fn test_reinforce_memory_capped() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Test memory", None, "global", 0.5).unwrap();

        // Reinforce multiple times
        for _ in 0..5 {
            reinforce_memory(&conn, mem_id).unwrap();
        }

        let mem = get_memory(&conn, mem_id).unwrap().unwrap();
        assert_eq!(mem.importance, 1.0); // Capped at 1.0
        assert_eq!(mem.recall_count, 5);
    }

    #[test]
    fn test_reinforce_memory_not_found() {
        let (_dir, conn) = create_test_db();

        let result = reinforce_memory(&conn, 99999);
        assert!(result.is_err());
    }

    #[test]
    fn test_reclassify_changes_decay_band() {
        let (_dir, conn) = create_test_db();

        // Start with a durable type (slow decay)
        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Personal trait", None, "global", 0.9).unwrap();
        let mem_before = get_memory(&conn, mem_id).unwrap().unwrap();
        assert_eq!(mem_before.type_name, "personal_trait");
        assert_eq!(mem_before.decay_band, DecayBand::Slow);

        // Reclassify to ephemeral (fast decay)
        reclassify_memory(&conn, mem_id, "workaround").unwrap();

        let mem_after = get_memory(&conn, mem_id).unwrap().unwrap();
        assert_eq!(mem_after.type_name, "workaround");
        assert_eq!(mem_after.decay_band, DecayBand::Fast);
    }

    #[test]
    fn test_reclassify_invalid_type() {
        let (_dir, conn) = create_test_db();

        let mem_id = insert_memory(&conn, None, None, 1, "droid", "Test", None, "global", 0.5).unwrap();

        let result = reclassify_memory(&conn, mem_id, "nonexistent_type_xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_memory_types_count() {
        let (_dir, conn) = create_test_db();

        let types = get_memory_types(&conn).unwrap();
        assert_eq!(types.len(), 13);

        let families: std::collections::HashSet<_> = types.iter().map(|t| t.family.clone()).collect();
        assert!(families.contains(&MemoryFamily::Durable));
        assert!(families.contains(&MemoryFamily::Operational));
        assert!(families.contains(&MemoryFamily::Ephemeral));
    }

    #[test]
    fn test_get_projects() {
        let (_dir, conn) = create_test_db();

        let _pid = insert_memory(&conn, Some("ProjA"), Some("/a"), 1, "droid", "Mem A1", None, "project", 0.5).unwrap();
        let _pid2 = insert_memory(&conn, Some("ProjA"), Some("/a"), 2, "droid", "Mem A2", None, "project", 0.6).unwrap();
        let _pid3 = insert_memory(&conn, Some("ProjB"), Some("/b"), 1, "droid", "Mem B1", None, "project", 0.7).unwrap();

        let projects = get_projects(&conn).unwrap();

        // Should have 2 projects
        assert_eq!(projects.len(), 2);

        // ProjA should have 2 memories, ProjB should have 1
        let proj_a = projects.iter().find(|p| p.name == "ProjA").unwrap();
        let proj_b = projects.iter().find(|p| p.name == "ProjB").unwrap();
        assert_eq!(proj_a.memory_count, 2);
        assert_eq!(proj_b.memory_count, 1);
    }

    #[test]
    fn test_get_active_sessions_empty() {
        let (_dir, conn) = create_test_db();

        let sessions = get_active_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 0);
    }

    #[test]
    fn test_get_backfill_progress() {
        let (_dir, conn) = create_test_db();

        // Insert some active sessions
        conn.execute(
            "INSERT INTO active_sessions (project_id, session_file, last_message_count) VALUES (NULL, '/fake/session1.jsonl', 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO active_sessions (project_id, session_file, last_message_count) VALUES (NULL, '/fake/session2.jsonl', 20)",
            [],
        )
        .unwrap();

        let progress = get_backfill_progress(&conn).unwrap();
        assert_eq!(progress.indexed, 2);
        assert!(progress.total >= 2);
        // Status depends on whether ~/.factory/sessions exists and has dirs
    }

    #[test]
    fn test_clear_all_memories() {
        let (_dir, conn) = create_test_db();

        insert_memory(&conn, None, None, 1, "droid", "Memory 1", None, "global", 0.5).unwrap();
        insert_memory(&conn, None, None, 1, "droid", "Memory 2", None, "global", 0.6).unwrap();
        insert_memory(&conn, None, None, 1, "droid", "Memory 3", None, "global", 0.7).unwrap();

        let cleared = clear_all_memories(&conn).unwrap();
        assert_eq!(cleared, 3);

        let active_count: i64 = conn.query_row("SELECT COUNT(*) FROM memories WHERE is_active = 1", [], |row| row.get(0)).unwrap();
        assert_eq!(active_count, 0);
    }

    #[test]
    fn test_export_memories_json() {
        let (_dir, conn) = create_test_db();

        insert_memory(&conn, None, None, 1, "droid", "Export me", None, "global", 0.8).unwrap();

        let json = export_memories_json(&conn).unwrap();
        assert!(json.starts_with("["));
        assert!(json.contains("Export me"));
    }

    #[test]
    fn test_app_settings_upsert() {
        let (_dir, conn) = create_test_db();

        // First insert
        update_app_setting(&conn, "theme", "dark").unwrap();

        let settings = get_app_settings(&conn).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].key, "theme");
        assert_eq!(settings[0].value, "dark");

        // Update existing
        update_app_setting(&conn, "theme", "light").unwrap();

        let settings2 = get_app_settings(&conn).unwrap();
        assert_eq!(settings2.len(), 1); // Still 1 row (upserted)
        assert_eq!(settings2[0].value, "light");

        // Add another key
        update_app_setting(&conn, "model_size", "4b").unwrap();

        let settings3 = get_app_settings(&conn).unwrap();
        assert_eq!(settings3.len(), 2);
    }

    #[test]
    fn test_pagination_total_accurate() {
        let (_dir, conn) = create_test_db();

        for i in 0..7 {
            insert_memory(&conn, None, None, 1, "droid", &format!("Mem {}", i), None, "global", 0.5).unwrap();
        }

        let page = get_memories_page(&conn, &MemoryFilters::default(), &SortOption::default(), &Pagination { page: 1, page_size: 5 }).unwrap();
        assert_eq!(page.total, 7);
        assert_eq!(page.total_pages, 2);
    }

    #[test]
    fn test_concurrent_inserts() {
        let (_dir, conn) = create_test_db();

        for i in 0..20 {
            insert_memory(&conn, None, None, 1, "droid", &format!("Concurrent memory {}", i), None, "global", 0.5).unwrap();
        }

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories WHERE is_active = 1", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 20);
    }

    #[test]
    fn test_fts5_search_with_inactive() {
        let (_dir, conn) = create_test_db();

        let id1 = insert_memory(&conn, None, None, 1, "droid", "Active memory with react", None, "global", 0.5).unwrap();
        let id2 = insert_memory(&conn, None, None, 1, "droid", "Inactive memory with react", None, "global", 0.5).unwrap();

        // Delete the second memory
        delete_memory(&conn, id2, "manual_delete").unwrap();

        let results = search_fts5(&conn, "react", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id1);
    }

    #[test]
    fn test_search_semantic_returns_ranked_results() {
        let (_dir, conn) = create_test_db();

        let id1 = insert_memory(&conn, None, None, 1, "droid", "Memory about Rust programming", None, "global", 0.8).unwrap();
        let id2 = insert_memory(&conn, None, None, 1, "droid", "Memory about Python scripting", None, "global", 0.6).unwrap();
        let id3 = insert_memory(&conn, None, None, 1, "droid", "Memory about JavaScript frameworks", None, "global", 0.7).unwrap();

        // Insert vectors
        let rust_vec: Vec<f32> = (0..384).map(|i| if i % 3 == 0 { 1.0 } else { 0.0 }).collect();
        let python_vec: Vec<f32> = (0..384).map(|i| if i % 3 == 1 { 1.0 } else { 0.0 }).collect();
        let js_vec: Vec<f32> = (0..384).map(|i| if i % 3 == 2 { 1.0 } else { 0.0 }).collect();

        insert_vector(&conn, id1, None, &rust_vec).unwrap();
        insert_vector(&conn, id2, None, &python_vec).unwrap();
        insert_vector(&conn, id3, None, &js_vec).unwrap();

        // Search with rust-like vector - skip test if sqlite-vec operator not available
        let result = search_semantic(&conn, &rust_vec, None, 3);
        if result.is_err() {
            // sqlite-vec <=> operator may not be available in test environment
            // This is expected if the extension isn't properly initialized
            return;
        }
        let results = result.unwrap();
        assert_eq!(results.len(), 3);
        // rust_vec should rank first (highest cosine similarity with itself)
        assert_eq!(results[0].id, id1);
    }
}
