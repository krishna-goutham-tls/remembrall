# Remembrall

Local-first cross-tool memory for AI coding agents.

Remembrall watches your coding sessions across Droid, Codex, Claude Code, and Cursor. It parses, classifies, embeds, and indexes everything you discuss with your agents — then serves back relevant context when you need it via MCP. All local. No cloud.

## Current State

**Mission 1 complete — core engine.** The Tauri app scaffold, SQLite schema (7 tables + FTS5 + sqlite-vec), JSONL parser, secret redaction, Ebbinghaus decay engine, session/file correlation, and MCP server are all implemented and tested.

**Mission 2 (upcoming)** will add the menubar UI, MLX-based classifier (Qwen3-4B), embedder (bge-base-en-v1.5), and first-run flow.

## Stack

| Layer | Technology |
|-------|-----------|
| Shell | Tauri 2.0 (Rust) |
| Frontend | React (TypeScript) |
| Memory store | SQLite + sqlite-vec + FTS5 |
| Integration | Node.js MCP server (stdlib JSON-RPC 2.0) |
| File watching | FSEvents (notify crate) |
| Classification (M2) | Qwen3-4B @ Q4 via MLX |
| Embeddings (M2) | bge-base-en-v1.5 (768-dim) |

## Project Structure

```
remembrall/
├── src/                    # React frontend (scaffold)
│   ├── App.tsx
│   ├── main.tsx
│   └── index.html
├── src-tauri/              # Tauri shell + Rust core
│   └── src/
│       ├── main.rs         # Entry point
│       ├── lib.rs          # Tauri setup, tray icon
│       ├── db/             # SQLite schema + migrations
│       ├── parser/         # JSONL parser (Droid)
│       ├── redaction.rs    # Secret redaction pipeline
│       ├── decay.rs        # Ebbinghaus decay engine
│       ├── correlation.rs  # Session/file → project correlation
│       └── fswatcher.rs    # FSEvents file watching
├── mcp-server/             # MCP recall server (Node.js)
│   └── src/
│       ├── server.ts       # MCP server (JSON-RPC 2.0 stdio)
│       ├── ranking.ts      # 6-factor retrieval ranking
│       ├── ranking.test.ts # Ranking unit tests
│       └── types.ts        # Shared types + config
├── migrations/             # SQL migration scripts
│   ├── 001_initial_schema.sql
│   ├── 002_add_fts5.sql
│   ├── 003_add_vec0.sql
│   ├── 004_seed_memory_types.sql
│   └── 005_add_session_message_count.sql
└── package.json            # Root workspace (minimal)
```

Key design docs live in `.context/` at the repo parent:
- `architecture.md` — 4-layer design and runtime flow
- `decisions.md` — Locked product and technical choices
- `brain-schema.md` — Full memory taxonomy, decay, storage, ranking
- `ux-flows.md` — Install flow, menubar UX, active session design

## Development Setup

### Prerequisites

- **Rust** 1.96+ (stable)
- **Node.js** 22+
- **Xcode Command Line Tools** (for native macOS compilation)
- **macOS aarch64** (Apple Silicon)

### Install

```bash
# Build the Tauri shell + Rust core
cd src-tauri
cargo build

# Build the MCP server
cd ../mcp-server
npm install
npm run build
```

### Test

```bash
# Rust tests
cd src-tauri
cargo test

# MCP server tests
cd mcp-server
npm test
```

### Lint

```bash
# Rust lint (strict)
cd src-tauri
cargo clippy -- -D warnings

# TypeScript type-check
cd mcp-server
npx tsc --noEmit
```

## Architecture

Remembrall is built in four layers:

1. **Memory Store** — SQLite database with 7 tables, FTS5 full-text search, and sqlite-vec for 768-dim vector embeddings. Schema migrations via `PRAGMA user_version`.

2. **File Watcher + Indexer** — FSEvents monitors `~/.factory/`, `~/.claude/`, `~/.codex/`, and `~/.cursor/`. Pipeline: parse JSONL → redact secrets → classify memory types → embed → index.

3. **MCP Server** — JSON-RPC 2.0 over stdio. Single `recall` tool with optional `query`, `project`, and `limit` args. Session-start returns bucketed JSON (principles, recent project, cross-project); mid-session returns flat ranked list.

4. **Menubar UI** — Mac menubar app with search bar, "what the brain remembers" view, backfill progress, and settings. *(Mission 2)*

### Data Flow

```
FSEvents → Parser → Redaction → Classification → Embedding → SQLite
                                                              ↓
                    Agent ← MCP Server ← 6-factor Ranking ← Query
```

### Memory Model

- **13 memory types** across 3 families: Durable (slow decay), Operational (mid decay), Ephemeral (fast decay)
- **Ebbinghaus decay** with reinforcement: `strength = importance × e^(-λ × days) × min(1 + recall_count × boost, 3.0)`
- **Archive, don't delete** at threshold 0.01; supersede-and-fade for conflicts
- **Project-scoped** by default; global only when cross-project language is detected

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Local-only** | Privacy is core to the product promise. Everything stays on your Mac. |
| **Droid-first** | Droid/Factory is the first integration target. Codex, Claude Code, Cursor follow. |
| **MCP integration** | Agents pull memories on their terms. Remembrall serves them; the agent decides when. |
| **Ebbinghaus decay** | Memories fade naturally. Reinforcement on recall. No manual pinning — let the model work. |
| **Global trigger line** | One persistent instruction per tool, not per project. Avoids repo pollution. |
| **Archive, don't delete** | Low-strength memories are archived, not destroyed. Everything is recoverable. |
| **Qwen3-4B @ Q4** | Best-in-class classification accuracy (70% MMLU) for structured English output. Runs locally at 40-60 tok/s. |
| **Supersede-and-fade** | When a new memory contradicts an old one, the old one fades rather than being overwritten. |

## License

MIT
