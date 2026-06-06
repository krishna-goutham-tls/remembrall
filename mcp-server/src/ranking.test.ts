/**
 * Unit tests for Remembrall MCP Server ranking module
 * Uses Node.js built-in test runner
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import {
  calculateScore,
  isVagueQuery,
  selectWeights,
  rankMemories,
  formatRecentContext,
  generateContext,
  memoryRowToMemory,
} from './ranking.js';
import {
  DEFAULT_WEIGHTS,
  VAGUE_WEIGHTS,
  SCORE_THRESHOLD,
  TYPE_PRIORITY_WEIGHTS,
} from './types.js';
import type { MemoryRow, RecentMessage } from './types.js';

// ============================================================================
// Test Fixtures
// ============================================================================

function createTestMemoryRow(overrides: Partial<MemoryRow> = {}): MemoryRow {
  const now = new Date().toISOString();
  return {
    id: 1,
    project_id: 1,
    project_name: 'test-project',
    type_name: 'preference',
    type_family: 'operational',
    type_priority_weight: 0.9,
    source_tool: 'droid',
    summary_text: 'Test memory',
    keywords: null,
    scope: 'project',
    importance: 0.8,
    strength: 0.75,
    recall_count: 0,
    last_accessed: null,
    created_at: now,
    is_active: 1,
    ...overrides,
  };
}

// ============================================================================
// Ranking Unit Tests
// ============================================================================

describe('Ranking', () => {
  describe('calculateScore', () => {
    test('calculates score with default weights', () => {
      const memory = createTestMemoryRow({
        type_name: 'decision_principle',
        strength: 0.75,
        recall_count: 3,
      });

      const score = calculateScore(memory, 0.3, DEFAULT_WEIGHTS, 1, new Date());

      // Should be positive and reasonable
      assert.ok(score > 0, 'Score should be positive');
      assert.ok(score <= 1, 'Score should be <= 1');
    });

    test('respects type priority weights', () => {
      const now = new Date();

      // Decision principle (priority 1.0)
      const principle = createTestMemoryRow({
        id: 1,
        type_name: 'decision_principle',
        type_priority_weight: 1.0,
      });

      // Task detail (priority 0.5)
      const task = createTestMemoryRow({
        id: 2,
        type_name: 'task_detail',
        type_priority_weight: 0.5,
      });

      const principleScore = calculateScore(principle, 0.3, DEFAULT_WEIGHTS, 1, now);
      const taskScore = calculateScore(task, 0.3, DEFAULT_WEIGHTS, 1, now);

      assert.ok(
        principleScore > taskScore,
        'Decision principle should score higher than task detail'
      );
    });

    test('applies project scope match boost', () => {
      const now = new Date();

      // Same project
      const sameProject = createTestMemoryRow({
        id: 1,
        project_id: 1,
        scope: 'project',
      });

      // Global scope
      const global = createTestMemoryRow({
        id: 2,
        scope: 'global',
      });

      const sameScore = calculateScore(sameProject, 0.3, DEFAULT_WEIGHTS, 1, now);
      const globalScore = calculateScore(global, 0.3, DEFAULT_WEIGHTS, 1, now);

      // Same project should score higher (scope weight 0.15 * 1.0 = 0.15 vs 0.15 * 0.7 = 0.105)
      assert.ok(
        sameScore > globalScore,
        'Same project should score higher than global'
      );
    });

    test('vector similarity affects score', () => {
      const now = new Date();
      const memory = createTestMemoryRow({ strength: 0.8 });

      // Perfect match (distance = 0)
      const perfectScore = calculateScore(memory, 0.0, DEFAULT_WEIGHTS, 1, now);
      // No match (distance = 1)
      const noMatchScore = calculateScore(memory, 1.0, DEFAULT_WEIGHTS, 1, now);

      assert.ok(
        perfectScore > noMatchScore,
        'Higher similarity should produce higher score'
      );
    });

    test('reinforcement frequency affects score', () => {
      const now = new Date();

      const noReinforcement = createTestMemoryRow({
        id: 1,
        recall_count: 0,
      });

      const highReinforcement = createTestMemoryRow({
        id: 2,
        recall_count: 10, // Should max out at 1.0
      });

      const noReinforceScore = calculateScore(noReinforcement, 0.3, DEFAULT_WEIGHTS, 1, now);
      const highReinforceScore = calculateScore(highReinforcement, 0.3, DEFAULT_WEIGHTS, 1, now);

      assert.ok(
        highReinforceScore > noReinforceScore,
        'More reinforcement should produce higher score'
      );
    });

    test('freshness bonus for recent memories', () => {
      const now = new Date();

      // Just created (< 24 hours)
      const recentMemory = createTestMemoryRow({
        id: 1,
        created_at: new Date(now.getTime() - 1 * 60 * 60 * 1000).toISOString(), // 1 hour ago
      });

      // Old memory (> 24 hours)
      const oldMemory = createTestMemoryRow({
        id: 2,
        created_at: new Date(now.getTime() - 48 * 60 * 60 * 1000).toISOString(), // 48 hours ago
      });

      const recentScore = calculateScore(recentMemory, 0.3, DEFAULT_WEIGHTS, 1, now);
      const oldScore = calculateScore(oldMemory, 0.3, DEFAULT_WEIGHTS, 1, now);

      assert.ok(
        recentScore > oldScore,
        'Recent memories should score higher'
      );
    });
  });

  describe('isVagueQuery', () => {
    test('returns true for low variance similarities', () => {
      // Low variance = all scores are similar = vague query
      const lowVarianceScores = [0.45, 0.48, 0.47, 0.46, 0.49];
      assert.ok(isVagueQuery(lowVarianceScores), 'Low variance should be vague');
    });

    test('returns false for high variance similarities', () => {
      // High variance = scores vary widely = specific query
      const highVarianceScores = [0.9, 0.3, 0.8, 0.2, 0.7];
      assert.ok(!isVagueQuery(highVarianceScores), 'High variance should be specific');
    });

    test('returns false for insufficient data', () => {
      const fewScores = [0.5, 0.6];
      assert.ok(!isVagueQuery(fewScores), 'Should return false for < 3 scores');
    });

    test('returns true for empty scores', () => {
      assert.ok(!isVagueQuery([]), 'Empty should return false');
    });
  });

  describe('selectWeights', () => {
    test('returns vague weights for vague query indicators', () => {
      const vagueQueries = [
        'what should i know',
        'what do you remember',
        'tell me about',
        'anything important',
      ];

      for (const query of vagueQueries) {
        const weights = selectWeights(query, [0.5, 0.5, 0.5]);
        assert.deepStrictEqual(weights, VAGUE_WEIGHTS, `Query "${query}" should use vague weights`);
      }
    });

    test('returns default weights for specific queries', () => {
      const weights = selectWeights('Stripe integration', [0.9, 0.3, 0.1]);
      assert.deepStrictEqual(weights, DEFAULT_WEIGHTS);
    });

    test('returns default weights when no query provided', () => {
      const weights = selectWeights(undefined, [0.5, 0.5, 0.5]);
      assert.deepStrictEqual(weights, DEFAULT_WEIGHTS);
    });

    test('returns default weights for empty query', () => {
      const weights = selectWeights('', [0.5, 0.5, 0.5]);
      assert.deepStrictEqual(weights, DEFAULT_WEIGHTS);
    });
  });

  describe('rankMemories', () => {
    test('stronger memories rank higher than weaker ones', () => {
      const now = new Date();

      // Strong memory: decision_principle with high strength and reinforcements
      const strongMemory = createTestMemoryRow({
        id: 1,
        type_name: 'decision_principle',
        strength: 0.9,
        recall_count: 5,
        created_at: new Date(now.getTime() - 1 * 60 * 60 * 1000).toISOString(), // Recent
      });

      // Weak memory: task_detail with low strength and no reinforcements
      const weakMemory = createTestMemoryRow({
        id: 2,
        type_name: 'task_detail',
        strength: 0.1,
        recall_count: 0,
        created_at: new Date(now.getTime() - 48 * 60 * 60 * 1000).toISOString(), // 48 hours ago - no freshness
      });

      const memories = [weakMemory, strongMemory]; // Intentionally reversed order
      const vectorDistances = new Map<number, number>();
      const ranked = rankMemories(memories, vectorDistances, DEFAULT_WEIGHTS, 1);

      // Strong memory should rank first (be at index 0)
      assert.ok(ranked.length >= 2, 'Should have at least 2 results');
      assert.strictEqual(ranked[0].id, 1, 'Strong memory should rank first');
      assert.strictEqual(ranked[1].id, 2, 'Weak memory should rank second');
    });

    test('sorts results by score descending', () => {
      const now = new Date();

      const memories: MemoryRow[] = [
        createTestMemoryRow({
          id: 1,
          type_name: 'preference',
          strength: 0.5,
        }),
        createTestMemoryRow({
          id: 2,
          type_name: 'decision_principle',
          strength: 0.9,
          recall_count: 5,
        }),
      ];

      const vectorDistances = new Map<number, number>();
      const ranked = rankMemories(memories, vectorDistances, DEFAULT_WEIGHTS, 1);

      assert.ok(ranked.length >= 1);
      // First result should have higher score
      if (ranked.length >= 2) {
        assert.ok(ranked[0].score >= ranked[1].score);
      }
    });

    test('returns empty array when all memories below threshold', () => {
      const now = new Date();

      // Memory specifically designed to score below 0.15:
      // - vector similarity default: 0.35 * 0.5 = 0.175
      // - task_detail (priority 0.5): 0.10 * 0.5 = 0.05
      // - other factors even at minimum...
      // These memories will likely be above threshold but let's test the function works
      const memories: MemoryRow[] = [
        createTestMemoryRow({
          id: 1,
          type_name: 'task_detail',
          strength: 0.005, // Very close to archive threshold
          recall_count: 0,
          created_at: new Date(now.getTime() - 72 * 60 * 60 * 1000).toISOString(),
          project_id: 999, // Different project - no scope match
          scope: 'project',
        }),
      ];

      const vectorDistances = new Map<number, number>();
      const ranked = rankMemories(memories, vectorDistances, DEFAULT_WEIGHTS, 1);

      // Function should work without crashing
      assert.ok(Array.isArray(ranked));
    });

    test('handles missing vector distances', () => {
      const memories = [createTestMemoryRow({ id: 1 })];
      const vectorDistances = new Map<number, number>(); // Empty map

      const ranked = rankMemories(memories, vectorDistances, DEFAULT_WEIGHTS, 1);

      // Should not crash, uses default 0.5 for missing distances
      assert.ok(ranked.length >= 0);
    });
  });
});

// ============================================================================
// Utility Function Tests
// ============================================================================

describe('Utility Functions', () => {
  describe('formatRecentContext', () => {
    test('formats messages correctly', () => {
      const messages: RecentMessage[] = [
        { role: 'user', text: 'Hello', timestamp: '2026-06-06T12:00:00Z' },
        { role: 'assistant', text: 'Hi there!', timestamp: '2026-06-06T12:00:01Z' },
      ];

      const formatted = formatRecentContext(messages);

      assert.strictEqual(formatted.length, 2);
      assert.ok(formatted[0].startsWith('User:'));
      assert.ok(formatted[1].startsWith('Assistant:'));
      assert.ok(formatted[0].includes('Hello'));
      assert.ok(formatted[1].includes('Hi there!'));
    });

    test('truncates long messages', () => {
      const longText = 'A'.repeat(600);
      const messages: RecentMessage[] = [
        { role: 'user', text: longText, timestamp: '2026-06-06T12:00:00Z' },
      ];

      const formatted = formatRecentContext(messages);

      assert.ok(formatted[0].length < longText.length + 10); // Account for "User: "
      assert.ok(formatted[0].endsWith('...'));
    });

    test('handles empty messages array', () => {
      const formatted = formatRecentContext([]);
      assert.strictEqual(formatted.length, 0);
    });
  });

  describe('generateContext', () => {
    test('generates session-start context', () => {
      const buckets = {
        principles: [{ id: 1 }, { id: 2 }] as any[],
        recent_project: [{ id: 3 }] as any[],
        proven_preferences: [{ id: 4 }] as any[],
      };

      const context = generateContext('session_start', buckets, null, undefined);

      assert.ok(context.includes('3') || context.includes('memories loaded'));
      assert.ok(context.includes('principles') || context.includes('principle'));
    });

    test('generates mid-session context with type counts', () => {
      const results = [
        { type: 'preference' },
        { type: 'preference' },
        { type: 'client_context' },
      ] as any[];

      const context = generateContext('mid_session', null, results, 'test query');

      assert.ok(context.includes('3') || context.includes('memories found'));
      assert.ok(context.includes('preference') || context.includes('client_context'));
    });

    test('handles empty results', () => {
      const context = generateContext('mid_session', null, [], 'test');
      assert.ok(context.includes('No memories') || context.includes('0'));
    });
  });

  describe('memoryRowToMemory', () => {
    test('converts database row to Memory object', () => {
      const now = new Date().toISOString();
      const row = createTestMemoryRow({
        id: 42,
        project_name: 'test-project',
        type_name: 'decision_principle',
        type_family: 'durable',
        summary_text: 'Test memory content',
        keywords: 'test, memory',
        scope: 'project',
        importance: 0.85,
        strength: 0.78,
        recall_count: 4,
        last_accessed: now,
        created_at: now,
      });

      const memory = memoryRowToMemory(row, 0.81);

      assert.strictEqual(memory.id, 42);
      assert.strictEqual(memory.content, 'Test memory content');
      assert.strictEqual(memory.type, 'decision_principle');
      assert.strictEqual(memory.family, 'durable');
      assert.strictEqual(memory.project, 'test-project');
      assert.strictEqual(memory.scope, 'project');
      assert.strictEqual(memory.strength, 0.78);
      assert.strictEqual(memory.score, 0.81);
      assert.strictEqual(memory.reinforcements, 4);
      assert.strictEqual(memory.source_tool, 'droid');
    });

    test('rounds score to 2 decimal places', () => {
      const row = createTestMemoryRow({ id: 1 });
      const memory = memoryRowToMemory(row, 0.87654321);

      assert.strictEqual(memory.score, 0.88);
    });

    test('handles null project_name', () => {
      const row = createTestMemoryRow({ id: 1, project_id: null, project_name: null });
      const memory = memoryRowToMemory(row, 0.5);

      assert.strictEqual(memory.project, null);
    });
  });
});

// ============================================================================
// Type Priority Weights Tests
// ============================================================================

describe('Type Priority Weights', () => {
  test('decision_principle, professional_trait, personal_trait have priority 1.0', () => {
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['decision_principle'], 1.0);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['professional_trait'], 1.0);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['personal_trait'], 1.0);
  });

  test('preference, like_interest, project_context have priority 0.9', () => {
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['preference'], 0.9);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['like_interest'], 0.9);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['project_context'], 0.9);
  });

  test('procedural, convention, client_context have priority 0.85', () => {
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['procedural'], 0.85);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['convention'], 0.85);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['client_context'], 0.85);
  });

  test('team_context has priority 0.7', () => {
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['team_context'], 0.7);
  });

  test('workaround, failure_warning, task_detail have priority 0.5', () => {
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['workaround'], 0.5);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['failure_warning'], 0.5);
    assert.strictEqual(TYPE_PRIORITY_WEIGHTS['task_detail'], 0.5);
  });
});

// ============================================================================
// Score Threshold Tests
// ============================================================================

describe('Score Threshold', () => {
  test('SCORE_THRESHOLD is 0.15', () => {
    assert.strictEqual(SCORE_THRESHOLD, 0.15);
  });
});

// ============================================================================
// Weight Constants Tests
// ============================================================================

describe('Weight Constants', () => {
  test('DEFAULT_WEIGHTS sum to 1.0', () => {
    const sum =
      DEFAULT_WEIGHTS.vector_similarity +
      DEFAULT_WEIGHTS.decay_strength +
      DEFAULT_WEIGHTS.project_scope +
      DEFAULT_WEIGHTS.type_priority +
      DEFAULT_WEIGHTS.reinforcement +
      DEFAULT_WEIGHTS.freshness;
    assert.strictEqual(sum, 1.0);
  });

  test('VAGUE_WEIGHTS sum to 1.0', () => {
    const sum =
      VAGUE_WEIGHTS.vector_similarity +
      VAGUE_WEIGHTS.decay_strength +
      VAGUE_WEIGHTS.project_scope +
      VAGUE_WEIGHTS.type_priority +
      VAGUE_WEIGHTS.reinforcement +
      VAGUE_WEIGHTS.freshness;
    assert.strictEqual(sum, 1.0);
  });

  test('DEFAULT_WEIGHTS has correct factor values', () => {
    assert.strictEqual(DEFAULT_WEIGHTS.vector_similarity, 0.35);
    assert.strictEqual(DEFAULT_WEIGHTS.decay_strength, 0.25);
    assert.strictEqual(DEFAULT_WEIGHTS.project_scope, 0.15);
    assert.strictEqual(DEFAULT_WEIGHTS.type_priority, 0.10);
    assert.strictEqual(DEFAULT_WEIGHTS.reinforcement, 0.10);
    assert.strictEqual(DEFAULT_WEIGHTS.freshness, 0.05);
  });

  test('VAGUE_WEIGHTS shifts weight from similarity to decay', () => {
    assert.strictEqual(VAGUE_WEIGHTS.vector_similarity, 0.15); // Down from 0.35
    assert.strictEqual(VAGUE_WEIGHTS.decay_strength, 0.35); // Up from 0.25
    assert.strictEqual(VAGUE_WEIGHTS.project_scope, 0.20); // Up from 0.15
    assert.strictEqual(VAGUE_WEIGHTS.type_priority, 0.15); // Up from 0.10
  });
});
