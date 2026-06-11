use crate::db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BackfillProgress {
    pub indexed: usize,
    pub total: usize,
    pub status: String,
}

/// Get current backfill progress.
#[tauri::command]
pub async fn get_backfill_progress() -> Result<BackfillProgress, String> {
    log::info!("get_backfill_progress called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let progress = db::queries::get_backfill_progress(&conn).map_err(|e| e.to_string())?;
    Ok(BackfillProgress {
        indexed: progress.indexed as usize,
        total: progress.total as usize,
        status: progress.status,
    })
}

/// Start the backfill engine (newest-first, background).
#[tauri::command]
pub async fn start_backfill() -> Result<(), String> {
    log::info!("start_backfill called");
    // TODO: Wire up to pipeline::backfill::start (Phase 2)
    Ok(())
}

/// Pause the backfill engine.
#[tauri::command]
pub async fn pause_backfill() -> Result<(), String> {
    log::info!("pause_backfill called");
    // TODO: Wire up to pipeline::backfill::pause (Phase 2)
    Ok(())
}

/// Resume the backfill engine.
#[tauri::command]
pub async fn resume_backfill() -> Result<(), String> {
    log::info!("resume_backfill called");
    // TODO: Wire up to pipeline::backfill::resume (Phase 2)
    Ok(())
}
