-- Migration 005: Add last_message_count to active_sessions
-- Track the last processed message count for efficient correlation

PRAGMA user_version = 5;

-- Add last_message_count column to track processed messages
-- This allows correlation to efficiently extract recent messages without parsing entire file
ALTER TABLE active_sessions ADD COLUMN last_message_count INTEGER NOT NULL DEFAULT 0;
