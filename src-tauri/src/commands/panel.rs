use super::search::MemoryRow;

/// Get the last N recent memories for the menubar panel.
#[tauri::command]
pub async fn get_recent_memories(limit: Option<usize>) -> Result<Vec<MemoryRow>, String> {
    log::info!("get_recent_memories called with limit: {:?}", limit);
    // TODO: Wire up to db::queries::get_recent_memories
    Ok(vec![])
}

/// Get per-tool status for the panel status row.
#[tauri::command]
pub async fn get_tool_status() -> Result<serde_json::Value, String> {
    log::info!("get_tool_status called");
    // TODO: Return { droid: "green"|"grey", codex: "grey", claude: "grey", cursor: "grey" }
    Ok(serde_json::json!({
        "droid": "green",
        "codex": "coming_soon",
        "claude": "coming_soon",
        "cursor": "coming_soon"
    }))
}
