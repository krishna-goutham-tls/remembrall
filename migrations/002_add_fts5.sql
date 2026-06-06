-- Migration 002: Add FTS5 Virtual Table
-- Creates full-text search virtual table with sync triggers

PRAGMA user_version = 2;

-- FTS5 virtual table for full-text search on summary_text and keywords
-- Using porter stemming and unicode61 tokenizer
CREATE VIRTUAL TABLE IF NOT EXISTS fts_memories USING fts5(
    summary_text,
    keywords,
    content='memories',
    content_rowid='id',
    tokenize='porter unicode61'
);

-- Trigger to sync FTS5 on INSERT to memories
CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO fts_memories(rowid, summary_text, keywords)
    VALUES (new.id, new.summary_text, new.keywords);
END;

-- Trigger to sync FTS5 on DELETE from memories
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO fts_memories(fts_memories, rowid, summary_text, keywords)
    VALUES ('delete', old.id, old.summary_text, old.keywords);
END;

-- Trigger to sync FTS5 on UPDATE to memories
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO fts_memories(fts_memories, rowid, summary_text, keywords)
    VALUES ('delete', old.id, old.summary_text, old.keywords);
    INSERT INTO fts_memories(rowid, summary_text, keywords)
    VALUES (new.id, new.summary_text, new.keywords);
END;
