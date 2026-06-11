
use crate::db;
use super::search::MemoryRow;

/// Get a single memory by ID.
#[tauri::command]
pub async fn get_memory(id: i64) -> Result<MemoryRow, String> {
    log::info!("get_memory called with id: {}", id);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let row = db::queries::get_memory(&conn, id).map_err(|e| e.to_string())?;
    row.map(MemoryRow::from).ok_or_else(|| format!("Memory {} not found", id))
}

/// Reinforce a memory (bump importance +0.3, cap 1.0, increment recall_count).
#[tauri::command]
pub async fn reinforce_memory(id: i64) -> Result<(), String> {
    log::info!("reinforce_memory called with id: {}", id);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::reinforce_memory(&conn, id).map_err(|e| e.to_string())
}

/// Soft-delete a memory (set is_active=0, archive).
#[tauri::command]
pub async fn delete_memory(id: i64) -> Result<(), String> {
    log::info!("delete_memory called with id: {}", id);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::delete_memory(&conn, id, "manual_delete").map_err(|e| e.to_string())
}

/// Reclassify a memory (update type_id + decay band).
#[tauri::command]
pub async fn reclassify_memory(id: i64, type_name: String) -> Result<(), String> {
    log::info!("reclassify_memory called with id: {}, type: {}", id, type_name);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::reclassify_memory(&conn, id, &type_name).map_err(|e| e.to_string())
}

/// Edit a memory's summary text (triggers re-embed).
#[tauri::command]
pub async fn edit_memory_summary(id: i64, summary: String) -> Result<(), String> {
    log::info!("edit_memory_summary called with id: {}, summary: {}", id, summary);
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    db::queries::update_memory_summary(&conn, id, &summary, None).map_err(|e| e.to_string())
}
