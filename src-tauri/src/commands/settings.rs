

use super::search::MemoryRow;

/// Get all app settings as a key-value map.
#[tauri::command]
pub async fn get_settings() -> Result<serde_json::Value, String> {
    log::info!("get_settings called");
    // TODO: Wire up to db::queries::get_app_settings
    Ok(serde_json::json!({}))
}

/// Update a single setting (upsert).
#[tauri::command]
pub async fn update_setting(key: String, value: String) -> Result<(), String> {
    log::info!("update_setting called: {} = {}", key, value);
    // TODO: Wire up to db::queries::update_app_setting
    Ok(())
}

/// Clear all memories (requires confirmation — sets is_active=0 for all).
#[tauri::command]
pub async fn clear_all_memories() -> Result<(), String> {
    log::info!("clear_all_memories called");
    // TODO: Wire up to db::queries::clear_all_memories
    Ok(())
}

/// Export all memories as JSON.
#[tauri::command]
pub async fn export_memories_json() -> Result<Vec<MemoryRow>, String> {
    log::info!("export_memories_json called");
    // TODO: Wire up to db::queries::export_memories_json
    Ok(vec![])
}

/// Read the tail of the redaction log.
#[tauri::command]
pub async fn get_redaction_log() -> Result<String, String> {
    log::info!("get_redaction_log called");
    // TODO: Wire up to redaction log file reading
    Ok(String::new())
}
