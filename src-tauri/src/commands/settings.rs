
use crate::db;
use crate::db::schema::get_data_dir;

/// Get all app settings as a key-value map.
#[tauri::command]
pub async fn get_settings() -> Result<serde_json::Value, String> {
    log::info!("get_settings called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let settings = db::queries::get_app_settings(&conn).map_err(|e| e.to_string())?;
    let map: serde_json::Map<String, serde_json::Value> = settings
        .into_iter()
        .map(|s| (s.key, serde_json::Value::String(s.value)))
        .collect();
    Ok(serde_json::Value::Object(map))
}

/// Update a single setting (upsert).
#[tauri::command]
pub async fn update_setting(key: String, value: String) -> Result<(), String> {
    log::info!("update_setting called: {} = {}", key, value);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::update_app_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

/// Clear all memories (requires confirmation — sets is_active=0 for all).
#[tauri::command]
pub async fn clear_all_memories() -> Result<(), String> {
    log::info!("clear_all_memories called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::clear_all_memories(&conn).map_err(|e| e.to_string())?;
    Ok(())
}

/// Export all memories as JSON string.
#[tauri::command]
pub async fn export_memories_json() -> Result<String, String> {
    log::info!("export_memories_json called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::export_memories_json(&conn).map_err(|e| e.to_string())
}

/// Read the tail of the redaction log.
#[tauri::command]
pub async fn get_redaction_log() -> Result<String, String> {
    log::info!("get_redaction_log called");
    let data_dir = get_data_dir().map_err(|e| e.to_string())?;
    let log_path = data_dir.join("redaction.log");
    if log_path.exists() {
        std::fs::read_to_string(&log_path).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}
