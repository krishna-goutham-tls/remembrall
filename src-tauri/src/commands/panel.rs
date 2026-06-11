use crate::db;
use super::search::MemoryRow;

/// Get the last N recent memories for the menubar panel.
#[tauri::command]
pub async fn get_recent_memories(limit: Option<usize>) -> Result<Vec<MemoryRow>, String> {
    log::info!("get_recent_memories called with limit: {:?}", limit);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let n = limit.unwrap_or(5) as i64;
    let results = db::queries::get_recent_memories(&conn, n).map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(MemoryRow::from).collect())
}

/// Get per-tool status for the panel status row.
#[tauri::command]
pub async fn get_tool_status() -> Result<serde_json::Value, String> {
    log::info!("get_tool_status called");
    // FSWatcher not built yet — return static status
    Ok(serde_json::json!({
        "droid": "green",
        "codex": "coming_soon",
        "claude": "coming_soon",
        "cursor": "coming_soon"
    }))
}

/// Quit the entire app (called from menubar panel Quit button).
#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle) {
    log::info!("Quit requested from panel");
    app.exit(0);
}
