use std::path::PathBuf;
use tauri::AppHandle;

/// Check if Full Disk Access permission is granted.
#[tauri::command]
pub async fn check_fda_permission() -> Result<bool, String> {
    log::info!("check_fda_permission called");
    let sessions_path = dirs::home_dir()
        .map(|h| h.join(".factory"))
        .unwrap_or_else(|| PathBuf::from("/Users/kg/.factory"));
    Ok(std::fs::read_dir(&sessions_path).is_ok())
}

/// Download a model (classifier or embedder) — emits progress events.
#[tauri::command]
pub async fn download_model(_app: AppHandle, model: String) -> Result<(), String> {
    log::info!("download_model called: {}", model);
    // TODO: Wire up to Python sidecar download / model conversion (deferred)
    // Emit events: model-download-progress with { model, bytes, total }
    Ok(())
}

/// Register Remembrall MCP server in ~/.factory/mcp.json and append trigger to AGENTS.md.
#[tauri::command]
pub async fn register_mcp() -> Result<serde_json::Value, String> {
    log::info!("register_mcp called");

    let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;

    // 1. Write/merge ~/.factory/mcp.json
    let factory_dir = home.join(".factory");
    let mcp_path = factory_dir.join("mcp.json");

    std::fs::create_dir_all(&factory_dir).map_err(|e| format!("Failed to create .factory dir: {}", e))?;

    // Read existing config or start fresh
    let mut config: serde_json::Map<String, serde_json::Value> = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path).map_err(|e| format!("Failed to read mcp.json: {}", e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::Map::new())
    } else {
        serde_json::Map::new()
    };

    // Add remembrall entry
    // Resolve the MCP server path relative to the app bundle
    let server_path = factory_dir.join("mcp-remembrall").join("dist").join("server.js");
    let server_path_str = server_path.to_string_lossy().to_string();

    config.insert(
        "remembrall".to_string(),
        serde_json::json!({
            "command": "node",
            "args": [server_path_str]
        }),
    );

    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize mcp.json: {}", e))?;
    std::fs::write(&mcp_path, config_json).map_err(|e| format!("Failed to write mcp.json: {}", e))?;

    // 2. Append trigger line to ~/.factory/AGENTS.md (only if not already present)
    let agents_path = factory_dir.join("AGENTS.md");
    let trigger_line = "- Remembrall recall tool: Use the `recall` MCP tool at the start of every new session to load project context.";

    let existing = if agents_path.exists() {
        std::fs::read_to_string(&agents_path).unwrap_or_default()
    } else {
        String::new()
    };

    if !existing.contains("Remembrall recall tool") {
        let new_content = if existing.is_empty() {
            trigger_line.to_string()
        } else {
            format!("{}\n{}", existing.trim_end(), trigger_line)
        };
        std::fs::write(&agents_path, new_content).map_err(|e| format!("Failed to write AGENTS.md: {}", e))?;
    }

    log::info!("MCP registration complete");
    Ok(serde_json::json!({ "success": true }))
}

/// Check if MCP is already registered in ~/.factory/mcp.json.
#[tauri::command]
pub async fn check_mcp_registered() -> Result<bool, String> {
    log::info!("check_mcp_registered called");

    let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;
    let mcp_path = home.join(".factory").join("mcp.json");

    if !mcp_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&mcp_path).map_err(|e| format!("Failed to read mcp.json: {}", e))?;
    let config: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&content).unwrap_or_default();

    Ok(config.contains_key("remembrall"))
}
