/**
 * TypeScript interfaces for Remembrall MCP server
 * Based on agent-brain-schema.md sections 3, 6, 7
 */

// Memory types from the database
export type MemoryFamily = 'durable' | 'operational' | 'ephemeral';
export type MemoryType =
  | 'personal_trait'
  | 'professional_trait'
  | 'decision_principle'
  | 'like_interest'
  | 'preference'
  | 'project_context'
  | 'procedural'
  | 'convention'
  | 'client_context'
  | 'team_context'
  | 'workaround'
  | 'failure_warning'
  | 'task_detail';

export type Scope = 'global' | 'project';
export type SourceTool = 'droid' | 'codex' | 'claude' | 'cursor';

// Core memory object returned in recall responses
export interface Memory {
  id: number;
  content: string;
  type: MemoryType;
  family: MemoryFamily;
  project: string | null;
  scope: Scope;
  strength: number;
  score: number;
  reinforcements: number;
  source_tool: SourceTool;
  created_at: string;
  last_accessed: string | null;
}

// Database row from memories + memory_types join
export interface MemoryRow {
  id: number;
  project_id: number | null;
  project_name: string | null;
  type_name: MemoryType;
  type_family: MemoryFamily;
  type_priority_weight: number;
  source_tool: SourceTool;
  summary_text: string;
  keywords: string | null;
  scope: Scope;
  importance: number;
  strength: number;
  recall_count: number;
  last_accessed: string | null;
  created_at: string;
  is_active: number;
}

// Vector distance from sqlite-vec
export interface VectorMatch {
  memory_id: number;
  distance: number;
}

// FTS5 match from keyword search
export interface FtsMatch {
  memory_id: number;
  rank: number;
}

// Recent message from session file
export interface RecentMessage {
  role: 'user' | 'assistant';
  text: string;
  timestamp: string;
}

// Session-start recall buckets
export interface SessionStartBuckets {
  principles: Memory[];
  recent_project: Memory[];
  proven_preferences: Memory[];
}

// Session-start recall response
export interface SessionStartResponse {
  recall_type: 'session_start';
  project: string | null;
  result_count: number;
  buckets: SessionStartBuckets;
  context: string;
}

// Mid-session recall response
export interface MidSessionResponse {
  recall_type: 'mid_session';
  query: string;
  project: string | null;
  result_count: number;
  results: Memory[];
  recent_context: string[];
  context: string;
}

// Error response
export interface ErrorResponse {
  recall_type: 'session_start' | 'mid_session';
  project: string | null;
  result_count: number;
  results?: never;
  context: string;
}

// Recall tool input arguments
export interface RecallArguments {
  query?: string;
  project?: string;
  limit?: number;
}

// Ranking weights for 6-factor formula
export interface RankingWeights {
  vector_similarity: number;
  decay_strength: number;
  project_scope: number;
  type_priority: number;
  reinforcement: number;
  freshness: number;
}

// Default weights for specific queries
export const DEFAULT_WEIGHTS: RankingWeights = {
  vector_similarity: 0.35,
  decay_strength: 0.25,
  project_scope: 0.15,
  type_priority: 0.10,
  reinforcement: 0.10,
  freshness: 0.05,
};

// Adaptive weights for vague queries
export const VAGUE_WEIGHTS: RankingWeights = {
  vector_similarity: 0.15,
  decay_strength: 0.35,
  project_scope: 0.20,
  type_priority: 0.15,
  reinforcement: 0.10,
  freshness: 0.05,
};

// Type priority weights from agent-brain-schema.md section 6.3
export const TYPE_PRIORITY_WEIGHTS: Record<MemoryType, number> = {
  decision_principle: 1.0,
  professional_trait: 1.0,
  personal_trait: 1.0,
  preference: 0.9,
  like_interest: 0.9,
  project_context: 0.9,
  procedural: 0.85,
  convention: 0.85,
  client_context: 0.85,
  team_context: 0.7,
  workaround: 0.5,
  failure_warning: 0.5,
  task_detail: 0.5,
};

// Score threshold for filtering weak results
export const SCORE_THRESHOLD = 0.15;

// Max results
export const MAX_SESSION_START_RESULTS = 15;
export const MAX_MID_SESSION_RESULTS = 10;
