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
    // TODO: Wire up to db::queries::get_backfill_progress
    Ok(BackfillProgress {
        indexed: 0,
        total: 0,
        status: "idle".to_string(),
    })
}

/// Start the backfill engine (newest-first, background).
#[tauri::command]
pub async fn start_backfill() -> Result<(), String> {
    log::info!("start_backfill called");
    // TODO: Wire up to pipeline::backfill::start
    Ok(())
}

/// Pause the backfill engine.
#[tauri::command]
pub async fn pause_backfill() -> Result<(), String> {
    log::info!("pause_backfill called");
    // TODO: Wire up to pipeline::backfill::pause
    Ok(())
}

/// Resume the backfill engine.
#[tauri::command]
pub async fn resume_backfill() -> Result<(), String> {
    log::info!("resume_backfill called");
    // TODO: Wire up to pipeline::backfill::resume
    Ok(())
}
