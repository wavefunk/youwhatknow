# youwhatknow v2 — Line-Gated Reads

**Date:** 2026-03-22
**Status:** Design approved, pending implementation

## Problem

The current design blocks every first file read with a summary, regardless of file size. This is wasteful — small files should just be read directly. Large files (which consume significant context) get no special treatment beyond the same summary every other file gets. Claude has no way to make targeted reads into specific sections of a file.

## Solution

Replace the blanket first-read gate with a **line-count threshold**. Files over the threshold (default: 30 lines) get denied on first read with a structured summary that includes a **line-range map** — a tree-sitter-derived table of what's on which lines. Claude can then make targeted reads with `offset`/`limit` instead of loading the entire file.

A new `youwhatknow summary <path>` CLI command lets Claude explicitly request a summary without going through the Read hook, and it counts toward the file's read tracking so the next actual Read passes through.

## Design Decisions

- **Line count, not token count.** Token estimation is unreliable and model-dependent. Line count is deterministic, fast, and configurable.
- **Tree-sitter for structure, Haiku for prose.** Line ranges come from AST parsing (free, fast, deterministic). File/folder descriptions come from Haiku CLI (richer, but costs API calls). No overlap.
- **Approach C: incremental refactor with summary formatter.** Existing daemon lifecycle, registry, discovery, symbol extraction, and Haiku description generation all carry over. A new `summary.rs` module owns all human-readable rendering, preventing duplication between hook logic and CLI output.
- **Session tracking via `CLAUDE_ENV_FILE`.** The SessionStart hook writes `YOUWHATKNOW_SESSION=<session_id>` to `$CLAUDE_ENV_FILE`, making it available to all subsequent Bash commands. The CLI `summary` command reads this env var and passes it to the daemon for accurate per-session count tracking. Subagents get their own SessionStart, own session ID, own counts.
- **Hook `allow` does not bypass user permissions.** The hook's "allow" only means "the hook has no objection" — the normal Claude Code permission flow continues. A hook "deny" blocks before permissions are checked. This means youwhatknow never escalates privileges.

## Data Model

### `LineRange` (new)

```rust
struct LineRange {
    start: u32,   // 1-indexed line number
    end: u32,
    label: String, // e.g., "AppState struct", "handle_pre_read() function"
}
```

### `FileSummary` (modified)

```rust
struct FileSummary {
    path: PathBuf,
    description: String,         // Haiku-generated (unchanged)
    symbols: Vec<String>,        // tree-sitter public symbols (unchanged)
    line_count: u32,             // NEW — total lines in file
    line_ranges: Vec<LineRange>, // NEW — top-level AST sections
    summarized: DateTime<Utc>,
}
```

### `FileAnalysis` (new, internal to indexer)

```rust
struct FileAnalysis {
    symbols: Vec<String>,
    line_ranges: Vec<LineRange>,
    line_count: u32,
}
```

Replaces the current separate `extract_symbols` call. One parse, two extractions.

### `ToolInput` (modified)

```rust
struct ToolInput {
    file_path: PathBuf,
    offset: Option<u32>,  // NEW
    limit: Option<u32>,   // NEW
}
```

### `ProjectConfig` (modified)

```rust
struct ProjectConfig {
    // ... existing fields ...
    line_threshold: u32, // NEW — default: 30
}
```

## Hook Logic

### PreToolUse handler for Read

```
handle_pre_read(index, session, project_root, config, request):
  1. File outside project              → allow, no context
  2. No summary available              → allow, no context
  3. Has offset or limit in tool_input → allow, no context (targeted read)
  4. File line_count ≤ line_threshold  → allow, no context
  5. Read count == 0 (first encounter) → deny + rendered summary with line-range map
  6. Read count == 1 (second)          → allow, clean pass
  7. Read count ≥ 2 (third+)          → allow + additionalContext with read count nudge
```

Key behaviors:
- Targeted reads (offset/limit present) always pass through — Claude already knows what section it wants.
- Files under the threshold always pass through — no interference for small files.
- Second read is clean — Claude has the full file, doesn't need the line-range map attached.
- Third+ read gets a nudge via `additionalContext` — "this file has been read N times this session."

## Summary Formatter

### New module: `summary.rs`

Three rendering functions, all pure (data in, string out):

**`render_file_summary(file_summary, config) -> String`**

```
src/server.rs (227 lines) — Axum HTTP server, routing, activity tracking
Public: ActivityTracker, AppState, router()

  1-67   ActivityTracker — idle timeout, touch/check
  69-76  AppState struct
  78-85  Router setup — 4 routes
  87-125 Handlers: pre_read, session_start, reindex, health
  126-227 Tests

Read specific sections with offset/limit, or read again for the full file.
```

- Header: path, line count, Haiku description
- Symbols line (if any)
- Line-range map (if any; omitted for unsupported languages)
- Footer instruction

**`render_project_map(index) -> String`**

Replaces the current `Index::project_map()` method. Same format:

```
src/ — Core logic
  main.rs — Entry point
  config.rs — Configuration loading
```

**`render_session_instructions(config) -> String`**

```
Files over 30 lines show a summary on first read. Read again for the full file, or use offset/limit to target specific sections.
To preview any file without triggering a read: run `youwhatknow summary <path>` in the terminal.
```

## CLI Changes

### New subcommand: `summary`

```
youwhatknow summary <path>
```

- Takes a file path relative to cwd
- Reads `$YOUWHATKNOW_SESSION` from env for session attribution
- Sends `POST /hook/summary` to daemon with `{ session_id, cwd, file_path }`
- Daemon renders the summary, increments read count to 1 (if session provided), returns rendered text
- CLI prints text to stdout
- Auto-starts daemon if not running (same as `init`)

### Modified subcommand: `init`

Added step: before proxying to daemon, parse `session_id` from stdin payload and write `YOUWHATKNOW_SESSION=<session_id>` to `$CLAUDE_ENV_FILE` (if that env var is present).

## New Daemon Endpoint

### `POST /hook/summary`

Request body:

```json
{
    "session_id": "optional-session-id",
    "cwd": "/path/to/project",
    "file_path": "src/server.rs"
}
```

Response: plain text rendered summary.

- If session_id is present: increment read count, return summary
- If session_id is absent: log warning, return summary without count tracking
- If no summary available: return message suggesting reindex, exit 0

## SessionStart Context

The daemon's SessionStart response includes `additionalContext` with:

1. Project map (via `render_project_map`)
2. Instructions (via `render_session_instructions`)
3. If indexing in progress: `"(indexing in progress — some summaries may be incomplete)"`

Wrapped with headers:

```
-- youwhatknow: project map --
<project map>

-- youwhatknow: instructions --
<instructions>
```

## Line-Range Extraction

### Refactored `symbols.rs`

`extract_symbols` and new `extract_line_ranges` merged into a single `analyze_file` function:

```rust
pub fn analyze_file(path: &Path, source: &[u8]) -> FileAnalysis
```

One tree-sitter parse, two extractions. `FileAnalysis` contains symbols, line ranges, and line count.

### Line-range extraction rules (Rust example):

| AST node | Label format |
|---|---|
| `fn` / `pub fn` | `"main() function"` |
| `struct` | `"AppState struct"` |
| `enum` | `"Command enum"` |
| `impl` | `"impl Index"` |
| `trait` | `"trait Foo"` |
| `mod` | `"mod indexer"` |
| `mod tests` | `"Tests"` |
| Consecutive `use` | Collapsed to `"Imports"` |

For unsupported languages: `line_ranges` is empty. Summary still shows description + symbols.

## Warning & Error Behavior

| Scenario | Behavior |
|---|---|
| Request without session_id | `warn!` log, serve response, skip count tracking |
| CLI `summary` without `$YOUWHATKNOW_SESSION` | Works, returns summary, no count increment |
| CLI `summary` for file with no summary | Return message suggesting reindex, exit 0 |
| CLI `summary` for file under threshold | Return summary anyway (threshold only gates Read hooks) |
| Hook fires, no summary available | Allow the read, no context |
| Hook fires, summary exists but no line ranges | Deny + summary without line-range section |
| Daemon not running for CLI commands | Auto-start (existing behavior) |

## What Changes, What Stays

### Unchanged
- `daemon.rs` — lifecycle, PID, idle shutdown, signals
- `registry.rs` — project registry, worktree sharing, lazy loading
- `storage.rs` — TOML read/write, atomic file ops, folder keys (format extends, doesn't break)
- `indexer/discovery.rs` — git file discovery, change detection, binary check
- `indexer/describe.rs` — Haiku CLI batching for descriptions

### Modified
- `types.rs` — `FileSummary` gains `line_count`, `line_ranges`; `ToolInput` gains `offset`, `limit`; add `LineRange`, `FileAnalysis`
- `hooks.rs` — new gating logic, uses `summary.rs` for rendering
- `server.rs` — new `/hook/summary` endpoint
- `main.rs` — add `Summary` subcommand
- `cli.rs` — add `summary()`, update `init()` for `CLAUDE_ENV_FILE`
- `indexer/symbols.rs` — refactor to `analyze_file()` returning `FileAnalysis`
- `indexer/mod.rs` — use `FileAnalysis`, store `line_count` and `line_ranges`
- `config.rs` — add `line_threshold` to `ProjectConfig`

### New
- `summary.rs` — `render_file_summary()`, `render_project_map()`, `render_session_instructions()`

### Deleted
- `format_summary()` in `hooks.rs` — replaced by `summary.rs`
