use crate::db;
use super::search::MemoryRow;

/// Filter + sort + paginated memory list for Browse view.
#[tauri::command]
pub async fn get_memories_page(
    type_filter: Option<String>,
    project_filter: Option<String>,
    date_range: Option<(String, String)>,
    decay_state: Option<String>,
    sort: String,
    page: usize,
    page_size: usize,
) -> Result<serde_json::Value, String> {
    log::info!("get_memories_page called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;

    // Build filters from loose params
    let mut filters = db::queries::MemoryFilters::default();
    if let Some(tf) = &type_filter {
        filters.type_names = Some(vec![tf.clone()]);
    }
    if let Some(pf) = &project_filter {
        // Look up project_id by name
        let projects = db::queries::get_projects(&conn).map_err(|e| e.to_string())?;
        let ids: Vec<i64> = projects
            .iter()
            .filter(|p| p.name == *pf)
            .map(|p| p.id)
            .collect();
        if !ids.is_empty() {
            filters.project_ids = Some(ids);
        }
    }
    if let Some((from, to)) = &date_range {
        filters.date_from = Some(from.clone());
        filters.date_to = Some(to.clone());
    }
    if let Some(ds) = &decay_state {
        match ds.as_str() {
            "strong" => { filters.min_strength = Some(0.5); }
            "fading" => { filters.min_strength = Some(0.01); filters.max_strength = Some(0.5); }
            "archived" => { filters.is_active = Some(false); }
            _ => {}
        }
    }

    // Parse sort field
    let sort_option = db::queries::SortOption {
        field: match sort.as_str() {
            "strength" => db::queries::SortField::Strength,
            "type" => db::queries::SortField::Type,
            "importance" => db::queries::SortField::Importance,
            _ => db::queries::SortField::Recency,
        },
        ascending: false,
    };

    let pagination = db::queries::Pagination {
        page: page as i64,
        page_size: page_size as i64,
    };

    let result = db::queries::get_memories_page(&conn, &filters, &sort_option, &pagination)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "memories": result.memories.into_iter().map(MemoryRow::from).collect::<Vec<_>>(),
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size,
        "total_pages": result.total_pages,
    }))
}

/// Get available filter options (types, projects, date range bounds).
#[tauri::command]
pub async fn get_filters() -> Result<serde_json::Value, String> {
    log::info!("get_filters called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let types = db::queries::get_memory_types(&conn).map_err(|e| e.to_string())?;
    let projects = db::queries::get_projects(&conn).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "types": types,
        "projects": projects,
    }))
}

/// Get all projects with memory counts.
#[tauri::command]
pub async fn get_projects() -> Result<serde_json::Value, String> {
    log::info!("get_projects called");
    let conn = db::queries::open_connection().map_err(|e| e.to_string())?;
    let projects = db::queries::get_projects(&conn).map_err(|e| e.to_string())?;
    Ok(serde_json::json!(projects))
}
