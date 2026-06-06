//! Droid JSONL Parser - Streaming parser for verified Droid session format
//!
//! Handles 4 line types:
//! - `session_start`: extracts id, cwd, title, owner
//! - `message`: extracts role and concatenates text blocks (skips thinking/tool_use/tool_result)
//! - `todo_state`: tracks state changes (content skipped)
//! - `session_end`: marks session as complete
//!
//! Groups messages into turns (user + assistant = 1 turn), batches 1-3 turns for processing.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Session directory path: ~/.factory/sessions/
pub fn get_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".factory")
        .join("sessions")
}

/// Convert cwd path to directory name (slashes → dashes)
/// Example: /Users/kg/Desktop/Work → -Users-kg-Desktop-Work
pub fn cwd_to_dir_name(cwd: &str) -> String {
    cwd.replace('/', "-")
}

/// Convert directory name back to cwd path
/// Example: -Users-kg-Desktop-Work → /Users/kg/Desktop/Work
pub fn dir_name_to_cwd(dir_name: &str) -> String {
    dir_name.replace('-', "/")
}

// ============================================================================
// Data Structures
// ============================================================================

/// Represents a parsed session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSession {
    pub id: String,
    pub cwd: String,
    pub title: String,
    pub owner: String,
}

/// Content block within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        content: String,
    },
}

/// Represents a parsed message with extracted text content
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub id: String,
    pub role: String, // "user" or "assistant"
    pub text: String, // Concatenated text from content blocks (text type only)
    pub timestamp: String,
    pub parent_id: Option<String>,
}

/// Represents a parsed line from the JSONL file
#[derive(Debug)]
pub enum ParsedLine {
    SessionStart(ParsedSession),
    Message(ParsedMessage),
    TodoState { id: String, todos: Vec<TodoItem> },
    SessionEnd { id: String, reason: Option<String> },
    Unknown(String),
}

/// A todo item from todo_state lines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub status: String,
    pub content: String,
}

/// A turn represents one user+assistant exchange
#[derive(Debug, Clone)]
pub struct Turn {
    pub user_message: ParsedMessage,
    pub assistant_message: Option<ParsedMessage>,
}

/// A batch of turns for processing (1-3 turns)
#[derive(Debug, Clone)]
pub struct TurnBatch {
    pub turns: Vec<Turn>,
    pub session: ParsedSession,
}

/// Project identity extracted from session
#[derive(Debug, Clone)]
pub struct ProjectIdentity {
    pub name: String, // Derived from cwd last component
    pub path: String, // Original cwd path
}

// ============================================================================
// JSONL Line Parsing
// ============================================================================

/// Raw JSONL line as received from the file
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawSessionStart {
    #[serde(rename = "type")]
    line_type: String,
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(rename = "sessionTitle")]
    #[serde(default)]
    session_title: Option<String>,
    cwd: String,
    owner: String,
    #[serde(default)]
    version: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawMessage {
    #[serde(rename = "type")]
    line_type: String,
    id: String,
    timestamp: String,
    message: RawMessageContent,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMessageContent {
    role: String,
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawTodoState {
    #[serde(rename = "type")]
    line_type: String,
    id: String,
    #[serde(default)]
    todos: Vec<TodoItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawSessionEnd {
    #[serde(rename = "type")]
    line_type: String,
    id: String,
    #[serde(default)]
    reason: Option<String>,
}

/// Parse a single JSONL line and return a ParsedLine
pub fn parse_line(line: &str) -> Result<ParsedLine> {
    // Skip empty lines
    let line = line.trim();
    if line.is_empty() {
        return Ok(ParsedLine::Unknown(String::new()));
    }

    // First, determine the line type by parsing just the type field
    let type_field: serde_json::Value =
        serde_json::from_str(line).context("Failed to parse JSON line")?;

    let line_type = type_field
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match line_type {
        "session_start" => {
            let raw: RawSessionStart =
                serde_json::from_str(line).context("Failed to parse session_start line")?;
            Ok(ParsedLine::SessionStart(ParsedSession {
                id: raw.id,
                cwd: raw.cwd,
                title: raw
                    .title
                    .or(raw.session_title)
                    .unwrap_or_else(|| "Untitled".to_string()),
                owner: raw.owner,
            }))
        }
        "message" => {
            let raw: RawMessage =
                serde_json::from_str(line).context("Failed to parse message line")?;

            // Extract text from content blocks (only type: "text")
            let text = raw
                .message
                .content
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            Ok(ParsedLine::Message(ParsedMessage {
                id: raw.id,
                role: raw.message.role,
                text,
                timestamp: raw.timestamp,
                parent_id: raw.parent_id,
            }))
        }
        "todo_state" => {
            let raw: RawTodoState =
                serde_json::from_str(line).context("Failed to parse todo_state line")?;
            Ok(ParsedLine::TodoState {
                id: raw.id,
                todos: raw.todos,
            })
        }
        "session_end" => {
            let raw: RawSessionEnd =
                serde_json::from_str(line).context("Failed to parse session_end line")?;
            Ok(ParsedLine::SessionEnd {
                id: raw.id,
                reason: raw.reason,
            })
        }
        _ => Ok(ParsedLine::Unknown(line_type.to_string())),
    }
}

/// Parse a JSONL file and return all parsed lines
pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Vec<ParsedLine>> {
    let content = fs::read_to_string(path.as_ref()).context("Failed to read session file")?;

    let mut lines = Vec::new();
    for line in content.lines() {
        let parsed = parse_line(line)?;
        if !matches!(parsed, ParsedLine::Unknown(_)) {
            lines.push(parsed);
        }
    }

    Ok(lines)
}

// ============================================================================
// Turn Grouping and Batching
// ============================================================================

/// Group parsed messages into turns (user + assistant = 1 turn)
#[allow(clippy::single_match)]
pub fn group_into_turns(lines: Vec<ParsedLine>) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut pending_user: Option<ParsedMessage> = None;

    for line in lines {
        match line {
            ParsedLine::Message(msg) => {
                if msg.role == "user" {
                    // If we have a pending user message without assistant response,
                    // it means the previous turn is incomplete - save it
                    if let Some(user_msg) = pending_user.take() {
                        turns.push(Turn {
                            user_message: user_msg,
                            assistant_message: None,
                        });
                    }
                    pending_user = Some(msg);
                } else if msg.role == "assistant" {
                    if let Some(user_msg) = pending_user.take() {
                        // Complete a turn with user + assistant
                        turns.push(Turn {
                            user_message: user_msg,
                            assistant_message: Some(msg),
                        });
                    } else {
                        // Assistant without preceding user - start a new turn with no user
                        // This shouldn't happen in normal sessions but handle it gracefully
                        turns.push(Turn {
                            user_message: ParsedMessage {
                                id: String::new(),
                                role: "unknown".to_string(),
                                text: String::new(),
                                timestamp: msg.timestamp.clone(),
                                parent_id: None,
                            },
                            assistant_message: Some(msg),
                        });
                    }
                }
            }
            // todo_state and session_end don't affect turn grouping
            _ => {}
        }
    }

    // Handle any remaining pending user message without assistant response
    if let Some(user_msg) = pending_user {
        turns.push(Turn {
            user_message: user_msg,
            assistant_message: None,
        });
    }

    turns
}

/// Batch turns into groups of 1-3 turns
pub fn batch_turns(turns: Vec<Turn>, session: ParsedSession) -> Vec<TurnBatch> {
    let mut batches = Vec::new();

    // Batch size: 1-3 turns per batch
    let batch_size = 3;

    for chunk in turns.chunks(batch_size) {
        batches.push(TurnBatch {
            turns: chunk.to_vec(),
            session: session.clone(),
        });
    }

    batches
}

// ============================================================================
// Project Identity
// ============================================================================

/// Determine project identity from session_start.cwd
pub fn determine_project(cwd: &str) -> ProjectIdentity {
    // Extract the last component of the path as the project name
    let name = Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    ProjectIdentity {
        name,
        path: cwd.to_string(),
    }
}

// ============================================================================
// Session File Discovery
// ============================================================================

/// Find all session files in the sessions directory
pub fn discover_session_files() -> Result<Vec<PathBuf>> {
    let sessions_dir = get_sessions_dir();

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut session_files = Vec::new();

    // Iterate through subdirectories (each represents a cwd)
    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Look for .jsonl files in the subdirectory
            if let Ok(entries) = fs::read_dir(&path) {
                for file_entry in entries.flatten() {
                    let file_path = file_entry.path();
                    if file_path.extension().is_some_and(|ext| ext == "jsonl") {
                        session_files.push(file_path);
                    }
                }
            }
        }
    }

    Ok(session_files)
}

/// Find session file for a specific project (by cwd)
pub fn find_session_for_project(cwd: &str) -> Result<Option<PathBuf>> {
    let sessions_dir = get_sessions_dir();
    let dir_name = cwd_to_dir_name(cwd);
    let project_dir = sessions_dir.join(&dir_name);

    if !project_dir.exists() {
        return Ok(None);
    }

    // Find the .jsonl file in the project directory
    // Usually there's only one per session
    let mut session_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                session_files.push(path);
            }
        }
    }

    // Return the most recently modified file
    session_files.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    Ok(session_files.into_iter().last())
}

// ============================================================================
// Full Pipeline
// ============================================================================

/// Process a session file and return batches of turns
pub fn process_session_file<P: AsRef<Path>>(path: P) -> Result<Vec<TurnBatch>> {
    let lines = parse_file(&path)?;
    let mut session: Option<ParsedSession> = None;

    // Extract session info from session_start line
    for line in &lines {
        if let ParsedLine::SessionStart(s) = line {
            session = Some(s.clone());
            break;
        }
    }

    let session = session.context("No session_start line found in file")?;

    // Group into turns
    let turns = group_into_turns(lines);

    // Batch the turns
    let batches = batch_turns(turns, session);

    Ok(batches)
}

/// Process a session file and return turns grouped by project
pub fn process_session_for_project(cwd: &str) -> Result<Option<Vec<TurnBatch>>> {
    let session_path = find_session_for_project(cwd)?;

    match session_path {
        Some(path) => {
            let batches = process_session_file(&path)?;
            Ok(Some(batches))
        }
        None => Ok(None),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_start_line() {
        let line = r#"{"type":"session_start","id":"abc123","title":"Test Session","cwd":"/Users/kg/project","owner":"kg","version":2}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::SessionStart(s) => {
                assert_eq!(s.id, "abc123");
                assert_eq!(s.cwd, "/Users/kg/project");
                assert_eq!(s.title, "Test Session");
                assert_eq!(s.owner, "kg");
            }
            _ => panic!("Expected SessionStart"),
        }
    }

    #[test]
    fn test_parse_session_start_with_session_title() {
        // Real Droid sessions have both title and sessionTitle
        // title is usually the first message or truncated version
        // sessionTitle is the full descriptive title
        let line = r#"{"type":"session_start","id":"abc123","title":"First message...","sessionTitle":"Pull latest commits and check repository history","cwd":"/Users/kg/project","owner":"kg","version":2}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::SessionStart(s) => {
                // title takes precedence when present
                assert_eq!(s.title, "First message...");
            }
            _ => panic!("Expected SessionStart"),
        }
    }

    #[test]
    fn test_parse_session_start_with_only_session_title() {
        // When title is absent, sessionTitle should be used
        let line = r#"{"type":"session_start","id":"abc123","sessionTitle":"Alt Title","cwd":"/Users/kg/project","owner":"kg","version":2}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::SessionStart(s) => {
                // Should fall back to sessionTitle when title is absent
                assert_eq!(s.title, "Alt Title");
            }
            _ => panic!("Expected SessionStart"),
        }
    }

    #[test]
    fn test_parse_message_line_text_only() {
        let line = r#"{"type":"message","id":"msg1","timestamp":"2026-05-06T12:58:28.532Z","message":{"role":"user","content":[{"type":"text","text":"Hello world"}]},"parentId":"ctx1"}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::Message(m) => {
                assert_eq!(m.id, "msg1");
                assert_eq!(m.role, "user");
                assert_eq!(m.text, "Hello world");
                assert_eq!(m.timestamp, "2026-05-06T12:58:28.532Z");
                assert_eq!(m.parent_id, Some("ctx1".to_string()));
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_line_with_thinking_skipped() {
        let line = r#"{"type":"message","id":"msg2","timestamp":"2026-05-06T12:58:36.318Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me think..."},{"type":"text","text":"Final answer"}]},"parentId":"msg1"}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::Message(m) => {
                assert_eq!(m.role, "assistant");
                assert_eq!(m.text, "Final answer"); // Thinking should be skipped
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_line_with_tool_use_skipped() {
        let line = r#"{"type":"message","id":"msg3","timestamp":"2026-05-06T12:58:40.170Z","message":{"role":"user","content":[{"type":"tool_use","id":"tool1","name":"bash","input":{"command":"ls"}},{"type":"text","text":"Tool result"}]},"parentId":"msg2"}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::Message(m) => {
                assert_eq!(m.role, "user");
                assert_eq!(m.text, "Tool result"); // tool_use should be skipped
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_line_multiple_text_blocks() {
        let line = r#"{"type":"message","id":"msg4","timestamp":"2026-05-06T12:58:45.812Z","message":{"role":"assistant","content":[{"type":"text","text":"Part 1"},{"type":"text","text":"Part 2"},{"type":"text","text":"Part 3"}]},"parentId":"msg3"}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::Message(m) => {
                assert_eq!(m.text, "Part 1\nPart 2\nPart 3");
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_todo_state_line() {
        let line = r#"{"type":"todo_state","id":"todo1","todos":[{"status":"in_progress","content":"Task 1"},{"status":"done","content":"Task 2"}]}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::TodoState { id, todos } => {
                assert_eq!(id, "todo1");
                assert_eq!(todos.len(), 2);
                assert_eq!(todos[0].status, "in_progress");
                assert_eq!(todos[1].content, "Task 2");
            }
            _ => panic!("Expected TodoState"),
        }
    }

    #[test]
    fn test_parse_session_end_line() {
        let line = r#"{"type":"session_end","id":"sess1","reason":"completed"}"#;
        let result = parse_line(line).unwrap();

        match result {
            ParsedLine::SessionEnd { id, reason } => {
                assert_eq!(id, "sess1");
                assert_eq!(reason, Some("completed".to_string()));
            }
            _ => panic!("Expected SessionEnd"),
        }
    }

    #[test]
    fn test_group_into_turns() {
        let lines = vec![
            ParsedLine::Message(ParsedMessage {
                id: "msg1".to_string(),
                role: "user".to_string(),
                text: "Hello".to_string(),
                timestamp: "2026-05-06T12:00:00Z".to_string(),
                parent_id: None,
            }),
            ParsedLine::Message(ParsedMessage {
                id: "msg2".to_string(),
                role: "assistant".to_string(),
                text: "Hi there!".to_string(),
                timestamp: "2026-05-06T12:00:01Z".to_string(),
                parent_id: Some("msg1".to_string()),
            }),
            ParsedLine::Message(ParsedMessage {
                id: "msg3".to_string(),
                role: "user".to_string(),
                text: "How are you?".to_string(),
                timestamp: "2026-05-06T12:00:02Z".to_string(),
                parent_id: Some("msg2".to_string()),
            }),
        ];

        let turns = group_into_turns(lines);

        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user_message.text, "Hello");
        assert_eq!(
            turns[0].assistant_message.as_ref().unwrap().text,
            "Hi there!"
        );
        assert_eq!(turns[1].user_message.text, "How are you?");
        assert!(turns[1].assistant_message.is_none());
    }

    #[test]
    fn test_batch_turns() {
        let session = ParsedSession {
            id: "sess1".to_string(),
            cwd: "/Users/kg/project".to_string(),
            title: "Test".to_string(),
            owner: "kg".to_string(),
        };

        let turns = vec![
            Turn {
                user_message: ParsedMessage {
                    id: "msg1".to_string(),
                    role: "user".to_string(),
                    text: "Turn 1".to_string(),
                    timestamp: "2026-05-06T12:00:00Z".to_string(),
                    parent_id: None,
                },
                assistant_message: Some(ParsedMessage {
                    id: "msg2".to_string(),
                    role: "assistant".to_string(),
                    text: "Response 1".to_string(),
                    timestamp: "2026-05-06T12:00:01Z".to_string(),
                    parent_id: Some("msg1".to_string()),
                }),
            },
            Turn {
                user_message: ParsedMessage {
                    id: "msg3".to_string(),
                    role: "user".to_string(),
                    text: "Turn 2".to_string(),
                    timestamp: "2026-05-06T12:00:02Z".to_string(),
                    parent_id: Some("msg2".to_string()),
                },
                assistant_message: Some(ParsedMessage {
                    id: "msg4".to_string(),
                    role: "assistant".to_string(),
                    text: "Response 2".to_string(),
                    timestamp: "2026-05-06T12:00:03Z".to_string(),
                    parent_id: Some("msg3".to_string()),
                }),
            },
            Turn {
                user_message: ParsedMessage {
                    id: "msg5".to_string(),
                    role: "user".to_string(),
                    text: "Turn 3".to_string(),
                    timestamp: "2026-05-06T12:00:04Z".to_string(),
                    parent_id: Some("msg4".to_string()),
                },
                assistant_message: Some(ParsedMessage {
                    id: "msg6".to_string(),
                    role: "assistant".to_string(),
                    text: "Response 3".to_string(),
                    timestamp: "2026-05-06T12:00:05Z".to_string(),
                    parent_id: Some("msg5".to_string()),
                }),
            },
            Turn {
                user_message: ParsedMessage {
                    id: "msg7".to_string(),
                    role: "user".to_string(),
                    text: "Turn 4".to_string(),
                    timestamp: "2026-05-06T12:00:06Z".to_string(),
                    parent_id: Some("msg6".to_string()),
                },
                assistant_message: None,
            },
        ];

        let batches = batch_turns(turns, session.clone());

        // 4 turns with batch size 3 = 2 batches (3 turns + 1 turn)
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].turns.len(), 3);
        assert_eq!(batches[1].turns.len(), 1);
    }

    #[test]
    fn test_cwd_to_dir_name() {
        assert_eq!(
            cwd_to_dir_name("/Users/kg/Desktop/Work"),
            "-Users-kg-Desktop-Work"
        );
        assert_eq!(cwd_to_dir_name("/Users/kg/project"), "-Users-kg-project");
    }

    #[test]
    fn test_dir_name_to_cwd() {
        assert_eq!(
            dir_name_to_cwd("-Users-kg-Desktop-Work"),
            "/Users/kg/Desktop/Work"
        );
        assert_eq!(dir_name_to_cwd("-Users-kg-project"), "/Users/kg/project");
    }

    #[test]
    fn test_determine_project() {
        let project = determine_project("/Users/kg/Desktop/Work/thelaunch.space/projects/myapp");

        assert_eq!(project.name, "myapp");
        assert_eq!(
            project.path,
            "/Users/kg/Desktop/Work/thelaunch.space/projects/myapp"
        );
    }

    #[test]
    fn test_determine_project_from_root() {
        let project = determine_project("/Users/kg");

        assert_eq!(project.name, "kg");
    }

    #[test]
    fn test_parse_empty_line() {
        let result = parse_line("");
        assert!(result.is_ok());
        match result.unwrap() {
            ParsedLine::Unknown(_) => {}
            _ => panic!("Expected Unknown for empty line"),
        }
    }

    #[test]
    fn test_parse_whitespace_line() {
        let result = parse_line("   ");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_real_session_file() {
        // VAL-PARSE-004: Parser handles real Droid session files from ~/.factory/sessions/
        // Use an actual session file for integration testing
        // This session file has both session_start and message lines
        let session_path = dirs::home_dir()
            .unwrap()
            .join(".factory")
            .join("sessions")
            .join("-Users-kg-Desktop-Work-thelaunch.space-project-folders-active-the-launch-space-projects-your-agents-business-mode")
            .join("76551339-5129-4857-b096-db0ecfe5326b.jsonl");

        // Skip test if session file doesn't exist (may not exist on all systems)
        if !session_path.exists() {
            println!(
                "Skipping real session file test - file not found: {:?}",
                session_path
            );
            return;
        }

        // Parse the file
        let lines = parse_file(&session_path).expect("Should parse session file");

        // Verify we got session_start and at least some messages
        assert!(!lines.is_empty(), "Should have parsed some lines");

        // Check that we have a session_start line
        let has_session_start = lines
            .iter()
            .any(|l| matches!(l, ParsedLine::SessionStart(_)));
        assert!(has_session_start, "Should have a session_start line");

        // Verify session info was extracted correctly from session_start
        for line in &lines {
            if let ParsedLine::SessionStart(s) = line {
                assert_eq!(s.id, "76551339-5129-4857-b096-db0ecfe5326b");
                assert!(!s.cwd.is_empty(), "cwd should not be empty");
                assert_eq!(s.owner, "kg");
                break;
            }
        }

        // Check that we have message lines
        let message_count = lines
            .iter()
            .filter(|l| matches!(l, ParsedLine::Message(_)))
            .count();
        assert!(message_count > 0, "Should have parsed some message lines");

        // Group into turns and verify we get some turns
        let turns = group_into_turns(lines);
        assert!(!turns.is_empty(), "Should have produced some turns");

        // Verify turns have user messages
        for turn in &turns {
            assert!(
                !turn.user_message.text.is_empty() || turn.assistant_message.is_some(),
                "Each turn should have either a user message or an assistant message"
            );
        }
    }

    #[test]
    fn test_process_session_file_integration() {
        // Integration test: process_session_file should return batches
        let session_path = dirs::home_dir()
            .unwrap()
            .join(".factory")
            .join("sessions")
            .join("-Users-kg-Desktop-Work-thelaunch.space-project-folders-active-the-launch-space-projects-your-agents-business-mode")
            .join("76551339-5129-4857-b096-db0ecfe5326b.jsonl");

        if !session_path.exists() {
            println!("Skipping process_session_file test - file not found");
            return;
        }

        let result = process_session_file(&session_path);
        assert!(result.is_ok(), "process_session_file should succeed");

        let batches = result.expect("Should get batches");
        assert!(!batches.is_empty(), "Should produce some batches");

        // Verify batch structure
        for batch in &batches {
            assert!(!batch.turns.is_empty(), "Each batch should have turns");
            assert!(
                !batch.session.id.is_empty(),
                "Batch should have session info"
            );
        }
    }

    #[test]
    fn test_discover_session_files() {
        // Test that we can discover session files in the sessions directory
        let sessions_dir = get_sessions_dir();

        if !sessions_dir.exists() {
            println!("Skipping discover_session_files test - sessions dir not found");
            return;
        }

        let files = discover_session_files().expect("Should discover session files");

        // Should have found at least some session files
        assert!(!files.is_empty(), "Should discover some session files");

        // Each file should be a valid path with .jsonl extension
        for file in &files {
            assert!(
                file.extension().map_or(false, |ext| ext == "jsonl"),
                "Discovered files should be .jsonl files"
            );
        }
    }
}
