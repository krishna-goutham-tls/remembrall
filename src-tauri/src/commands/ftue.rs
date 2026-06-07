use std::path::PathBuf;
use tauri::AppHandle;

/// Check if Full Disk Access permission is granted.
#[tauri::command]
pub async fn check_fda_permission() -> Result<bool, String> {
    log::info!("check_fda_permission called");
    // Poll access() on ~/.factory/ — return true if readable
    let sessions_path = dirs::home_dir()
        .map(|h| h.join(".factory"))
        .unwrap_or_else(|| PathBuf::from("/Users/kg/.factory"));
    Ok(std::fs::read_dir(&sessions_path).is_ok())
}

/// Download a model (classifier or embedder) — emits progress events.
#[tauri::command]
pub async fn download_model(_app: AppHandle, model: String) -> Result<(), String> {
    log::info!("download_model called: {}", model);
    // TODO: Wire up to Python sidecar download / model conversion
    // Emit events: model-download-progress with { model, bytes, total }
    Ok(())
}

/// Register Remembrall MCP server in ~/.factory/mcp.json.
#[tauri::command]
pub async fn register_mcp() -> Result<serde_json::Value, String> {
    log::info!("register_mcp called");
    // TODO: Write { "remembrall": { "command": "...", "args": [] } } to ~/.factory/mcp.json
    // TODO: Append trigger line to ~/.factory/AGENTS.md
    Ok(serde_json::json!({ "success": true }))
}

/// Check if MCP is already registered.
#[tauri::command]
pub async fn check_mcp_registered() -> Result<bool, String> {
    log::info!("check_mcp_registered called");
    // TODO: Read ~/.factory/mcp.json and check for "remembrall" entry
    Ok(false)
}
