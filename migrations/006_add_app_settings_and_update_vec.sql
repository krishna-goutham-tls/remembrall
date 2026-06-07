-- Migration 006: Update vec_embeddings dimension and add app_settings table
-- Changes:
--   1. Drop and recreate vec_memory_embeddings with float[384] (all-MiniLM-L6-v2)
--   2. Create app_settings table for key-value storage

-- Drop existing vec table (will be recreated with new dimension)
-- Using DROP TABLE IF EXISTS for idempotency
DROP TABLE IF EXISTS vec_memory_embeddings;

-- Recreate vec_memory_embeddings with 384 dimensions (all-MiniLM-L6-v2 model)
-- Note: No IF NOT EXISTS since we just dropped it
CREATE VIRTUAL TABLE vec_memory_embeddings USING vec0(
    memory_id INTEGER PRIMARY KEY,
    project_id INTEGER PARTITION KEY,
    summary_embedding float[384] distance_metric=cosine
);

-- Create app_settings table for application configuration storage
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Insert default settings
-- Model preference: use 4b classifier by default
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('model.classifier', '4b');
-- Droid tool enabled by default
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('tool.droid.enabled', 'true');
-- Indexing enabled by default
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('indexing.enabled', 'true');
-- Last decay sweep timestamp (null means never run)
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('decay.last_sweep', '');
