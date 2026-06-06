/**
 * MCP Server for Remembrall recall tool
 * JSON-RPC 2.0 stdio transport
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  Tool,
} from '@modelcontextprotocol/sdk/types.js';
import Database from 'better-sqlite3';
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import type {
  RecallArguments,
  MemoryRow,
  SessionStartResponse,
  MidSessionResponse,
  RecentMessage,
  Memory,
} from './types.js';
import {
  rankMemories,
  selectWeights,
  formatRecentContext,
  generateContext,
  memoryRowToMemory,
} from './ranking.js';
import {
  MAX_SESSION_START_RESULTS,
  MAX_MID_SESSION_RESULTS,
  DEFAULT_WEIGHTS,
} from './types.js';

// Database path
const DB_PATH = path.join(
  os.homedir(),
  'Library',
  'Application Support',
  'Remembrall',
  'brain.db'
);

// Re-exports for testing
export type { RecallArguments, MemoryRow, RecentMessage };

// ============================================================================
// Database Connection
// ============================================================================

let db: Database.Database | null = null;

function getDatabase(): Database.Database {
  if (!db) {
    // Ensure directory exists
    const dbDir = path.dirname(DB_PATH);
    if (!fs.existsSync(dbDir)) {
      fs.mkdirSync(dbDir, { recursive: true });
    }

    // Open in WAL mode, read-only
    db = new Database(DB_PATH, { readonly: true, fileMustExist: true });
    db.pragma('journal_mode = WAL');
    db.pragma('query_only = true');
  }
  return db;
}

function closeDatabase(): void {
  if (db) {
    db.close();
    db = null;
  }
}

// ============================================================================
// Recent Context from Session Files
// ============================================================================

const RECENT_CONTEXT_LINES = 10;
const MAX_RECENT_MESSAGES = 6;

interface RecentContextResult {
  messages: RecentMessage[];
  projectMatched: boolean;
}

function getRecentContext(projectArg: string | undefined): RecentContextResult {
  const conn = getDatabase();
  const messages: RecentMessage[] = [];
  let projectMatched = false;

  try {
    // Find project by path or name
    let projectId: number | null = null;

    if (projectArg) {
      // Try path match first
      let row = conn
        .prepare('SELECT id FROM projects WHERE path = ?1 LIMIT 1')
        .get(projectArg) as { id: number } | undefined;

      if (!row) {
        // Try name match
        row = conn
          .prepare('SELECT id FROM projects WHERE name = ?1 LIMIT 1')
          .get(projectArg) as { id: number } | undefined;
      }

      if (row) {
        projectId = row.id;
        projectMatched = true;
      }
    }

    // Get most recent session file
    let sessionFile: string | null = null;
    let messageCount = 0;

    if (projectId !== null) {
      const sessionRow = conn
        .prepare(
          `SELECT session_file, last_message_count
           FROM active_sessions
           WHERE project_id = ?1
           ORDER BY last_modified DESC
           LIMIT 1`
        )
        .get(projectId) as
        | { session_file: string; last_message_count: number }
        | undefined;

      if (sessionRow) {
        sessionFile = sessionRow.session_file;
        messageCount = sessionRow.last_message_count;
      }
    }

    // Fallback to global most recent
    if (!sessionFile) {
      const globalRow = conn
        .prepare(
          `SELECT session_file, last_message_count
           FROM active_sessions
           ORDER BY last_modified DESC
           LIMIT 1`
        )
        .get() as
        | { session_file: string; last_message_count: number }
        | undefined;

      if (globalRow) {
        sessionFile = globalRow.session_file;
        messageCount = globalRow.last_message_count;
      }
    }

    // Read last lines from session file
    if (sessionFile && fs.existsSync(sessionFile)) {
      const content = fs.readFileSync(sessionFile, 'utf-8');
      const allLines = content.split('\n').filter((line) => line.trim());

      // Get last N lines
      const totalLines = allLines.length;
      const startIdx = Math.max(0, totalLines - RECENT_CONTEXT_LINES);
      const lastLines = allLines.slice(startIdx);

      // Parse JSONL and extract messages
      for (const line of lastLines) {
        try {
          const parsed = JSON.parse(line);
          if (parsed.type === 'message' && parsed.message) {
            const role = parsed.message.role;
            if ((role === 'user' || role === 'assistant') && parsed.message.content) {
              // Extract text from content array
              let text = '';
              if (Array.isArray(parsed.message.content)) {
                for (const block of parsed.message.content) {
                  if (block.type === 'text' && block.text) {
                    text += block.text + ' ';
                  }
                }
              } else if (typeof parsed.message.content === 'string') {
                text = parsed.message.content;
              }
              text = text.trim();
              if (text) {
                messages.push({
                  role,
                  text,
                  timestamp: parsed.timestamp || '',
                });
              }
            }
          }
        } catch {
          // Skip malformed JSON lines
        }
        if (messages.length >= MAX_RECENT_MESSAGES) break;
      }
    }
  } catch (err) {
    // Log error but don't fail - recent_context is optional
    console.error('Error getting recent context:', err);
  }

  return { messages, projectMatched };
}

// ============================================================================
// Session-Start Recall (no query)
// ============================================================================

function getPrinciples(projectId: number | null): Memory[] {
  const conn = getDatabase();

  // Principles: decision_principle, professional_trait, personal_trait
  // Global or matching project, limit 5
  const rows = conn
    .prepare(
      `SELECT m.*, mt.name as type_name, mt.family as type_family, mt.priority_weight as type_priority_weight,
              p.name as project_name
       FROM memories m
       JOIN memory_types mt ON m.type_id = mt.id
       LEFT JOIN projects p ON m.project_id = p.id
       WHERE m.is_active = 1
         AND mt.name IN ('decision_principle', 'professional_trait', 'personal_trait')
         AND (m.scope = 'global' OR m.project_id = ?1)
       ORDER BY m.strength DESC, m.recall_count DESC
       LIMIT 5`
    )
    .all(projectId) as MemoryRow[];

  return rows.map((row) => memoryRowToMemory(row, row.strength));
}

function getRecentProject(projectId: number | null): Memory[] {
  const conn = getDatabase();

  // Recent project: last 7 days, strength > 0.5, limit 5
  const sevenDaysAgo = new Date();
  sevenDaysAgo.setDate(sevenDaysAgo.getDate() - 7);
  const sevenDaysAgoStr = sevenDaysAgo.toISOString();

  const rows = conn
    .prepare(
      `SELECT m.*, mt.name as type_name, mt.family as type_family, mt.priority_weight as type_priority_weight,
              p.name as project_name
       FROM memories m
       JOIN memory_types mt ON m.type_id = mt.id
       LEFT JOIN projects p ON m.project_id = p.id
       WHERE m.is_active = 1
         AND m.project_id = ?1
         AND m.created_at > ?2
         AND m.strength > 0.5
       ORDER BY m.strength DESC, m.created_at DESC
       LIMIT 5`
    )
    .all(projectId, sevenDaysAgoStr) as MemoryRow[];

  return rows.map((row) => memoryRowToMemory(row, row.strength));
}

function getProvenPreferences(projectId: number | null): Memory[] {
  const conn = getDatabase();

  // Proven preferences: preference + like_interest with recall_count >= 2, limit 5
  const rows = conn
    .prepare(
      `SELECT m.*, mt.name as type_name, mt.family as type_family, mt.priority_weight as type_priority_weight,
              p.name as project_name
       FROM memories m
       JOIN memory_types mt ON m.type_id = mt.id
       LEFT JOIN projects p ON m.project_id = p.id
       WHERE m.is_active = 1
         AND mt.name IN ('preference', 'like_interest')
         AND m.recall_count >= 2
         AND (m.scope = 'global' OR m.project_id = ?1)
       ORDER BY m.recall_count DESC, m.strength DESC
       LIMIT 5`
    )
    .all(projectId) as MemoryRow[];

  return rows.map((row) => memoryRowToMemory(row, row.strength));
}

function sessionStartRecall(projectArg: string | undefined): SessionStartResponse {
  const conn = getDatabase();

  // Find project
  let projectId: number | null = null;
  let projectName: string | null = null;

  if (projectArg) {
    const row = conn
      .prepare(
        `SELECT id, name FROM projects WHERE path = ?1 OR name = ?1 LIMIT 1`
      )
      .get(projectArg) as { id: number; name: string } | undefined;

    if (row) {
      projectId = row.id;
      projectName = row.name;
    }
  }

  // Get buckets
  const principles = getPrinciples(projectId);
  const recentProject = projectId ? getRecentProject(projectId) : [];
  const provenPreferences = getProvenPreferences(projectId);

  // Limit total to MAX_SESSION_START_RESULTS (15)
  const allBuckets = {
    principles,
    recent_project: recentProject,
    proven_preferences: provenPreferences,
  };

  // Count total
  const totalCount =
    allBuckets.principles.length +
    allBuckets.recent_project.length +
    allBuckets.proven_preferences.length;

  // Trim if over limit (prioritize principles, then recent_project, then proven_preferences)
  if (totalCount > MAX_SESSION_START_RESULTS) {
    const remaining = MAX_SESSION_START_RESULTS;
    let count = 0;

    // Take principles first
    if (allBuckets.principles.length > remaining) {
      allBuckets.principles = allBuckets.principles.slice(0, remaining);
    }
    count += allBuckets.principles.length;

    // Then recent_project
    const remainingAfterPrinciples = remaining - count;
    if (
      count < remaining &&
      allBuckets.recent_project.length > remainingAfterPrinciples
    ) {
      allBuckets.recent_project = allBuckets.recent_project.slice(
        0,
        remainingAfterPrinciples
      );
    }
    count += allBuckets.recent_project.length;

    // Then proven_preferences
    const remainingAfterRecent = remaining - count;
    if (
      count < remaining &&
      allBuckets.proven_preferences.length > remainingAfterRecent
    ) {
      allBuckets.proven_preferences = allBuckets.proven_preferences.slice(
        0,
        remainingAfterRecent
      );
    }
  }

  const finalCount =
    allBuckets.principles.length +
    allBuckets.recent_project.length +
    allBuckets.proven_preferences.length;

  if (finalCount === 0) {
    return {
      recall_type: 'session_start',
      project: projectName,
      result_count: 0,
      buckets: { principles: [], recent_project: [], proven_preferences: [] },
      context: 'No indexed memories yet. Backfill may still be in progress.',
    };
  }

  return {
    recall_type: 'session_start',
    project: projectName,
    result_count: finalCount,
    buckets: allBuckets,
    context: generateContext('session_start', allBuckets, null, undefined),
  };
}

// ============================================================================
// Mid-Session Recall (with query)
// ============================================================================

function midSessionRecall(
  query: string,
  projectArg: string | undefined,
  limit: number = MAX_MID_SESSION_RESULTS
): MidSessionResponse {
  const conn = getDatabase();

  // Find project
  let projectId: number | null = null;
  let projectName: string | null = null;

  if (projectArg) {
    const row = conn
      .prepare(
        `SELECT id, name FROM projects WHERE path = ?1 OR name = ?1 LIMIT 1`
      )
      .get(projectArg) as { id: number; name: string } | undefined;

    if (row) {
      projectId = row.id;
      projectName = row.name;
    }
  }

  // FTS5 keyword search
  const searchTerms = query
    .toLowerCase()
    .split(/\s+/)
    .filter((t) => t.length > 1);

  let ftsMemoryIds: Set<number> = new Set();

  if (searchTerms.length > 0) {
    // Build FTS5 query with OR between terms
    const ftsQuery = searchTerms.map((term) => `"${term}"*`).join(' OR ');

    try {
      const ftsRows = conn
        .prepare(
          `SELECT m.id, rank
           FROM memories m
           JOIN fts_memories fts ON m.id = fts.rowid
           WHERE fts_memories MATCH ?1
             AND m.is_active = 1
           ORDER BY rank
           LIMIT 50`
        )
        .all(ftsQuery) as { id: number; rank: number }[];

      ftsMemoryIds = new Set(ftsRows.map((r) => r.id));
    } catch (err) {
      // FTS5 query failed, continue without FTS results
      console.error('FTS5 search error:', err);
    }
  }

  // Get all candidate memories (from FTS or all active)
  let candidateRows: MemoryRow[];

  if (ftsMemoryIds.size > 0) {
    // Query only FTS-matched memories
    const placeholders = Array.from(ftsMemoryIds).map(() => '?').join(',');
    candidateRows = conn
      .prepare(
        `SELECT m.*, mt.name as type_name, mt.family as type_family, mt.priority_weight as type_priority_weight,
                p.name as project_name
         FROM memories m
         JOIN memory_types mt ON m.type_id = mt.id
         LEFT JOIN projects p ON m.project_id = p.id
         WHERE m.is_active = 1
           AND m.id IN (${placeholders})
         ORDER BY m.strength DESC`
      )
      .all(...Array.from(ftsMemoryIds)) as MemoryRow[];
  } else {
    // No FTS matches, query all active memories
    candidateRows = conn
      .prepare(
        `SELECT m.*, mt.name as type_name, mt.family as type_family, mt.priority_weight as type_priority_weight,
                p.name as project_name
         FROM memories m
         JOIN memory_types mt ON m.type_id = mt.id
         LEFT JOIN projects p ON m.project_id = p.id
         WHERE m.is_active = 1
         ORDER BY m.strength DESC
         LIMIT 100`
      )
      .all() as MemoryRow[];
  }

  // For now, we don't have vector similarity (Mission 2 will add it)
  // Use empty map - vector distance factor will use default (0.5)
  const emptyVectorDistances = new Map<number, number>();

  // Calculate similarity scores for adaptive weighting decision
  const topSimilarities: number[] = [];
  for (const row of candidateRows.slice(0, 20)) {
    // Simple keyword match score as proxy for similarity
    let similarity = 0;
    if (searchTerms.length > 0) {
      const contentLower = row.summary_text.toLowerCase();
      const keywordsLower = (row.keywords || '').toLowerCase();
      for (const term of searchTerms) {
        if (contentLower.includes(term)) similarity += 0.5;
        if (keywordsLower.includes(term)) similarity += 0.5;
      }
      similarity = Math.min(similarity / searchTerms.length, 1.0);
    } else {
      similarity = 0.5; // Default for vague queries
    }
    topSimilarities.push(similarity);
  }

  // Select weights adaptively
  const weights = selectWeights(query, topSimilarities);

  // Rank memories
  const rankedResults = rankMemories(
    candidateRows,
    emptyVectorDistances,
    weights,
    projectId
  ).slice(0, limit);

  // Get recent context
  const { messages } = getRecentContext(projectArg);
  const recentContext = formatRecentContext(messages);

  if (rankedResults.length === 0) {
    return {
      recall_type: 'mid_session',
      query,
      project: projectName,
      result_count: 0,
      results: [],
      recent_context: recentContext,
      context: 'No memories match this query.',
    };
  }

  return {
    recall_type: 'mid_session',
    query,
    project: projectName,
    result_count: rankedResults.length,
    results: rankedResults,
    recent_context: recentContext,
    context: generateContext('mid_session', null, rankedResults, query),
  };
}

// ============================================================================
// MCP Server Setup
// ============================================================================

const RECALL_TOOL: Tool = {
  name: 'recall',
  description:
    'Retrieve relevant memories from Remembrall. Call at session start for durable context, and when a major decision is being made. Let Remembrall decide what to return.',
  inputSchema: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description:
          'Optional keyword or concept query. If omitted, returns durable session-start context.',
      },
      project: {
        type: 'string',
        description: 'Current project path or name. Used for scoping.',
      },
      limit: {
        type: 'integer',
        default: 10,
        description: 'Max memories to return.',
      },
    },
    required: [],
  },
};

class RemembrallServer {
  private server: Server;

  constructor() {
    this.server = new Server(
      {
        name: 'remembrall',
        version: '0.1.0',
      },
      {
        capabilities: {
          tools: {},
        },
      }
    );

    this.setupHandlers();
  }

  private setupHandlers() {
    // List tools handler
    this.server.setRequestHandler(ListToolsRequestSchema, async () => {
      return {
        tools: [RECALL_TOOL],
      };
    });

    // Call tool handler
    this.server.setRequestHandler(CallToolRequestSchema, async (request) => {
      const { name, arguments: args } = request.params;

      if (name !== 'recall') {
        return {
          content: [
            {
              type: 'text',
              text: JSON.stringify({
                error: `Unknown tool: ${name}`,
              }),
            },
          ],
          isError: true,
        };
      }

      try {
        const recallArgs = args as RecallArguments;

        // Check if query is provided
        if (recallArgs.query && recallArgs.query.trim().length > 0) {
          // Mid-session recall
          const limit = recallArgs.limit || MAX_MID_SESSION_RESULTS;
          const result = midSessionRecall(
            recallArgs.query,
            recallArgs.project,
            limit
          );
          return {
            content: [
              {
                type: 'text',
                text: JSON.stringify(result),
              },
            ],
          };
        } else {
          // Session-start recall
          const result = sessionStartRecall(recallArgs.project);
          return {
            content: [
              {
                type: 'text',
                text: JSON.stringify(result),
              },
            ],
          };
        }
      } catch (err) {
        // Check if database doesn't exist or is empty
        const errorMessage = err instanceof Error ? err.message : String(err);

        if (
          errorMessage.includes('no such table') ||
          errorMessage.includes('SQLITE_CANTOPEN') ||
          errorMessage.includes('unable to open database') ||
          errorMessage.includes('ENOENT')
        ) {
          // Empty or missing database - return graceful error
          const result = {
            recall_type: 'session_start',
            project: (args as RecallArguments)?.project || null,
            result_count: 0,
            buckets: {
              principles: [],
              recent_project: [],
              proven_preferences: [],
            },
            context: 'No indexed memories yet. Backfill may still be in progress.',
          };
          return {
            content: [
              {
                type: 'text',
                text: JSON.stringify(result),
              },
            ],
          };
        }

        // Other errors
        console.error('Recall error:', err);
        return {
          content: [
            {
              type: 'text',
              text: JSON.stringify({
                error: `Recall failed: ${errorMessage}`,
              }),
            },
          ],
          isError: true,
        };
      }
    });
  }

  async start() {
    const transport = new StdioServerTransport();
    await this.server.connect(transport);
    console.error('Remembrall MCP server started');
  }
}

// ============================================================================
// Main Entry Point
// ============================================================================

const server = new RemembrallServer();
server.start().catch((err) => {
  console.error('Failed to start server:', err);
  process.exit(1);
});

// Handle graceful shutdown
process.on('SIGINT', () => {
  closeDatabase();
  process.exit(0);
});

process.on('SIGTERM', () => {
  closeDatabase();
  process.exit(0);
});
