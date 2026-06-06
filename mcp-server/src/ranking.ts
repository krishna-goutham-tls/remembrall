/**
 * 6-factor ranking formula with adaptive weighting
 * Based on agent-brain-schema.md section 6
 */

import type {
  MemoryRow,
  Memory,
  RankingWeights,
  RecentMessage,
  RecallArguments,
} from './types.js';
import {
  DEFAULT_WEIGHTS,
  VAGUE_WEIGHTS,
  TYPE_PRIORITY_WEIGHTS,
  SCORE_THRESHOLD,
} from './types.js';

/**
 * Calculate the relevance score for a memory given a query
 * Uses 6-factor formula with adaptive weighting
 */
export function calculateScore(
  memory: MemoryRow,
  vectorDistance: number | null,
  weights: RankingWeights,
  currentProjectId: number | null,
  now: Date
): number {
  // Factor 1: Vector similarity (0.35 default, 0.15 for vague)
  // cosine distance: 0 = identical, 1 = opposite
  // convert to similarity: 1.0 - distance
  const vectorSimilarity = vectorDistance !== null ? 1.0 - vectorDistance : 0.5;

  // Factor 2: Decay strength (0.25 default, 0.35 for vague)
  // strength is already computed: importance * exp(-lambda * days) * boost
  const decayStrength = memory.strength;

  // Factor 3: Project scope match (0.15 default, 0.20 for vague)
  // 1.0 = same project, 0.7 = global, 0.0 = other project
  let projectScopeMatch = 0.0;
  if (memory.scope === 'global') {
    projectScopeMatch = 0.7;
  } else if (memory.project_id !== null && currentProjectId !== null) {
    if (memory.project_id === currentProjectId) {
      projectScopeMatch = 1.0;
    } else {
      projectScopeMatch = 0.0; // Other project, exclude entirely
    }
  } else if (memory.scope === 'project' && currentProjectId === null) {
    projectScopeMatch = 0.5; // No project context, partial match
  }

  // Factor 4: Memory type priority (0.10 default, 0.15 for vague)
  const typePriority = TYPE_PRIORITY_WEIGHTS[memory.type_name] ?? 0.5;

  // Factor 5: Reinforcement frequency (0.10 default and vague)
  // min(recall_count / 10, 1.0)
  const reinforcement = Math.min(memory.recall_count / 10, 1.0);

  // Factor 6: Freshness bonus (0.05 for both)
  // 1.0 if created in last 24 hours, 0 otherwise
  let freshnessBonus = 0.0;
  if (memory.created_at) {
    const createdAt = new Date(memory.created_at);
    const hoursDiff = (now.getTime() - createdAt.getTime()) / (1000 * 60 * 60);
    if (hoursDiff < 24) {
      freshnessBonus = 1.0;
    }
  }

  // Weighted sum
  const score =
    weights.vector_similarity * vectorSimilarity +
    weights.decay_strength * decayStrength +
    weights.project_scope * projectScopeMatch +
    weights.type_priority * typePriority +
    weights.reinforcement * reinforcement +
    weights.freshness * freshnessBonus;

  return score;
}

/**
 * Determine if query is vague (low variance in top similarity scores)
 * Returns true if the query should use adaptive/vague weights
 */
export function isVagueQuery(topSimilarities: number[]): boolean {
  if (topSimilarities.length < 3) {
    // Not enough data, assume specific
    return false;
  }

  // Calculate variance of top 20 similarity scores
  // If variance is low (< 0.05), query is vague
  const mean = topSimilarities.reduce((a, b) => a + b, 0) / topSimilarities.length;
  const variance =
    topSimilarities.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) /
    topSimilarities.length;

  // Vague query has low variance (scores are similar to each other)
  return variance < 0.05;
}

/**
 * Select appropriate weights based on query specificity
 */
export function selectWeights(
  query: string | undefined,
  topSimilarities: number[]
): RankingWeights {
  if (!query || query.trim().length === 0) {
    // No query = session-start, use default weights
    return DEFAULT_WEIGHTS;
  }

  // Check for vague query indicators
  const vagueIndicators = [
    'what should i know',
    'what do you remember',
    'tell me about',
    'what are my',
    'anything important',
    'what have we discussed',
    'remind me',
    'context',
  ];

  const lowerQuery = query.toLowerCase();
  const hasVagueIndicator = vagueIndicators.some((indicator) =>
    lowerQuery.includes(indicator)
  );

  if (hasVagueIndicator || isVagueQuery(topSimilarities)) {
    return VAGUE_WEIGHTS;
  }

  return DEFAULT_WEIGHTS;
}

/**
 * Rank and filter memories by score, returning only those above threshold
 */
export function rankMemories(
  memories: MemoryRow[],
  vectorDistances: Map<number, number>,
  weights: RankingWeights,
  currentProjectId: number | null
): Memory[] {
  const now = new Date();

  const scoredMemories = memories.map((mem) => {
    const vectorDistance = vectorDistances.get(mem.id) ?? null;
    const score = calculateScore(mem, vectorDistance, weights, currentProjectId, now);
    return { memory: mem, score };
  });

  // Filter by threshold and sort by score descending
  return scoredMemories
    .filter(({ score }) => score > SCORE_THRESHOLD)
    .sort((a, b) => b.score - a.score)
    .map(({ memory, score }) => memoryRowToMemory(memory, score));
}

/**
 * Convert a database row to a Memory object for API response
 */
export function memoryRowToMemory(row: MemoryRow, score: number): Memory {
  return {
    id: row.id,
    content: row.summary_text,
    type: row.type_name,
    family: row.type_family,
    project: row.project_name,
    scope: row.scope,
    strength: row.strength,
    score: Math.round(score * 100) / 100, // Round to 2 decimal places
    reinforcements: row.recall_count,
    source_tool: row.source_tool,
    created_at: row.created_at,
    last_accessed: row.last_accessed,
  };
}

/**
 * Build recent_context array from messages for mid-session recall
 */
export function formatRecentContext(messages: RecentMessage[]): string[] {
  return messages.map((msg) => {
    const role = msg.role === 'user' ? 'User' : 'Assistant';
    // Truncate very long messages
    const text =
      msg.text.length > 500 ? msg.text.substring(0, 500) + '...' : msg.text;
    return `${role}: ${text}`;
  });
}

/**
 * Generate context message string for response
 */
export function generateContext(
  recallType: 'session_start' | 'mid_session',
  buckets: { principles: Memory[]; recent_project: Memory[]; proven_preferences: Memory[] } | null,
  results: Memory[] | null,
  query: string | undefined
): string {
  if (recallType === 'session_start' && buckets) {
    const total = buckets.principles.length + buckets.recent_project.length + buckets.proven_preferences.length;
    const parts: string[] = [];
    if (buckets.principles.length > 0) {
      parts.push(`${buckets.principles.length} principles`);
    }
    if (buckets.recent_project.length > 0) {
      parts.push(`${buckets.recent_project.length} recent project details`);
    }
    if (buckets.proven_preferences.length > 0) {
      parts.push(`${buckets.proven_preferences.length} proven preferences`);
    }
    return `${total} memories loaded: ${parts.join(', ')}.`;
  } else if (recallType === 'mid_session' && results) {
    if (results.length === 0) {
      return 'No memories match this query.';
    }
    // Group by type for context message
    const typeCounts: Record<string, number> = {};
    results.forEach((mem) => {
      typeCounts[mem.type] = (typeCounts[mem.type] || 0) + 1;
    });
    const parts = Object.entries(typeCounts)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([type, count]) => `${count} ${type.replace('_', ' ')}`);
    return `${results.length} memories found: ${parts.join(', ')}.`;
  }
  return '';
}
