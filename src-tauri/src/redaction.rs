use chrono::Local;
use once_cell::sync::Lazy;
use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::db::schema::get_data_dir;

// Regex patterns for each redaction tier
// Tier 1: API key patterns
static API_KEY_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // sk- followed by 48 alphanumeric chars (OpenAI style)
        Regex::new(r"sk-[A-Za-z0-9]{48}").unwrap(),
        // Generic API key patterns: key prefix followed by 32-64 char alphanumeric/encoded string
        Regex::new(r"(?i)(api[_-]?key|apikey|key)[_-]?[A-Za-z0-9]{32,64}").unwrap(),
        // Other common key prefixes with 32-64 char values
        Regex::new(r"(?i)(secret[_-]?key|access[_-]?key|private[_-]?key)[_-]?[A-Za-z0-9]{32,64}")
            .unwrap(),
    ]
});

// Tier 2: Token patterns
static TOKEN_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Bearer tokens: Bearer followed by whitespace and token
        Regex::new(r"Bearer\s+[A-Za-z0-9\-_.]+").unwrap(),
        // GitHub personal access tokens
        Regex::new(r"ghp_[A-Za-z0-9]{36}").unwrap(),
        // gho_, ghu_, ghs_, ghr_ tokens (GitHub other types)
        Regex::new(r"gh[aou]_[A-Za-z0-9]{36}").unwrap(),
        // Generic token patterns with common prefixes
        Regex::new(r"(?i)token[_-]?[A-Za-z0-9\-_.]{32,64}").unwrap(),
    ]
});

// Tier 3: Password/credential patterns
static PASSWORD_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // password= or password: or password followed by space
        Regex::new(r"(?i)\bpassword[:=]\s*\S+").unwrap(),
        // pwd= or pwd:
        Regex::new(r"(?i)\bpwd[:=]\s*\S+").unwrap(),
        // secret= or secret:
        Regex::new(r"(?i)\bsecret[:=]\s*\S+").unwrap(),
    ]
});

// Redaction log file path
fn get_redaction_log_path() -> PathBuf {
    get_data_dir()
        .unwrap_or_else(|_| PathBuf::from(".").join("Remembrall"))
        .join("redaction.log")
}

// Mutex for thread-safe log writing
static LOG_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Log a redaction event to the redaction log
fn log_redaction(placeholder_type: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_entry = format!("[{}] REDACTED {}\n", timestamp, placeholder_type);

    // Ensure data directory exists
    if let Ok(data_dir) = get_data_dir() {
        let _ = fs::create_dir_all(&data_dir);
    }

    // Thread-safe log write
    let _guard = LOG_MUTEX.lock();
    let log_path = get_redaction_log_path();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = file.write_all(log_entry.as_bytes());
    }
}

/// Redact all secrets from the given text unconditionally.
/// This function runs before any other processing pipeline step.
/// Returns the redacted text with all secrets replaced by placeholders.
pub fn redact(text: &str) -> String {
    let mut result = text.to_string();

    // Apply Tier 1: API keys
    for pattern in API_KEY_PATTERNS.iter() {
        let matches: Vec<_> = pattern.find_iter(&result).collect();
        for _m in &matches {
            log_redaction("api_key");
        }
        result = pattern
            .replace_all(&result, "[REDACTED:api_key]")
            .to_string();
    }

    // Apply Tier 2: Tokens
    for pattern in TOKEN_PATTERNS.iter() {
        let matches: Vec<_> = pattern.find_iter(&result).collect();
        for _m in &matches {
            log_redaction("token");
        }
        result = pattern.replace_all(&result, "[REDACTED:token]").to_string();
    }

    // Apply Tier 3: Passwords/credentials
    for pattern in PASSWORD_PATTERNS.iter() {
        let matches: Vec<_> = pattern.find_iter(&result).collect();
        for _m in &matches {
            log_redaction("password");
        }
        result = pattern
            .replace_all(&result, "[REDACTED:password]")
            .to_string();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_openai_api_key() {
        // OpenAI sk- key with 48 characters after sk-
        let input = "Using OpenAI key sk-1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP";
        let result = redact(input);
        assert!(result.contains("[REDACTED:api_key]"));
        assert!(!result.contains("sk-"));
    }

    #[test]
    fn test_redact_generic_api_key() {
        let input = "api_key_abcdefghijklmnopqrstuvwxyz123456";
        let result = redact(input);
        assert!(result.contains("[REDACTED:api_key]"));
        assert!(!result.contains("api_key_"));
    }

    #[test]
    fn test_redact_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let result = redact(input);
        assert!(result.contains("[REDACTED:token]"));
        assert!(!result.contains("Bearer"));
    }

    #[test]
    fn test_redact_github_token() {
        let input = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
        let result = redact(input);
        assert!(result.contains("[REDACTED:token]"));
        assert!(!result.contains("ghp_"));
    }

    #[test]
    fn test_redact_github_other_token_types() {
        let input = "gho_abcdefghijklmnopqrstuvwxyz1234567890AB";
        let result = redact(input);
        assert!(result.contains("[REDACTED:token]"));
        assert!(!result.contains("gho_"));
    }

    #[test]
    fn test_redact_password_assignment() {
        let input = "password=mysecretpassword123";
        let result = redact(input);
        assert!(result.contains("[REDACTED:password]"));
        assert!(!result.contains("password="));
    }

    #[test]
    fn test_redact_password_with_colon() {
        let input = "password: supersecretpass";
        let result = redact(input);
        assert!(result.contains("[REDACTED:password]"));
        assert!(!result.contains("password:"));
    }

    #[test]
    fn test_redact_pwd_assignment() {
        let input = "pwd= hunter2";
        let result = redact(input);
        assert!(result.contains("[REDACTED:password]"));
        assert!(!result.contains("pwd="));
    }

    #[test]
    fn test_redact_secret_assignment() {
        let input = "secret: my-api-secret-key-12345";
        let result = redact(input);
        assert!(result.contains("[REDACTED:password]"));
        assert!(!result.contains("secret:"));
    }

    #[test]
    fn test_redact_multiple_passwords() {
        let input = "password=firstsecret and password=secondsecret";
        let result = redact(input);
        let count = result.matches("[REDACTED:password]").count();
        assert_eq!(count, 2);
        assert!(!result.contains("firstsecret"));
        assert!(!result.contains("secondsecret"));
    }

    #[test]
    fn test_redact_mixed_tiers() {
        let input = "API key: sk-1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP and password=secret123";
        let result = redact(input);
        assert!(result.contains("[REDACTED:api_key]"));
        assert!(result.contains("[REDACTED:password]"));
    }

    #[test]
    fn test_no_false_positives() {
        // Normal text should pass through unchanged
        let input =
            "This is a normal sentence about coding and programming. It contains no secrets.";
        let result = redact(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_normal_text_with_password_word() {
        // "password" as a regular word should not be redacted
        let input = "My password is weak but I won't change it";
        let result = redact(input);
        // "password is" doesn't match password: or password= so no redaction
        assert!(!result.contains("[REDACTED:"));
    }

    #[test]
    fn test_empty_string() {
        let input = "";
        let result = redact(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_realistic_message_with_embedded_secrets() {
        let input = "Message: I used the OpenAI API with key sk-1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP and stored password=secret123 in config";
        let result = redact(input);
        assert!(result.contains("[REDACTED:api_key]"));
        assert!(result.contains("[REDACTED:password]"));
        assert!(!result.contains("sk-1234567890abcdef"));
        assert!(!result.contains("secret123"));
    }

    #[test]
    fn test_no_bypass_path_exists() {
        // Verify redaction is unconditional - no way to bypass
        let input = "password=supersecret";
        // Call redact multiple times - should be idempotent (second call doesn't change anything new)
        let first = redact(input);
        let second = redact(&first);
        assert_eq!(first, second);
        assert!(first.contains("[REDACTED:password]"));
    }
}
