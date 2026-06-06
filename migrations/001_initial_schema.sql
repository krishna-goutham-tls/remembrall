-- Migration 001: Initial Schema - Core Tables
-- Creates all 7 tables: projects, memory_types, memories, memory_vectors,
-- memory_access_log, archived_memories, active_sessions

PRAGMA user_version = 1;

-- Projects table
CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT,
    git_root TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(name, path)
);

-- Memory types table (will be seeded separately)
CREATE TABLE IF NOT EXISTS memory_types (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    family TEXT NOT NULL CHECK(family IN ('durable', 'operational', 'ephemeral')),
    decay_band TEXT NOT NULL CHECK(decay_band IN ('slow', 'mid', 'fast')),
    base_lambda REAL NOT NULL,
    priority_weight REAL NOT NULL CHECK(priority_weight >= 0.5 AND priority_weight <= 1.0)
);

-- Memories table (core memory storage)
CREATE TABLE IF NOT EXISTS memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER REFERENCES projects(id),
    type_id INTEGER REFERENCES memory_types(id),
    source_tool TEXT NOT NULL CHECK(source_tool IN ('droid', 'codex', 'claude', 'cursor')),
    summary_text TEXT NOT NULL,
    raw_snippet TEXT,
    keywords TEXT,
    scope TEXT NOT NULL DEFAULT 'project' CHECK(scope IN ('global', 'project')),
    importance REAL NOT NULL DEFAULT 0.5 CHECK(importance >= 0.0 AND importance <= 1.0),
    strength REAL NOT NULL DEFAULT 1.0 CHECK(strength >= 0.0),
    recall_count INTEGER NOT NULL DEFAULT 0,
    last_accessed DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0, 1))
);

-- Memory vectors table (768-dim embeddings via sqlite-vec)
CREATE TABLE IF NOT EXISTS memory_vectors (
    memory_id INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    project_id INTEGER REFERENCES projects(id),
    embedding BLOB
);

-- Memory access log for tracking retrievals
CREATE TABLE IF NOT EXISTS memory_access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id INTEGER REFERENCES memories(id) ON DELETE CASCADE,
    accessed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    context TEXT,
    source_tool TEXT
);

-- Archived memories (for decay threshold, superseded, manual delete)
CREATE TABLE IF NOT EXISTS archived_memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    original_memory_id INTEGER,
    archived_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    final_strength REAL,
    archive_reason TEXT CHECK(archive_reason IN ('decay_threshold', 'superseded', 'manual_delete'))
);

-- Active sessions for file-to-project correlation
CREATE TABLE IF NOT EXISTS active_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER REFERENCES projects(id),
    session_file TEXT NOT NULL,
    last_modified DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_id, session_file)
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_memories_type_id ON memories(type_id);
CREATE INDEX IF NOT EXISTS idx_memories_is_active ON memories(is_active);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memory_vectors_project_id ON memory_vectors(project_id);
CREATE INDEX IF NOT EXISTS idx_memory_access_log_memory_id ON memory_access_log(memory_id);
CREATE INDEX IF NOT EXISTS idx_active_sessions_project_id ON active_sessions(project_id);
