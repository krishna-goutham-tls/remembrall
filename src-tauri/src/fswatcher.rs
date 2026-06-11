//! FSEvents watcher for monitoring ~/.factory/sessions/
//!
//! Watches for .jsonl file changes, parses them via the Droid parser,
//! redacts secrets, and inserts memories into the brain database.

use crate::commands::icon_state::set_icon_state;
use crate::commands::icon_state::IconState;
use crate::correlation;
use crate::db;
use crate::parser;
use crate::redaction;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{App, Manager, Runtime};

/// State holding the active watcher, stored in Tauri managed state.
pub struct WatcherState {
    pub watcher: Mutex<Option<RecommendedWatcher>>,
}

/// Start the file system watcher on ~/.factory/sessions/.
/// Gracefully handles missing directory.
pub fn start_watcher<R: Runtime>(app: &App<R>) {
    let sessions_dir = parser::droid::get_sessions_dir();

    if !sessions_dir.exists() {
        log::warn!(
            "Sessions directory {:?} does not exist — watcher not started",
            sessions_dir
        );
        return;
    }

    let app_handle = app.handle().clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            let event = match res {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Watcher error: {:?}", e);
                    return;
                }
            };

            // Only process file create/modify events
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {}
                _ => return,
            }

            // Filter to .jsonl files only
            for path in &event.paths {
                if path.extension().is_some_and(|ext| ext == "jsonl") {
                    let path_clone = path.clone();
                    let ah = app_handle.clone();
                    std::thread::spawn(move || {
                        process_and_insert_file(&path_clone, &ah);
                    });
                }
            }
        },
        Config::default(),
    )
    .expect("Failed to create file watcher");

    watcher
        .watch(&sessions_dir, RecursiveMode::Recursive)
        .expect("Failed to start watching sessions directory");

    log::info!("FSWatcher started on {:?}", sessions_dir);

    // Store watcher in managed state
    let state = app.state::<WatcherState>();
    *state.watcher.lock().unwrap() = Some(watcher);
}

/// Debounce tracker — maps file paths to last-processed timestamps.
/// 500ms debounce per file to avoid processing the same file too frequently.
static DEBOUNCE_MAP: once_cell::sync::Lazy<Mutex<HashMap<PathBuf, Instant>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

/// Process a changed .jsonl file: parse → redact → insert memories.
fn process_and_insert_file<R: Runtime>(path: &PathBuf, _app: &tauri::AppHandle<R>) {
    // Debounce check
    {
        let mut map = DEBOUNCE_MAP.lock().unwrap();
        let now = Instant::now();
        if let Some(last) = map.get(path) {
            if now.duration_since(*last) < DEBOUNCE_INTERVAL {
                log::debug!("Debouncing {:?}", path);
                return;
            }
        }
        map.insert(path.clone(), now);
    }

    log::info!("Processing session file: {:?}", path);
    set_icon_state(IconState::Indexing);

    // Parse the session file
    let batches = match parser::droid::process_session_file(path) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to parse session file {:?}: {}", path, e);
            set_icon_state(IconState::Error);
            return;
        }
    };

    if batches.is_empty() {
        log::info!("No turn batches in {:?}", path);
        set_icon_state(IconState::Idle);
        return;
    }

    // Open DB connection for all inserts
    let conn = match db::queries::open_connection() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to open DB for indexing: {}", e);
            set_icon_state(IconState::Error);
            return;
        }
    };

    // Get project info from the first batch's session
    let first_session = &batches[0].session;
    let project_identity = parser::droid::determine_project(&first_session.cwd);

    // Update active_sessions tracking
    let message_count = batches.iter().map(|b| b.turns.len() as i64).sum();
    let project_id = find_project_id(&conn, &project_identity);
    let _ = correlation::update_active_session(&conn, project_id, path, message_count);

    // Insert memories from each turn batch
    let mut inserted = 0;
    for batch in &batches {
        for turn in &batch.turns {
            // Concatenate user + assistant text
            let mut full_text = turn.user_message.text.clone();
            if let Some(ref assistant) = turn.assistant_message {
                if !assistant.text.is_empty() {
                    full_text.push_str("\n");
                    full_text.push_str(&assistant.text);
                }
            }

            if full_text.trim().is_empty() {
                continue;
            }

            // Redact secrets
            let redacted = redaction::redact(&full_text);

            // Extract summary: first 200 chars, truncated at word boundary
            let summary = truncate_at_word(&redacted, 200);

            // Extract keywords: unique lowercase words > 3 chars, up to 5
            let keywords = extract_keywords(&summary);

            // Insert memory with hardcoded defaults
            match db::queries::insert_memory(
                &conn,
                Some(&project_identity.name),
                Some(&project_identity.path),
                12,           // type_id: task_detail
                "droid",       // source_tool
                &summary,
                Some(&keywords),
                "project",     // scope
                0.5,           // importance
            ) {
                Ok(_) => inserted += 1,
                Err(e) => log::error!("Failed to insert memory: {}", e),
            }
        }
    }

    log::info!(
        "Indexed {} memories from {:?} (project: {})",
        inserted,
        path,
        project_identity.name
    );

    set_icon_state(IconState::NewMemories);
}

/// Find project_id by name, creating the project if needed.
/// Returns None if no project match (insert_memory will auto-create).
fn find_project_id(conn: &rusqlite::Connection, project: &parser::droid::ProjectIdentity) -> Option<i64> {
    let projects = db::queries::get_projects(conn).ok()?;
    projects.iter().find(|p| p.name == project.name).map(|p| p.id)
}

/// Truncate text at the last word boundary before `max_len`.
fn truncate_at_word(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let truncated = &text[..max_len];
    match truncated.rfind(' ') {
        Some(pos) => truncated[..pos].to_string(),
        None => truncated.to_string(),
    }
}

/// Extract up to 5 unique lowercase keywords (>3 chars) from text.
/// Filters out common stop words.
fn extract_keywords(text: &str) -> String {
    const STOP_WORDS: &[&str] = &[
        "that", "this", "with", "from", "have", "will", "been", "they",
        "their", "which", "would", "about", "could", "other", "than",
        "very", "just", "also", "some", "more", "into", "over", "after",
        "what", "when", "where", "which", "there", "these", "those",
    ];

    let mut seen = std::collections::HashSet::new();
    let mut keywords = Vec::new();

    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let lower = word.to_lowercase();
        let lower = lower.as_str();
        if lower.len() > 3
            && !STOP_WORDS.contains(&lower)
            && seen.insert(lower.to_string())
        {
            keywords.push(lower.to_string());
            if keywords.len() >= 5 {
                break;
            }
        }
    }

    keywords.join(", ")
}
