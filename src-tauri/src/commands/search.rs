use crate::db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: i64,
    pub project_name: String,
    pub type_name: String,
    pub family: String,
    pub summary_text: String,
    pub keywords: String,
    pub scope: String,
    pub importance: f64,
    pub strength: f64,
    pub recall_count: i64,
    pub source_tool: String,
    pub created_at: String,
    pub last_accessed: String,
}

impl From<db::queries::MemoryRow> for MemoryRow {
    fn from(row: db::queries::MemoryRow) -> Self {
        MemoryRow {
            id: row.id,
            project_name: row.project_name.unwrap_or_default(),
            type_name: row.type_name,
            family: format!("{:?}", row.family).to_lowercase(),
            summary_text: row.summary_text,
            keywords: row.keywords.unwrap_or_default(),
            scope: row.scope,
            importance: row.importance,
            strength: row.strength,
            recall_count: row.recall_count,
            source_tool: row.source_tool,
            created_at: row.created_at,
            last_accessed: row.last_accessed.unwrap_or_default(),
        }
    }
}

/// Full-text search using FTS5 prefix matching.
#[tauri::command]
pub async fn search_fts5(query: String) -> Result<Vec<MemoryRow>, String> {
    log::info!("search_fts5 called with query: {}", query);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let results = db::queries::search_fts5(&conn, &query, 50).map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(MemoryRow::from).collect())
}

/// Semantic search using sqlite-vec cosine distance.
/// Requires embedding model — returns error until models are available.
#[tauri::command]
pub async fn search_semantic(query: String) -> Result<Vec<MemoryRow>, String> {
    log::info!("search_semantic called with query: {}", query);
    Err("Semantic search requires embedding model (not yet available)".to_string())
}
