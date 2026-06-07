/// Filter + sort + paginated memory list for Browse view.
#[tauri::command]
pub async fn get_memories_page(
    _type_filter: Option<String>,
    _project_filter: Option<String>,
    _date_range: Option<(String, String)>,
    _decay_state: Option<String>,
    _sort: String,
    _page: usize,
    _page_size: usize,
) -> Result<serde_json::Value, String> {
    log::info!("get_memories_page called");
    // TODO: Wire up to db::queries::get_memories_page
    Ok(serde_json::json!({
        "memories": [],
        "total": 0
    }))
}

/// Get available filter options (types, projects, date range bounds).
#[tauri::command]
pub async fn get_filters() -> Result<serde_json::Value, String> {
    log::info!("get_filters called");
    // TODO: Wire up to db::queries::get_memory_types and get_projects
    Ok(serde_json::json!({
        "types": [],
        "projects": []
    }))
}

/// Get all projects with memory counts.
#[tauri::command]
pub async fn get_projects() -> Result<serde_json::Value, String> {
    log::info!("get_projects called");
    // TODO: Wire up to db::queries::get_projects
    Ok(serde_json::json!([]))
}
