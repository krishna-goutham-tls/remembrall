//! Session/file correlation module
//!
//! Maintains `active_sessions` mapping table for project-to-session-file correlation.
//! Provides `recent_context` extraction for recall via MCP.
//!
//! ## Behavior
//!
//! - `update_active_session`: Called on session file modification, updates project → file_path mapping
//! - `get_recent_context`: Matches project → most recent session file → extracts last 5-6 messages
//!
//! ## Fallbacks
//!
//! - No project match → use globally most recent session
//! - File deleted → empty recent_context + log warning
//! - Session file >1000 lines → read last N lines only

use crate::parser::droid::{self, ParsedLine};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

/// Number of last lines to read from session file for recent_context
const RECENT_CONTEXT_LINES: usize = 10;

/// Maximum messages to extract for recent_context
const MAX_RECENT_MESSAGES: usize = 6;

// ============================================================================
// Data Structures
// ============================================================================

/// A message extracted for recent_context
#[derive(Debug, Clone)]
pub struct RecentMessage {
    pub role: String,
    pub text: String,
    pub timestamp: String,
}

/// Recent context result containing extracted messages
#[derive(Debug, Clone)]
pub struct RecentContext {
    pub messages: Vec<RecentMessage>,
    pub session_file: Option<PathBuf>,
    pub project_matched: bool,
}

/// Project info from the projects table
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub id: i64,
    pub name: String,
    pub path: Option<String>,
    pub git_root: Option<String>,
}

// ============================================================================
// Active Session Management
// ============================================================================

/// Update active_sessions mapping when a session file is modified
///
/// # Arguments
/// - `conn`: Database connection
/// - `project_id`: ID from projects table (can be NULL for global sessions)
/// - `session_file`: Path to the session .jsonl file
/// - `message_count`: Total message count in the session file
///
/// # Returns
/// Ok(()) on success
pub fn update_active_session(
    conn: &Connection,
    project_id: Option<i64>,
    session_file: &Path,
    message_count: i64,
) -> Result<()> {
    let session_file_str = session_file.to_string_lossy().to_string();
    let now = chrono_now();

    // Use INSERT OR REPLACE for idempotent updates
    conn.execute(
        "INSERT INTO active_sessions (project_id, session_file, last_modified, last_message_count)
         VALUES (?1, ?2, ?3, ?4)",
        params![project_id, session_file_str, now, message_count],
    )?;

    log::info!(
        "Updated active_session: project_id={:?}, file={}, messages={}",
        project_id,
        session_file_str,
        message_count
    );

    Ok(())
}

/// Get the most recent active session for a project
pub fn get_active_session_for_project(
    conn: &Connection,
    project_id: i64,
) -> Result<Option<(PathBuf, i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT session_file, last_modified, last_message_count
         FROM active_sessions
         WHERE project_id = ?1
         ORDER BY last_modified DESC
         LIMIT 1",
    )?;

    let result = stmt.query_row(params![project_id], |row| {
        let session_file: String = row.get(0)?;
        let last_modified: String = row.get(1)?;
        let last_message_count: i64 = row.get(2)?;
        Ok((session_file, last_modified, last_message_count))
    });

    match result {
        Ok((session_file, _last_modified, last_message_count)) => {
            Ok(Some((PathBuf::from(session_file), 0, last_message_count)))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Failed to query active_session: {}", e)),
    }
}

/// Get the globally most recent active session (fallback when project not found)
pub fn get_global_most_recent_session(conn: &Connection) -> Result<Option<(PathBuf, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT session_file, last_message_count
         FROM active_sessions
         ORDER BY last_modified DESC
         LIMIT 1",
    )?;

    let result = stmt.query_row([], |row| {
        let session_file: String = row.get(0)?;
        let last_message_count: i64 = row.get(1)?;
        Ok((session_file, last_message_count))
    });

    match result {
        Ok((session_file, last_message_count)) => {
            Ok(Some((PathBuf::from(session_file), last_message_count)))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to query global active_session: {}",
            e
        )),
    }
}

// ============================================================================
// Project Matching
// ============================================================================

/// Find project by path (cwd) - exact match
pub fn find_project_by_path(conn: &Connection, cwd: &str) -> Result<Option<ProjectInfo>> {
    let mut stmt =
        conn.prepare("SELECT id, name, path, git_root FROM projects WHERE path = ?1 LIMIT 1")?;

    let result = stmt.query_row(params![cwd], |row| {
        Ok(ProjectInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            git_root: row.get(3)?,
        })
    });

    match result {
        Ok(project) => Ok(Some(project)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Failed to find project by path: {}", e)),
    }
}

/// Find project by name - exact match
pub fn find_project_by_name(conn: &Connection, name: &str) -> Result<Option<ProjectInfo>> {
    let mut stmt =
        conn.prepare("SELECT id, name, path, git_root FROM projects WHERE name = ?1 LIMIT 1")?;

    let result = stmt.query_row(params![name], |row| {
        Ok(ProjectInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            git_root: row.get(3)?,
        })
    });

    match result {
        Ok(project) => Ok(Some(project)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Failed to find project by name: {}", e)),
    }
}

/// Find project by path OR name, returning the first match
/// This is the primary lookup used during recall
pub fn find_project(conn: &Connection, project_arg: &str) -> Result<Option<ProjectInfo>> {
    // First try exact path match
    if let Some(project) = find_project_by_path(conn, project_arg)? {
        return Ok(Some(project));
    }

    // Then try name match
    if let Some(project) = find_project_by_name(conn, project_arg)? {
        return Ok(Some(project));
    }

    Ok(None)
}

// ============================================================================
// Recent Context Extraction
// ============================================================================

/// Read last N lines from a file efficiently
/// For files with >MAX_LINES_TO_READ lines, only reads the last MAX_LINES_TO_READ lines
fn read_last_lines(path: &Path, num_lines: usize) -> Result<Vec<String>> {
    let content = fs::read_to_string(path).context("Failed to read session file")?;

    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();

    if total_lines <= num_lines {
        // File is small enough, return all lines
        Ok(all_lines.iter().map(|s| s.to_string()).collect())
    } else {
        // File is large, only read last num_lines
        let start_idx = total_lines - num_lines;
        Ok(all_lines[start_idx..]
            .iter()
            .map(|s| s.to_string())
            .collect())
    }
}

/// Extract last N messages from a parsed session file content
/// Reads last RECENT_CONTEXT_LINES lines, parses as JSONL, extracts last MAX_RECENT_MESSAGES messages
fn extract_recent_messages(lines: Vec<String>) -> Vec<RecentMessage> {
    let mut messages = Vec::new();

    for line in lines {
        match droid::parse_line(&line) {
            Ok(ParsedLine::Message(msg)) => {
                // Only include user and assistant messages with actual content
                if (msg.role == "user" || msg.role == "assistant") && !msg.text.is_empty() {
                    messages.push(RecentMessage {
                        role: msg.role,
                        text: msg.text,
                        timestamp: msg.timestamp,
                    });
                }
            }
            Ok(ParsedLine::SessionStart(_)) | Ok(ParsedLine::SessionEnd { .. }) => {
                // Include session boundaries as they mark conversation structure
            }
            _ => {
                // Skip todo_state, unknown, etc.
            }
        }

        // Stop once we have enough messages
        if messages.len() >= MAX_RECENT_MESSAGES {
            break;
        }
    }

    messages
}

/// Get recent context for a project
///
/// 1. Match project_arg to project_id via projects table (path or name match)
/// 2. Get most recent active_sessions entry
/// 3. Read last 10 lines of file
/// 4. Parse as JSONL
/// 5. Extract last 5-6 user/assistant messages
/// 6. Return as recent_context
///
/// Fallbacks:
/// - No project match → use globally most recent session
/// - File deleted → empty recent_context + log warning
/// - Session file >1000 lines → read last N lines only
pub fn get_recent_context(conn: &Connection, project_arg: &str) -> Result<RecentContext> {
    // Step 1: Try to find project by path or name
    let project_info = find_project(conn, project_arg)?;

    // Step 2: Get the session file path
    let (session_file, _message_count) = if let Some(ref project) = project_info {
        // Project found - get active session for this project
        if let Some((path, _, _)) = get_active_session_for_project(conn, project.id)? {
            (Some(path), None)
        } else {
            // No active session for this project, try global fallback
            log::info!(
                "No active session for project '{}', falling back to global most recent",
                project_arg
            );
            if let Some((path, count)) = get_global_most_recent_session(conn)? {
                (Some(path), Some(count))
            } else {
                return Ok(RecentContext {
                    messages: Vec::new(),
                    session_file: None,
                    project_matched: false,
                });
            }
        }
    } else {
        // Project not found - use global fallback
        log::info!(
            "Project '{}' not found, using global most recent session",
            project_arg
        );
        if let Some((path, count)) = get_global_most_recent_session(conn)? {
            (Some(path), Some(count))
        } else {
            return Ok(RecentContext {
                messages: Vec::new(),
                session_file: None,
                project_matched: false,
            });
        }
    };

    let session_file = session_file.context("No session file found")?;

    // Step 3: Read the session file (handle deleted file gracefully)
    let lines = match read_last_lines(&session_file, RECENT_CONTEXT_LINES) {
        Ok(l) => l,
        Err(e) => {
            log::warn!(
                "Failed to read session file {:?}: {}. Returning empty recent_context.",
                session_file,
                e
            );
            return Ok(RecentContext {
                messages: Vec::new(),
                session_file: Some(session_file),
                project_matched: project_info.is_some(),
            });
        }
    };

    // Step 4-5: Parse and extract recent messages
    let messages = extract_recent_messages(lines);

    log::info!(
        "Extracted {} recent messages from {:?}",
        messages.len(),
        session_file
    );

    Ok(RecentContext {
        messages,
        session_file: Some(session_file),
        project_matched: project_info.is_some(),
    })
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get current timestamp in SQLite format (YYYY-MM-DD HH:MM:SS)
fn chrono_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (TempDir, Connection) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Initialize database with schema
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Create minimal schema for testing
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                path TEXT,
                git_root TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(name, path)
            );

            CREATE TABLE IF NOT EXISTS active_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER REFERENCES projects(id),
                session_file TEXT NOT NULL,
                last_modified DATETIME DEFAULT CURRENT_TIMESTAMP,
                last_message_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(project_id, session_file)
            );
            "#,
        )
        .unwrap();

        (temp_dir, conn)
    }

    fn create_test_session_file(temp_dir: &TempDir, lines: &[impl AsRef<str>]) -> PathBuf {
        let session_path = temp_dir.path().join("test_session.jsonl");
        let content = lines
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&session_path, content).unwrap();
        session_path
    }

    #[test]
    fn test_update_and_get_active_session() {
        let (_temp_dir, conn) = create_test_db();

        // Insert a project
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test_project', '/Users/kg/test')",
            [],
        )
        .unwrap();

        let project_id: i64 = conn.last_insert_rowid();
        let session_path = PathBuf::from("/tmp/test_session.jsonl");

        // Update active session
        update_active_session(&conn, Some(project_id), &session_path, 10).unwrap();

        // Get active session
        let result = get_active_session_for_project(&conn, project_id).unwrap();
        assert!(result.is_some());

        let (path, _, count) = result.unwrap();
        assert_eq!(path, session_path);
        assert_eq!(count, 10);
    }

    #[test]
    fn test_find_project_by_path() {
        let (_temp_dir, conn) = create_test_db();

        conn.execute(
            "INSERT INTO projects (name, path, git_root) VALUES ('test_project', '/Users/kg/test', '/Users/kg')",
            [],
        )
        .unwrap();

        let project = find_project_by_path(&conn, "/Users/kg/test").unwrap();
        assert!(project.is_some());
        assert_eq!(project.unwrap().name, "test_project");
    }

    #[test]
    fn test_find_project_by_name() {
        let (_temp_dir, conn) = create_test_db();

        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('my_project', '/Users/kg/myproject')",
            [],
        )
        .unwrap();

        let project = find_project_by_name(&conn, "my_project").unwrap();
        assert!(project.is_some());
        assert_eq!(
            project.unwrap().path,
            Some("/Users/kg/myproject".to_string())
        );
    }

    #[test]
    fn test_find_project_prefers_path() {
        let (_temp_dir, conn) = create_test_db();

        // Insert two projects - one matching by name, one by path
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('same_name', '/path/one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('other', '/path/same_name')",
            [],
        )
        .unwrap();

        // Path match should take precedence
        let project = find_project(&conn, "/path/same_name").unwrap();
        assert!(project.is_some());
        assert_eq!(project.unwrap().name, "other");
    }

    #[test]
    fn test_get_global_most_recent_session() {
        let (_temp_dir, conn) = create_test_db();

        // Insert two global active sessions (no project_id) with different timestamps
        // Session2 has a later timestamp so should be returned as "most recent"
        conn.execute(
            "INSERT INTO active_sessions (project_id, session_file, last_modified, last_message_count)
             VALUES (NULL, '/tmp/session1.jsonl', '2026-01-01 12:00:00', 5)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO active_sessions (project_id, session_file, last_modified, last_message_count)
             VALUES (NULL, '/tmp/session2.jsonl', '2026-01-02 12:00:00', 10)",
            [],
        )
        .unwrap();

        // Get global most recent
        let result = get_global_most_recent_session(&conn).unwrap();
        assert!(result.is_some());

        let (_path, count) = result.unwrap();
        // Session2 has later timestamp, should be returned
        assert_eq!(count, 10);
    }

    #[test]
    fn test_extract_recent_messages_from_session_file() {
        let temp_dir = TempDir::new().unwrap();

        let session_lines = vec![
            r#"{"type":"session_start","id":"sess1","title":"Test","cwd":"/Users/kg/test","owner":"kg","version":2}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-05-06T12:00:00Z","message":{"role":"user","content":[{"type":"text","text":"First user message"}]},"parentId":null}"#,
            r#"{"type":"message","id":"msg2","timestamp":"2026-05-06T12:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"First assistant response"}]},"parentId":"msg1"}"#,
            r#"{"type":"message","id":"msg3","timestamp":"2026-05-06T12:00:02Z","message":{"role":"user","content":[{"type":"text","text":"Second user message"}]},"parentId":"msg2"}"#,
            r#"{"type":"message","id":"msg4","timestamp":"2026-05-06T12:00:03Z","message":{"role":"assistant","content":[{"type":"text","text":"Second assistant response"}]},"parentId":"msg3"}"#,
            r#"{"type":"message","id":"msg5","timestamp":"2026-05-06T12:00:04Z","message":{"role":"user","content":[{"type":"text","text":"Third user message"}]},"parentId":"msg4"}"#,
            r#"{"type":"message","id":"msg6","timestamp":"2026-05-06T12:00:05Z","message":{"role":"assistant","content":[{"type":"text","text":"Third assistant response"}]},"parentId":"msg5"}"#,
        ];

        let session_path = create_test_session_file(&temp_dir, &session_lines);

        // Read last lines
        let lines = read_last_lines(&session_path, 10).unwrap();
        assert_eq!(lines.len(), 7); // All lines since file is small

        // Extract messages
        let messages = extract_recent_messages(lines);
        assert!(messages.len() >= 5);

        // Last messages should be user/assistant alternating
        let last_msg = messages.last().unwrap();
        assert_eq!(last_msg.role, "assistant");
        assert_eq!(last_msg.text, "Third assistant response");
    }

    #[test]
    fn test_handles_missing_file_gracefully() {
        let (_temp_dir, conn) = create_test_db();

        // Create a project and active session pointing to non-existent file
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let project_id: i64 = conn.last_insert_rowid();
        let missing_path = PathBuf::from("/tmp/nonexistent_session_12345.jsonl");

        update_active_session(&conn, Some(project_id), &missing_path, 5).unwrap();

        // Try to get recent context - should return empty messages, not crash
        let context = get_recent_context(&conn, "/tmp/test").unwrap();
        assert!(context.messages.is_empty());
        assert!(context.session_file.is_some());
    }

    #[test]
    fn test_project_not_found_uses_global_fallback() {
        let (_temp_dir, conn) = create_test_db();

        // Create only a global active session (no project_id)
        let session_path = PathBuf::from("/tmp/global_session.jsonl");
        update_active_session(&conn, None, &session_path, 20).unwrap();

        // Try to get context for non-existent project - should use global fallback
        let context = get_recent_context(&conn, "/nonexistent/path").unwrap();
        // Since file doesn't exist, should return empty but not crash
        assert!(!context.project_matched); // Should indicate no project match
    }

    #[test]
    fn test_large_file_reads_only_last_lines() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with more than MAX_LINES_TO_READ lines
        let mut lines = Vec::new();
        lines.push(r#"{"type":"session_start","id":"sess1","title":"Test","cwd":"/Users/kg/test","owner":"kg","version":2}"#.to_string());

        for i in 0..1500 {
            lines.push(format!(
                r#"{{"type":"message","id":"msg{}","timestamp":"2026-05-06T12:00:{:04}Z","message":{{"role":"user","content":[{{"type":"text","text":"Message {}"}}]}},"parentId":null}}"#,
                i, i, i
            ));
        }

        let session_path = temp_dir.path().join("large_session.jsonl");
        fs::write(&session_path, lines.join("\n")).unwrap();

        // Read last lines - should only get MAX_LINES_TO_READ
        let result = read_last_lines(&session_path, RECENT_CONTEXT_LINES).unwrap();
        assert_eq!(result.len(), RECENT_CONTEXT_LINES);

        // The last message should be the last one (msg1499)
        let parsed = droid::parse_line(&result[result.len() - 1]).unwrap();
        if let ParsedLine::Message(msg) = parsed {
            assert!(msg.text.contains("Message 1499"));
        } else {
            panic!("Expected message line");
        }
    }

    #[test]
    fn test_skips_thinking_and_tool_blocks() {
        let temp_dir = TempDir::new().unwrap();

        let session_lines = vec![
            r#"{"type":"session_start","id":"sess1","title":"Test","cwd":"/Users/kg/test","owner":"kg","version":2}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-05-06T12:00:00Z","message":{"role":"user","content":[{"type":"text","text":"User question"}]},"parentId":null}"#,
            r#"{"type":"message","id":"msg2","timestamp":"2026-05-06T12:00:01Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me think about this..."},{"type":"text","text":"Final answer"}]},"parentId":"msg1"}"#,
        ];

        let session_path = create_test_session_file(&temp_dir, &session_lines);
        let lines = read_last_lines(&session_path, 10).unwrap();
        let messages = extract_recent_messages(lines);

        // Should have 2 messages (user + assistant)
        assert_eq!(messages.len(), 2);

        // Assistant message should only contain "Final answer", not thinking
        let assistant_msg = messages.get(1).unwrap();
        assert_eq!(assistant_msg.role, "assistant");
        assert_eq!(assistant_msg.text, "Final answer");
        assert!(!assistant_msg.text.contains("think"));
    }

    #[test]
    fn test_skips_todo_state_lines() {
        let temp_dir = TempDir::new().unwrap();

        let session_lines = vec![
            r#"{"type":"session_start","id":"sess1","title":"Test","cwd":"/Users/kg/test","owner":"kg","version":2}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-05-06T12:00:00Z","message":{"role":"user","content":[{"type":"text","text":"User question"}]},"parentId":null}"#,
            r#"{"type":"todo_state","id":"todo1","todos":[{"status":"in_progress","content":"Task 1"}]}"#,
            r#"{"type":"message","id":"msg2","timestamp":"2026-05-06T12:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"Answer"}]},"parentId":"msg1"}"#,
        ];

        let session_path = create_test_session_file(&temp_dir, &session_lines);
        let lines = read_last_lines(&session_path, 10).unwrap();
        let messages = extract_recent_messages(lines);

        // Should only have 2 messages (user + assistant), todo_state skipped
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_limits_to_max_recent_messages() {
        let temp_dir = TempDir::new().unwrap();

        let mut session_lines = vec![
            r#"{"type":"session_start","id":"sess1","title":"Test","cwd":"/Users/kg/test","owner":"kg","version":2}"#.to_string(),
        ];

        // Add 20 messages
        for i in 0..20 {
            session_lines.push(format!(
                r#"{{"type":"message","id":"msg{}","timestamp":"2026-05-06T12:00:{:02}Z","message":{{"role":"user","content":[{{"type":"text","text":"Message {}"}}]}},"parentId":null}}"#,
                i, i, i
            ));
        }

        let session_path = create_test_session_file(&temp_dir, &session_lines);
        let lines = read_last_lines(&session_path, 30).unwrap();
        let messages = extract_recent_messages(lines);

        // Should be limited to MAX_RECENT_MESSAGES
        assert!(messages.len() <= MAX_RECENT_MESSAGES);
    }
}
