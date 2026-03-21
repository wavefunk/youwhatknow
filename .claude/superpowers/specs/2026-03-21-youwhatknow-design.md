# youwhatknow — Design Spec

## Problem

Claude Code reads files aggressively during exploration, often consuming large files that aren't relevant and re-reading the same files multiple times in a session. This wastes context tokens and slows down work.

## Solution

A local background HTTP server that integrates with Claude Code hooks to:

1. Intercept file reads and inject compact summaries before the full read proceeds, letting Claude decide if it actually needs the file
2. Track per-session read counts and warn on repeated reads
3. Provide a project structure map at session start

## Architecture

### Components

**HTTP Server (Axum)** — listens on `localhost:<port>` (default `7849`, configurable). Endpoints:

- `POST /hook/pre-read` — PreToolUse hook for Read tool
- `POST /hook/session-start` — SessionStart hook
- `POST /reindex` — trigger a full re-index on demand
- `GET /health` — health check, returns indexing status

**Indexer** — background Tokio task that scans the codebase:

- Uses `git ls-files` to discover files (respects `.gitignore`)
- Skips binary files, files over 100KB, and default ignore patterns (lock files, minified JS, build artifacts)
- Extracts public symbols via `tree-sitter`
- Generates one-line file descriptions via batched Haiku calls through `claude` CLI
- Writes results to `.claude/summaries/` as TOML files using atomic writes (write to temp file, then rename)
- On startup: loads existing TOML summaries from disk into memory, then runs incremental re-index in background for changed files
- On first run: full scan, generates all summaries
- Exposes an `is_ready` flag so the server knows when initial indexing is complete

**Session Tracker** — in-memory `HashMap<(session_id, file_path) -> read_count>`. Ephemeral, not persisted. A background task sweeps stale sessions every 10 minutes, removing sessions with no activity for `session_timeout_minutes` (default 60). Last-activity timestamp tracked per session.

**Config (Figment + TOML)** — reads from `.claude/youwhatknow.toml`:

- `port` — server port (default `7849`)
- `summary_path` — where to store summaries (default `.claude/summaries/`)
- `ignored_patterns` — additional glob patterns to skip during indexing
- `session_timeout_minutes` — session inactivity timeout (default `60`)
- `max_file_size_kb` — max file size to index (default `100`)

### Data Flow

```
Claude Code hook fires (PreToolUse / Read)
  -> HTTP POST to youwhatknow with session_id, file_path, cwd
  -> Server looks up file in in-memory index
  -> Checks session read count for that file
  -> Returns JSON with permissionDecision: "allow" + summary as additionalContext
  -> Claude sees injected summary, decides whether to proceed with full read
```

```
Claude Code hook fires (SessionStart)
  -> HTTP POST to youwhatknow with session_id, cwd
  -> Server returns project structure map as additionalContext
```

## Hook Integration

Uses HTTP hooks in Claude Code `settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Read",
        "hooks": [
          {
            "type": "http",
            "url": "http://localhost:7849/hook/pre-read"
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "startup",
        "hooks": [
          {
            "type": "http",
            "url": "http://localhost:7849/hook/session-start"
          }
        ]
      }
    ]
  }
}
```

### Hook Input (received as POST body)

PreToolUse:
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PreToolUse",
  "tool_name": "Read",
  "tool_input": {
    "file_path": "/absolute/path/to/file.rs"
  }
}
```

`session_id` is provided by Claude Code in every hook payload.

SessionStart:
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "SessionStart"
}
```

### Hook Output

PreToolUse response (allow read, inject summary via `additionalContext`):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "additionalContext": "-- youwhatknow: src/indexer.rs --\nCodebase indexer — walks repo, extracts symbols via tree-sitter\nPublic: Indexer, IndexEntry, index_project(), reindex_changed()\nFolder: src/ — Core application logic"
  }
}
```

PreToolUse response (file not indexed or outside project):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow"
  }
}
```
No `additionalContext` — the read proceeds without any injected summary.

PreToolUse response (indexing still in progress):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "additionalContext": "-- youwhatknow: indexing in progress --\nFile summaries are still being generated. This read will proceed without a summary."
  }
}
```

SessionStart response (inject project map):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "-- youwhatknow: project map --\nsrc/ — Core application logic\n  main.rs — Entry point, server startup\n  ..."
  }
}
```

HTTP hooks that fail to connect are non-blocking — if the server isn't running, Claude works normally.

## Response Format

**Pre-read (first read):**
```
-- youwhatknow: src/indexer.rs --
Codebase indexer — walks repo, extracts symbols via tree-sitter
Public: Indexer, IndexEntry, index_project(), reindex_changed()
Folder: src/ — Core application logic
```

**Pre-read (re-read, 2nd+ time):**
```
-- youwhatknow: src/indexer.rs (read 2x this session) --
This file was already read. Summary below — do you still need the full file?
Codebase indexer — walks repo, extracts symbols via tree-sitter
Public: Indexer, IndexEntry, index_project(), reindex_changed()
```

**Session start (project map):**
```
-- youwhatknow: project map --
src/ — Core application logic
  main.rs — Entry point, server startup
  indexer.rs — Codebase indexer, tree-sitter symbol extraction
  server.rs — Axum HTTP server, hook endpoints
  session.rs — Per-session read tracking
  config.rs — Figment-based configuration
```

## Indexing

### Symbol Extraction (tree-sitter)

Initial language support:

| Language | Extensions | Symbols Extracted |
|----------|------------|-------------------|
| Rust | `.rs` | `pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub type` |
| TypeScript | `.ts`, `.tsx` | `export function`, `export class`, `export interface`, `export type`, `export const` |
| JavaScript | `.js`, `.jsx` | `export function`, `export class`, `export const`, `export default` |
| Python | `.py` | Top-level `def`, `class`, `__all__` exports |
| Go | `.go` | Exported (capitalized) `func`, `type`, `var`, `const` |

The indexer defines a `LanguageSupport` trait so adding new languages is straightforward.

### Description Generation (Haiku via claude CLI)

File descriptions are generated by shelling out to `claude` CLI:

- Batch files that need descriptions (10-20 per call)
- For each file in the batch, include the first 100 lines of content plus extracted symbols — not the full file
- Run `claude --dangerously-skip-permissions -m haiku --print` with a prompt asking for a one-line description per file
- Parse response into per-file descriptions
- The server assumes `claude` CLI is available in the environment

**Fallback when `claude` CLI is unavailable:** if the CLI is not on PATH or the call fails, the indexer logs a warning and falls back to deriving descriptions from the filename and extracted symbols (e.g., file `session.rs` with symbols `SessionTracker`, `track_read()` → "Per-session file read tracking"). Symbols and folder structure are still fully indexed — only descriptions degrade.

### File Filtering

Files are skipped during indexing if they match any of:

- Binary files (detected by null bytes in first 512 bytes)
- Files over `max_file_size_kb` (default 100KB)
- Default ignore patterns: `*.min.js`, `*.min.css`, `*.generated.*`, `*.bundle.*`, lock files (`package-lock.json`, `yarn.lock`, `Cargo.lock`, etc.)
- User-configured `ignored_patterns` from config

### Incremental Re-indexing

- `.claude/summaries/.last-run` stores the git commit hash from the last full/incremental index
- On startup, `git diff <last-hash>..HEAD` plus `git diff` (unstaged) plus `git ls-files --others --exclude-standard` (untracked) identifies changed files
- Only the changed files are re-indexed; the containing folder's TOML is regenerated with the updated entries
- If `.last-run` is missing or invalid, fall back to full scan
- A full re-index can be triggered via `POST /reindex`

## Storage Format

TOML files in `.claude/summaries/`. All writes are atomic (write to temp file in the same directory, then rename).

**Note:** the project's `CLAUDE.md` currently references `.claude/summaries/*.md` files. This will be updated to reflect the new TOML format as part of implementation.

### Per-folder summary (e.g., `.claude/summaries/src.toml`)

```toml
generated = "2026-03-21T10:00:00Z"
description = "Core application logic"

[files.main]
path = "src/main.rs"
description = "Entry point, server startup"
symbols = ["main()"]
summarized = "2026-03-21T10:00:00Z"

[files.indexer]
path = "src/indexer.rs"
description = "Codebase indexer, tree-sitter symbol extraction"
symbols = ["Indexer", "IndexEntry", "index_project(), reindex_changed()"]
summarized = "2026-03-21T10:00:00Z"
```

Nested folders use `--` as separator in filenames (e.g., `src--server.toml`).

### Project summary (`.claude/summaries/project-summary.toml`)

```toml
generated = "2026-03-21T10:00:00Z"
last_commit = "94aed7d"

[folders.src]
path = "src/"
description = "Core application logic"

[folders."src--server"]
path = "src/server/"
description = "Axum HTTP server and hook endpoints"
```

## Session Tracking

- In-memory `HashMap<(String, PathBuf), u32>` mapping `(session_id, file_path)` to read count
- Per-session last-activity timestamp tracked in a separate `HashMap<String, Instant>`
- Incremented on each PreToolUse/Read hook call
- Background task runs every 10 minutes, removes sessions with no activity for `session_timeout_minutes`
- Not persisted — resets on server restart

## Server Lifecycle

**Starting:** the server is started manually by the user (e.g., `youwhatknow` or `just serve`). It:
1. Reads config from `.claude/youwhatknow.toml` (or defaults)
2. Attempts to bind to the configured port — exits with an error if the port is already in use
3. Loads existing summaries from disk into memory
4. Spawns background indexing task
5. Begins serving hook requests immediately (files not yet indexed return a "still indexing" response)

**Health check:** `GET /health` returns:
```json
{
  "status": "ok",
  "indexing": true,
  "indexed_files": 42,
  "total_files": 128
}
```

**Stopping:** `Ctrl+C` / SIGTERM. Graceful shutdown — finishes in-flight requests, flushes any pending summary writes.

**Files outside the project:** if a hook request references a file path outside the project root (derived from `cwd`), the server returns `permissionDecision: "allow"` with no `additionalContext`. The read proceeds normally.

## Configuration

`.claude/youwhatknow.toml`:

```toml
port = 7849
summary_path = ".claude/summaries"
ignored_patterns = ["*.generated.*", "*.min.js"]
session_timeout_minutes = 60
max_file_size_kb = 100
```

Overridable via environment variables (figment env provider, prefix `YOUWHATKNOW_`).

## Dependencies

From `Cargo.toml` (already present):
- `tokio` — async runtime
- `serde` / `serde_json` — serialization
- `figment` — configuration
- `tracing` / `tracing-subscriber` — logging
- `eyre` — error handling
- `thiserror` — typed errors

To add:
- `axum` — HTTP server
- `tower-http` — middleware (timeouts, logging)
- `toml` — TOML serialization/deserialization
- `tree-sitter` — parsing framework
- `tree-sitter-rust` — Rust grammar
- `tree-sitter-typescript` — TypeScript/JavaScript grammar
- `tree-sitter-python` — Python grammar
- `tree-sitter-go` — Go grammar
- `chrono` — timestamps

## Non-Goals

- No authentication on the local server (localhost only)
- No multi-project support in a single server instance (one server per project)
- No real-time file watching (re-index on startup and on demand via `/reindex`)
- No blocking of reads — always allow, only inject context
- No token usage monitoring (out of scope for v1; project description will be updated)
