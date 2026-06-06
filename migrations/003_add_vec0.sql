-- Migration 003: Add sqlite-vec Virtual Table
-- Creates vector search table for 768-dimensional embeddings with cosine distance

PRAGMA user_version = 3;

-- sqlite-vec virtual table for vector similarity search
-- 768 dimensions (bge-base-en-v1.5 model output)
-- Partitioned by project_id for scoped search
CREATE VIRTUAL TABLE IF NOT EXISTS vec_memory_embeddings USING vec0(
    memory_id INTEGER PRIMARY KEY,
    project_id INTEGER PARTITION KEY,
    summary_embedding float[768] distance_metric=cosine
);
