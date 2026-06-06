// Ebbinghaus exponential decay engine
// Implements memory strength decay with configurable bands, archive threshold,
// daily sweep scheduling, reinforcement, and supersede-and-fade.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::db::schema::get_data_dir;

/// Lambda values for each decay band (per Ebbinghaus formula)
pub const LAMBDA_SLOW: f64 = 0.03; // ~23 day half-life
pub const LAMBDA_MID: f64 = 0.05; // ~14 day half-life
pub const LAMBDA_FAST: f64 = 0.13; // ~5 day half-life

/// Archive threshold - memories with strength below this are archived
pub const ARCHIVE_THRESHOLD: f64 = 0.01;

/// Maximum reinforcement multiplier (3x boost from recall_count)
pub const MAX_REINFORCEMENT_MULTIPLIER: f64 = 3.0;

/// Reinforcement boost per recall (15% per recall)
pub const RECALL_BOOST_PER_COUNT: f64 = 0.15;

/// Sweep interval for daily decay check (24 hours)
pub const SWEEP_INTERVAL_HOURS: i64 = 24;

/// Time of day for scheduled daily sweep (3 AM)
pub const SWEEP_HOUR: u32 = 3;

/// Decay band enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecayBand {
    Slow,
    Mid,
    Fast,
}

impl DecayBand {
    /// Get lambda value for this decay band
    pub fn lambda(&self) -> f64 {
        match self {
            DecayBand::Slow => LAMBDA_SLOW,
            DecayBand::Mid => LAMBDA_MID,
            DecayBand::Fast => LAMBDA_FAST,
        }
    }

    /// Parse decay band from database string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "slow" => Some(DecayBand::Slow),
            "mid" => Some(DecayBand::Mid),
            "fast" => Some(DecayBand::Fast),
            _ => None,
        }
    }
}

/// Memory decay state for computation
#[derive(Debug, Clone)]
pub struct DecayState {
    pub importance: f64,
    pub decay_band: DecayBand,
    pub recall_count: i32,
    pub days_since_access: f64,
}

impl DecayState {
    /// Create new decay state from memory data
    pub fn new(
        importance: f64,
        decay_band: DecayBand,
        recall_count: i32,
        last_accessed: Option<DateTime<Utc>>,
    ) -> Self {
        let days_since_access = Self::compute_days_since(last_accessed);

        Self {
            importance,
            decay_band,
            recall_count,
            days_since_access,
        }
    }

    /// Compute days since last access (0 if never accessed)
    fn compute_days_since(last_accessed: Option<DateTime<Utc>>) -> f64 {
        match last_accessed {
            Some(dt) => {
                let now = Utc::now();
                let duration = now.signed_duration_since(dt);
                duration.num_seconds() as f64 / (24.0 * 60.0 * 60.0)
            }
            None => 0.0,
        }
    }

    /// Compute current strength using Ebbinghaus formula
    /// strength = importance * exp(-lambda * days) * min(1 + recall_count * 0.15, 3.0)
    pub fn compute_strength(&self) -> f64 {
        let lambda = self.decay_band.lambda();
        let exponential_decay = (-lambda * self.days_since_access).exp();

        let reinforcement_multiplier = (1.0 + self.recall_count as f64 * RECALL_BOOST_PER_COUNT)
            .min(MAX_REINFORCEMENT_MULTIPLIER);

        self.importance * exponential_decay * reinforcement_multiplier
    }
}

/// Result of a decay sweep operation
#[derive(Debug, Serialize, Deserialize)]
pub struct DecaySweepResult {
    pub memories_processed: u32,
    pub memories_archived: u32,
    pub memories_reinforced: u32,
    pub next_sweep_time: DateTime<Local>,
}

/// Metadata about the last decay sweep
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepMetadata {
    pub last_sweep_at: DateTime<Local>,
    pub next_scheduled: DateTime<Local>,
}

impl SweepMetadata {
    /// Load sweep metadata from file
    pub fn load() -> Result<Self> {
        let path = Self::get_metadata_path()?;

        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content).context("Failed to parse sweep metadata")
        } else {
            // Return default metadata (never run, next is 3 AM today/tomorrow)
            Ok(Self::default())
        }
    }

    /// Save sweep metadata to file
    pub fn save(&self) -> Result<()> {
        let path = Self::get_metadata_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create metadata directory")?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content).context("Failed to write sweep metadata")?;

        Ok(())
    }

    /// Get path to sweep metadata file
    fn get_metadata_path() -> Result<PathBuf> {
        let base = get_data_dir().context("Failed to get data directory")?;
        Ok(base.join("decay_sweep_meta.json"))
    }

    /// Calculate next scheduled sweep time (3 AM tomorrow or today if before 3 AM)
    pub fn calculate_next_sweep_time() -> DateTime<Local> {
        let now = Local::now();
        let today_3am = now.date_naive().and_hms_opt(SWEEP_HOUR, 0, 0).unwrap();

        let next_sweep = if now.naive_local() < today_3am {
            // Before 3 AM today, schedule for today
            DateTime::<Local>::from_naive_utc_and_offset(today_3am, *now.offset())
        } else {
            // After 3 AM today, schedule for tomorrow
            let tomorrow = (now.date_naive() + chrono::Duration::days(1))
                .and_hms_opt(SWEEP_HOUR, 0, 0)
                .unwrap();
            DateTime::<Local>::from_naive_utc_and_offset(tomorrow, *now.offset())
        };

        next_sweep
    }
}

impl Default for SweepMetadata {
    fn default() -> Self {
        let now = Local::now();
        let last_sweep = now - Duration::hours(SWEEP_INTERVAL_HOURS + 1); // Force first sweep
        Self {
            last_sweep_at: last_sweep,
            next_scheduled: SweepMetadata::calculate_next_sweep_time(),
        }
    }
}

/// Check if a sweep should run (3 AM daily or on launch if >24h since last)
pub fn should_run_sweep() -> Result<bool> {
    let metadata = SweepMetadata::load()?;

    let now = Local::now();
    let hours_since_last = now
        .signed_duration_since(metadata.last_sweep_at)
        .num_hours();

    // Run if: more than 24 hours since last sweep OR it's past 3 AM and before next scheduled
    let overdue = hours_since_last > SWEEP_INTERVAL_HOURS;
    let scheduled_due = now >= metadata.next_scheduled;

    Ok(overdue || scheduled_due)
}

/// Run the daily decay sweep
/// - Recompute strength for all active memories
/// - Archive memories below threshold
/// - Update last_accessed times
pub fn run_decay_sweep(conn: &Connection) -> Result<DecaySweepResult> {
    log::info!("Starting decay sweep");

    let mut result = DecaySweepResult {
        memories_processed: 0,
        memories_archived: 0,
        memories_reinforced: 0,
        next_sweep_time: SweepMetadata::calculate_next_sweep_time(),
    };

    // Get all active memories with their type info
    let mut stmt = conn.prepare(
        r#"
        SELECT m.id, m.importance, m.strength, m.recall_count, m.last_accessed,
               mt.decay_band
        FROM memories m
        JOIN memory_types mt ON m.type_id = mt.id
        WHERE m.is_active = 1
        "#,
    )?;

    let memories: Vec<(i64, f64, f64, i32, Option<String>, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    drop(stmt);

    log::info!("Found {} active memories to process", memories.len());

    for (id, importance, _old_strength, recall_count, last_accessed, decay_band_str) in memories {
        result.memories_processed += 1;

        let decay_band = DecayBand::parse(&decay_band_str).unwrap_or(DecayBand::Mid);

        // Parse last_accessed
        let last_accessed_dt = last_accessed.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        // Compute new decay state
        let state = DecayState::new(importance, decay_band, recall_count, last_accessed_dt);
        let new_strength = state.compute_strength();

        log::debug!(
            "Memory {}: importance={}, band={:?}, recall={}, days={:.2}, strength={:.4} -> {:.4}",
            id,
            importance,
            decay_band,
            recall_count,
            state.days_since_access,
            _old_strength,
            new_strength
        );

        if new_strength < ARCHIVE_THRESHOLD {
            // Archive the memory
            archive_memory(conn, id, new_strength, "decay_threshold")?;
            result.memories_archived += 1;
            log::info!(
                "Archived memory {} (strength {:.6} < {})",
                id,
                new_strength,
                ARCHIVE_THRESHOLD
            );
        } else {
            // Update strength and last_accessed
            conn.execute(
                "UPDATE memories SET strength = ?, last_accessed = ? WHERE id = ?",
                params![new_strength, Utc::now().to_rfc3339(), id],
            )?;
        }
    }

    // Update sweep metadata
    let metadata = SweepMetadata {
        last_sweep_at: Local::now(),
        next_scheduled: result.next_sweep_time,
    };
    metadata.save()?;

    log::info!(
        "Decay sweep complete: processed={}, archived={}, next={}",
        result.memories_processed,
        result.memories_archived,
        result.next_sweep_time.format("%Y-%m-%d %H:%M")
    );

    Ok(result)
}

/// Archive a memory with the given reason
fn archive_memory(
    conn: &Connection,
    memory_id: i64,
    final_strength: f64,
    reason: &str,
) -> Result<()> {
    // Insert into archived_memories
    conn.execute(
        "INSERT INTO archived_memories (original_memory_id, final_strength, archive_reason) VALUES (?, ?, ?)",
        params![memory_id, final_strength, reason],
    )?;

    // Mark as inactive in memories table
    conn.execute(
        "UPDATE memories SET is_active = 0 WHERE id = ?",
        params![memory_id],
    )?;

    Ok(())
}

/// Reinforce a memory (called on recall)
/// Increments recall_count and boosts strength
pub fn reinforce_memory(conn: &Connection, memory_id: i64) -> Result<f64> {
    log::info!("Reinforcing memory {}", memory_id);

    // Get current state
    let (importance, recall_count, decay_band_str): (f64, i32, String) = conn
        .query_row(
            r#"
        SELECT m.importance, m.recall_count, mt.decay_band
        FROM memories m
        JOIN memory_types mt ON m.type_id = mt.id
        WHERE m.id = ? AND m.is_active = 1
        "#,
            params![memory_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("Memory not found or inactive")?;

    let decay_band = DecayBand::parse(&decay_band_str).unwrap_or(DecayBand::Mid);

    // Increment recall count
    let new_recall_count = recall_count + 1;

    // Get last accessed
    let last_accessed_str: Option<String> = conn.query_row(
        "SELECT last_accessed FROM memories WHERE id = ?",
        params![memory_id],
        |row| row.get(0),
    )?;

    let last_accessed_dt = last_accessed_str.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    // Compute new strength with updated recall count
    let state = DecayState::new(importance, decay_band, new_recall_count, last_accessed_dt);
    let new_strength = state.compute_strength();

    // Update database
    conn.execute(
        "UPDATE memories SET recall_count = ?, strength = ?, last_accessed = ? WHERE id = ?",
        params![
            new_recall_count,
            new_strength,
            Utc::now().to_rfc3339(),
            memory_id
        ],
    )?;

    // Log the access
    conn.execute(
        "INSERT INTO memory_access_log (memory_id, source_tool, context) VALUES (?, 'droid', 'reinforcement')",
        params![memory_id],
    )?;

    log::info!(
        "Memory {} reinforced: recall_count {} -> {}, new_strength={:.4}",
        memory_id,
        recall_count,
        new_recall_count,
        new_strength
    );

    Ok(new_strength)
}

/// Supersede a memory (new conflicting memory detected)
/// Drops old memory's strength significantly
pub fn supersede_memory(conn: &Connection, memory_id: i64) -> Result<()> {
    log::info!("Superseding memory {}", memory_id);

    // Get current strength
    let current_strength: f64 = conn
        .query_row(
            "SELECT strength FROM memories WHERE id = ? AND is_active = 1",
            params![memory_id],
            |row| row.get(0),
        )
        .context("Memory not found or inactive")?;

    // Apply supersede penalty: reduce strength to 10% of current
    let new_strength = current_strength * 0.1;

    if new_strength < ARCHIVE_THRESHOLD {
        // Archive immediately if below threshold
        archive_memory(conn, memory_id, new_strength, "superseded")?;
        log::info!(
            "Memory {} superseded and archived (strength {:.6} < {})",
            memory_id,
            new_strength,
            ARCHIVE_THRESHOLD
        );
    } else {
        // Update strength but keep active
        conn.execute(
            "UPDATE memories SET strength = ? WHERE id = ?",
            params![new_strength, memory_id],
        )?;
        log::info!(
            "Memory {} superseded, strength reduced to {:.6}",
            memory_id,
            new_strength
        );
    }

    Ok(())
}

/// Find conflicting memories based on keywords or content similarity
/// Returns memory IDs that should be superseded
pub fn find_conflicting_memories(
    conn: &Connection,
    project_id: Option<i64>,
    new_keywords: &str,
) -> Result<Vec<i64>> {
    let keywords: Vec<&str> = new_keywords.split(',').map(|s| s.trim()).collect();
    let mut conflicts = Vec::new();

    for keyword in keywords {
        if keyword.len() < 3 {
            continue; // Skip very short keywords
        }

        // Find memories with similar keywords (using LIKE for simplicity)
        let mut stmt = conn.prepare(
            r#"
            SELECT id FROM memories
            WHERE is_active = 1
            AND keywords LIKE ?
            AND (project_id = ? OR project_id IS NULL)
            "#,
        )?;

        let pattern = format!("%{}%", keyword);
        let rows = stmt.query_map(params![pattern, project_id], |row| row.get(0))?;

        for id in rows.flatten() {
            if !conflicts.contains(&id) {
                conflicts.push(id);
            }
        }
    }

    Ok(conflicts)
}

/// Process new memory with supersede-and-fade logic
/// Returns IDs of memories that were superseded
pub fn process_new_memory_with_supersede(
    conn: &Connection,
    memory_id: i64,
    keywords: &str,
) -> Result<Vec<i64>> {
    let project_id: Option<i64> = conn.query_row(
        "SELECT project_id FROM memories WHERE id = ?",
        params![memory_id],
        |row| row.get(0),
    )?;

    // Find conflicting memories
    let conflicts = find_conflicting_memories(conn, project_id, keywords)?;

    log::info!(
        "New memory {} has {} conflicting memories",
        memory_id,
        conflicts.len()
    );

    // Supersede each conflicting memory (except the new one)
    let mut superseded = Vec::new();
    for conflict_id in conflicts {
        if conflict_id != memory_id {
            supersede_memory(conn, conflict_id)?;
            superseded.push(conflict_id);
        }
    }

    Ok(superseded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn create_test_db() -> (TempDir, Connection) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Initialize with schema
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Create minimal schema for testing
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memory_types (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                family TEXT NOT NULL,
                decay_band TEXT NOT NULL,
                base_lambda REAL NOT NULL,
                priority_weight REAL NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER,
                type_id INTEGER REFERENCES memory_types(id),
                source_tool TEXT NOT NULL,
                summary_text TEXT NOT NULL,
                keywords TEXT,
                scope TEXT NOT NULL DEFAULT 'project',
                importance REAL NOT NULL DEFAULT 0.5,
                strength REAL NOT NULL DEFAULT 1.0,
                recall_count INTEGER NOT NULL DEFAULT 0,
                last_accessed DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                is_active INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS archived_memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                original_memory_id INTEGER,
                archived_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                final_strength REAL,
                archive_reason TEXT
            );

            CREATE TABLE IF NOT EXISTS memory_access_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id INTEGER REFERENCES memories(id),
                accessed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                context TEXT,
                source_tool TEXT
            );
            "#,
        )
        .unwrap();

        // Insert test memory types
        conn.execute(
            "INSERT INTO memory_types (id, name, family, decay_band, base_lambda, priority_weight) VALUES (1, 'test_slow', 'durable', 'slow', 0.03, 1.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_types (id, name, family, decay_band, base_lambda, priority_weight) VALUES (2, 'test_mid', 'operational', 'mid', 0.05, 0.9)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_types (id, name, family, decay_band, base_lambda, priority_weight) VALUES (3, 'test_fast', 'ephemeral', 'fast', 0.13, 0.5)",
            [],
        )
        .unwrap();

        (temp_dir, conn)
    }

    #[test]
    fn test_decay_band_lambda() {
        assert!((DecayBand::Slow.lambda() - 0.03).abs() < 1e-10);
        assert!((DecayBand::Mid.lambda() - 0.05).abs() < 1e-10);
        assert!((DecayBand::Fast.lambda() - 0.13).abs() < 1e-10);
    }

    #[test]
    fn test_strength_formula_slow_band() {
        // slow band (lambda=0.03): 0.5 importance, 0 days → 0.5
        let state = DecayState::new(0.5, DecayBand::Slow, 0, None);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.5).abs() < 0.001,
            "Expected ~0.5, got {}",
            strength
        );

        // slow band: 0.5 importance, 23 days → ~0.25
        let state = DecayState::new_with_days(0.5, DecayBand::Slow, 0, 23.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.25).abs() < 0.02,
            "Expected ~0.25, got {}",
            strength
        );

        // slow band: 0.5 importance, 69 days → ~0.06
        let state = DecayState::new_with_days(0.5, DecayBand::Slow, 0, 69.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.06).abs() < 0.01,
            "Expected ~0.06, got {}",
            strength
        );
    }

    #[test]
    fn test_strength_formula_mid_band() {
        // mid band (lambda=0.05): 0.5 importance, 0 days → 0.5
        let state = DecayState::new(0.5, DecayBand::Mid, 0, None);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.5).abs() < 0.001,
            "Expected ~0.5, got {}",
            strength
        );

        // mid band: 0.5 importance, 14 days → ~0.25
        let state = DecayState::new_with_days(0.5, DecayBand::Mid, 0, 14.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.25).abs() < 0.02,
            "Expected ~0.25, got {}",
            strength
        );

        // mid band: 0.5 importance, 46 days → ~0.05
        let state = DecayState::new_with_days(0.5, DecayBand::Mid, 0, 46.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.05).abs() < 0.01,
            "Expected ~0.05, got {}",
            strength
        );
    }

    #[test]
    fn test_strength_formula_fast_band() {
        // fast band (lambda=0.13): 0.5 importance, 0 days → 0.5
        let state = DecayState::new(0.5, DecayBand::Fast, 0, None);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.5).abs() < 0.001,
            "Expected ~0.5, got {}",
            strength
        );

        // fast band: 0.5 importance, 5 days → ~0.26
        let state = DecayState::new_with_days(0.5, DecayBand::Fast, 0, 5.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.26).abs() < 0.02,
            "Expected ~0.26, got {}",
            strength
        );

        // fast band: 0.5 importance, 16 days → ~0.06
        let state = DecayState::new_with_days(0.5, DecayBand::Fast, 0, 16.0);
        let strength = state.compute_strength();
        assert!(
            (strength - 0.06).abs() < 0.01,
            "Expected ~0.06, got {}",
            strength
        );
    }

    #[test]
    fn test_reinforcement_boost() {
        // Recall count = 1 adds 15% boost, capped at 3x
        let state_no_recall = DecayState::new(0.5, DecayBand::Mid, 0, Some(Utc::now()));
        let state_with_recall = DecayState::new(0.5, DecayBand::Mid, 1, Some(Utc::now()));

        let strength_no_recall = state_no_recall.compute_strength();
        let strength_with_recall = state_with_recall.compute_strength();

        // 15% boost
        assert!(
            strength_with_recall > strength_no_recall,
            "Expected recall to boost strength"
        );
        assert!(
            (strength_with_recall / strength_no_recall - 1.15).abs() < 0.001,
            "Expected 15% boost, got {} vs {}",
            strength_with_recall / strength_no_recall,
            1.15
        );
    }

    #[test]
    fn test_reinforcement_cap_at_3x() {
        // recall_count very high should cap at 3x
        let state_high_recall = DecayState::new(0.5, DecayBand::Mid, 100, Some(Utc::now()));
        let state_no_recall = DecayState::new(0.5, DecayBand::Mid, 0, Some(Utc::now()));

        let strength_high = state_high_recall.compute_strength();
        let strength_no = state_no_recall.compute_strength();

        // Should be capped at 3x
        let ratio = strength_high / strength_no;
        assert!(
            (ratio - 3.0).abs() < 0.01,
            "Expected 3x cap, got {}x",
            ratio
        );
    }

    #[test]
    fn test_archive_threshold() {
        // Create a memory that will be below threshold
        let state = DecayState::new_with_days(0.5, DecayBand::Fast, 0, 50.0);
        let strength = state.compute_strength();

        assert!(
            strength < ARCHIVE_THRESHOLD,
            "Expected strength {} < {}",
            strength,
            ARCHIVE_THRESHOLD
        );
    }

    #[test]
    fn test_archive_threshold_check() {
        // Verify archive threshold constant
        assert!(ARCHIVE_THRESHOLD > 0.0 && ARCHIVE_THRESHOLD < 0.1);
        assert_eq!(ARCHIVE_THRESHOLD, 0.01);
    }

    #[test]
    fn test_sweep_metadata_default() {
        let metadata = SweepMetadata::default();

        // Should be overdue (last sweep > 24h ago)
        let now = Local::now();
        let hours_since = now
            .signed_duration_since(metadata.last_sweep_at)
            .num_hours();
        assert!(hours_since > SWEEP_INTERVAL_HOURS);

        // Next scheduled should be in the future
        assert!(metadata.next_scheduled > now);

        // Next scheduled should be within 25 hours (today or tomorrow at 3 AM)
        let hours_until = metadata
            .next_scheduled
            .signed_duration_since(now)
            .num_hours();
        assert!(
            hours_until <= 25,
            "Next sweep should be within 25 hours, was {} hours away",
            hours_until
        );
    }

    #[test]
    fn test_find_conflicting_memories() {
        // Test with a mock connection - this is more of an integration test
        // For unit testing, we test the keyword parsing logic
        let keywords = "rust,async, tokio";
        let parsed: Vec<&str> = keywords.split(',').map(|s| s.trim()).collect();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "rust");
        assert_eq!(parsed[1], "async");
        assert_eq!(parsed[2], "tokio");
    }

    // Helper to create DecayState with explicit days (for testing without dates)
    impl DecayState {
        fn new_with_days(
            importance: f64,
            decay_band: DecayBand,
            recall_count: i32,
            days: f64,
        ) -> Self {
            Self {
                importance,
                decay_band,
                recall_count,
                days_since_access: days,
            }
        }
    }
}
