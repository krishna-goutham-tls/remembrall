use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

/// Icon state stored as an atomic — id: 0, indexing: 1, recall: 2, error: 3, new_memories: 4
static ICON_STATE: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconState {
    Idle,
    Indexing,
    Recall,
    Error,
    NewMemories,
}

impl From<u8> for IconState {
    fn from(v: u8) -> Self {
        match v {
            1 => IconState::Indexing,
            2 => IconState::Recall,
            3 => IconState::Error,
            4 => IconState::NewMemories,
            _ => IconState::Idle,
        }
    }
}

impl From<IconState> for u8 {
    fn from(s: IconState) -> Self {
        match s {
            IconState::Idle => 0,
            IconState::Indexing => 1,
            IconState::Recall => 2,
            IconState::Error => 3,
            IconState::NewMemories => 4,
        }
    }
}

/// Return the current icon state.
#[tauri::command]
pub async fn get_icon_state() -> Result<IconState, String> {
    let state = ICON_STATE.load(Ordering::SeqCst);
    Ok(IconState::from(state))
}

/// Clear the "new memories" blue dot — reset to idle.
#[tauri::command]
pub async fn clear_new_memories() -> Result<(), String> {
    ICON_STATE.store(0, Ordering::SeqCst);
    log::info!("Icon state cleared to idle");
    Ok(())
}

/// Set icon state (used internally by pipeline/backfill).
pub fn set_icon_state(state: IconState) {
    ICON_STATE.store(state.into(), Ordering::SeqCst);
}
