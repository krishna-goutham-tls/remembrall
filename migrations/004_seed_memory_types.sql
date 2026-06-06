-- Migration 004: Seed Memory Types
-- Seeds all 13 memory types with correct family, decay_band, base_lambda, and priority_weight
-- Per agent-brain-schema.md section 1: Memory Taxonomy

PRAGMA user_version = 4;

-- DURABLE family (slow decay, half-life 17-69 days)
INSERT OR IGNORE INTO memory_types (name, family, decay_band, base_lambda, priority_weight)
VALUES
    ('personal_trait', 'durable', 'slow', 0.03, 1.0),
    ('professional_trait', 'durable', 'slow', 0.03, 1.0),
    ('decision_principle', 'durable', 'slow', 0.03, 1.0),
    ('like_interest', 'durable', 'slow', 0.03, 0.9);

-- OPERATIONAL family (mid decay, half-life 10-17 days)
INSERT OR IGNORE INTO memory_types (name, family, decay_band, base_lambda, priority_weight)
VALUES
    ('preference', 'operational', 'mid', 0.05, 0.9),
    ('project_context', 'operational', 'mid', 0.05, 0.9),
    ('procedural', 'operational', 'mid', 0.05, 0.85),
    ('convention', 'operational', 'mid', 0.05, 0.85),
    ('client_context', 'operational', 'mid', 0.05, 0.85),
    ('team_context', 'operational', 'mid', 0.05, 0.7);

-- EPHEMERAL family (fast decay, half-life 2-7 days)
INSERT OR IGNORE INTO memory_types (name, family, decay_band, base_lambda, priority_weight)
VALUES
    ('workaround', 'ephemeral', 'fast', 0.13, 0.5),
    ('failure_warning', 'ephemeral', 'fast', 0.13, 0.5),
    ('task_detail', 'ephemeral', 'fast', 0.13, 0.5);
