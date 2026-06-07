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

/// Full-text search using FTS5 prefix matching.
#[tauri::command]
pub async fn search_fts5(query: String) -> Result<Vec<MemoryRow>, String> {
    log::info!("search_fts5 called with query: {}", query);
    // TODO: Wire up to db::queries::search_fts5
    Ok(vec![])
}

/// Semantic search using sqlite-vec cosine distance.
#[tauri::command]
pub async fn search_semantic(query: String) -> Result<Vec<MemoryRow>, String> {
    log::info!("search_semantic called with query: {}", query);
    // TODO: Wire up to db::queries::search_semantic
    Ok(vec![])
}
