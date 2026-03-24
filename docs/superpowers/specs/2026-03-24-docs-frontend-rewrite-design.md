# Design Spec: Docs & Frontend Rewrite

**Date:** 2026-03-24
**Status:** Reviewed
**Goal:** Rewrite the youwhatknow landing page and documentation to reflect the actual deny-first behavior, add missing features, and extract shared CSS/JS into common files.

---

## File Structure

```
docs/
├── css/
│   └── style.css          # shared design system
├── js/
│   └── main.js            # shared JS (intersection observer, utilities)
├── index.html             # landing page
├── docs.html              # reference docs
├── logo.svg               # unchanged
└── CNAME                  # unchanged
```

### Shared CSS (`css/style.css`)

Contains everything both pages use:
- CSS variables (colors, fonts)
- Reset and body styles
- Typography (headings, links, muted text)
- Nav and footer components
- Code block / terminal styling
- Section labels, callouts
- Config tables
- Animations (fadeSlideUp, pulse, blink, reveal)
- Responsive breakpoints
- Scanline / noise overlays

No `<style>` blocks in either HTML file.

### Shared JS (`js/main.js`)

- IntersectionObserver for `.reveal` elements
- Any shared utilities

Page-specific JS (terminal demo) stays inline in index.html.

---

## Landing Page (index.html)

### Hero

- Badge: "A Claude Code hook server"
- Logo SVG (unchanged)
- Title: *you what (k)now?*
- Tagline: *"Wait, are you reading that file again?"*
- Sub copy: Same fridge metaphor, reframed for deny-first — "Instead of 2000 lines of code, Claude gets a summary. If it still wants the full file, it has to ask twice."
- CTA: "Make it stop" → #setup
- Links: Docs, GitHub

### Terminal Demo

Rewritten to show the actual deny-first flow:

1. `claude Read src/server.rs` (623 lines)
2. youwhatknow **denies** — shows summary: description, public symbols, line-range map
3. "If this summary is sufficient, do not read the file."
4. Claude decides the summary is enough, moves on to another file
5. Later: `claude Read src/server.rs` — allowed clean (second read)
6. Later: `claude Read src/server.rs` — allowed with nudge: "read 3x this session, consider offset/limit"

### The Problem (conversation bubbles)

Same humor, same structure. Updated punchline:
- Claude reads main.rs
- Claude tries to read main.rs again 30s later
- You: "you literally just read that"
- youwhatknow **denies the read**, shows summary
- Claude: "...okay, the summary is enough. Thanks."

The key difference: youwhatknow blocks the read instead of just commenting.

### How It Works — 6 steps

1. **Claude reaches for a file** — PreToolUse hook fires before every Read. youwhatknow gets the file path and session ID via HTTP.
   `POST /hook/pre-read`

2. **Small files get a free pass** — Files with 30 lines or fewer, or reads with offset/limit, go through without intervention.
   `line_threshold = 30`

3. **First read: denied with a summary** — Instead of 2000 lines, Claude sees: file description, public symbols, line-range map, and an instruction: "If this is sufficient, do not read the file. Read again for the full file."
   This is the core mechanism. Claude has to prove it needs the full content.

4. **Second read: allowed clean** — Claude asked twice, so it genuinely needs the file. Goes through with no context injection.

5. **Repeat offenders get nudged** — Third read and beyond: allowed, but with a reminder — "This file has been read N times this session. Consider using offset/limit for targeted reads."

6. **Day-one orientation** — On SessionStart, Claude gets a full project map plus instructions on how youwhatknow works. No more "let me explore the codebase" spirals.
   `POST /hook/session-start`

### Working Set Eviction — new section

- After more than 40 other file reads (41+), a file's read count resets to 0
- Next read shows the summary again (fresh deny)
- Keeps Claude's working set current without manual intervention
- Configurable via `eviction_threshold` in project config

### Architecture

Same diagram concept — still accurate:
- Multiple Claude sessions + subagents → HTTP hooks → single daemon on localhost:7849
- Daemon routes by cwd → per-project indexes
- Indexes built with tree-sitter + haiku descriptions, stored as TOML

### Benefits Grid (6 items)

1. **Summary first, file second** — Claude gets description, symbols, and line ranges. Has to ask twice for the full file. Less context waste.
2. **Working set eviction** — After 41+ intervening file reads, stale counts reset automatically. No manual cleanup.
3. **"You already read that"** — Per-session tracking nudges Claude on 3rd+ reads to use offset/limit.
4. **Stupid simple setup** — One command: `youwhatknow setup`. Hooks, daemon, indexing — all handled.
5. **All projects, one process** — The daemon loads project indexes lazily. First request for a new project triggers background indexing.
6. **Invisible when off** — Daemon not running? Claude works normally. HTTP hooks fail silently.

### Setup Section

**Install (pick one):**
```sh
# Installer script (pre-built binaries)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wavefunk/youwhatknow/releases/latest/download/youwhatknow-installer.sh | sh

# Nix flake (see docs for details)
# Build from source
cargo build --release
```

**Then:**
```sh
cd your-project
youwhatknow setup
```

Variants: `--shared`, `--no-index`. Show "other useful commands": `status`, `summary`, `reset`.

Collapsible: manual hook setup (same as current).
Collapsible: optional config.toml.

### Footer

Same: logo, links (docs, GitHub), "Built with Rust, Tokio, Axum, and tree-sitter.", quip.

---

## Docs Page (docs.html)

### Nav

Same layout. Links: Home, Docs (active), GitHub.

### Header

Title: "Documentation"
Sub: "Everything you need to install, configure, and understand youwhatknow."

### Table of Contents

1. Installation
2. Quickstart
3. CLI Reference
4. Hook Behavior
5. Working Set Eviction
6. Daemon Configuration
7. Project Configuration
8. API Endpoints
9. Indexing & Symbols
10. Storage Format
11. Environment Variables
12. Lifecycle & PID
13. Nix Integration

### Installation (new section)

Three methods:

**Installer script (recommended):**
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wavefunk/youwhatknow/releases/latest/download/youwhatknow-installer.sh | sh
```
Pre-built binaries for macOS and Linux via cargo-dist.

**Nix flake:**
```nix
inputs.youwhatknow.url = "github:wavefunk/youwhatknow";
# In devShell packages:
packages = [ youwhatknow.packages.${system}.default ];
```
See Nix Integration section for shell hook details.

**Build from source:**
```sh
cargo build --release
```
Requires Rust 2024 edition. A Nix flake is included for reproducible builds: `nix build`.

### Quickstart

Same as current — `youwhatknow setup`, what it does, variants, callout about graceful degradation.

### CLI Reference

**`youwhatknow` / `youwhatknow serve`** — starts daemon on localhost:7849.

**`youwhatknow setup [--shared|--local] [--no-index]`** — one-command project setup.
- `--local` (default; flag exists for explicitness) → `.claude/settings.local.json`
- `--shared` → `.claude/settings.json`
- `--no-index` → skip initial indexing
- Merges hooks, preserves existing non-youwhatknow hooks.

**`youwhatknow status`** — daemon status. Show actual output: pid, port, uptime, idle time, sessions, projects, idle_shutdown_minutes.

**`youwhatknow summary <path>`** — preview a file's summary. If a session is active (via `$YOUWHATKNOW_SESSION`), primes the read count so the next Read hook allows the file through without a deny.

**`youwhatknow init`** — SessionStart hook handler (called automatically). Reads hook payload from stdin, writes session_id to `$CLAUDE_ENV_FILE`, ensures daemon is running, proxies to `/hook/session-start`.

**`youwhatknow reset <path> [--session <id>]`** (new) — resets read count for a file to 0. Next read shows the summary again. Uses `$YOUWHATKNOW_SESSION` or `--session` override.

### Hook Behavior (rewritten)

Core section. Explains the deny-first mechanism:

**PreToolUse / Read:**

Early exits (always allow, no tracking):
- No tool_input
- File outside project
- No summary in index
- Targeted read (offset or limit present)
- File at or under `line_threshold` (default 30)

Read count behavior:
- **Count 1 → DENY with summary.** Claude sees: file description, symbols, line-range map, and "If this summary is sufficient, do not read the file. If you need the full file contents, read it again."
- **Count 2 → ALLOW clean.** No context injection. Claude proved it needs the file.
- **Count 3+ → ALLOW with nudge.** "This file has been read N times this session. Consider using offset/limit for targeted reads."

**SessionStart:**
- Injects full project map (folder/file hierarchy with descriptions)
- Includes session instructions: line threshold, how to use `youwhatknow summary`, offset/limit tips
- Notes if indexing is in progress

**"What Claude sees" examples** — actual format:

First read (deny). The deny reason contains the full summary wrapped with a header and instruction:
```
-- youwhatknow: src/server.rs --
src/server.rs (245 lines) — Axum server with activity tracking and idle shutdown
Public: create_router, start_server, AppState

  1-45    imports and types
  46-120  router setup and middleware
  121-200 hook endpoint handlers
  201-245 health and status endpoints

Read specific sections with offset/limit, or read again for the full file.
If this summary is sufficient, do not read the file. If you need the full file contents, read it again.
```

Third+ read (allow with nudge):
```
This file has been read 3 times this session. Consider using offset/limit for targeted reads.
```

Session start:
```
-- youwhatknow: project map --
src/ — Core hook server implementation with CLI, daemon, server, and indexing
  cli.rs — CLI command handlers for daemon and summary management
  config.rs — System and per-project configuration loading via Figment
  ...

-- youwhatknow: instructions --
Files over 30 lines show a summary on first read. Read again for the full file, or use offset/limit.
To preview any file without triggering a read: run `youwhatknow summary <path>` in the terminal.
```

Callout: "The deny is soft — Claude can always read the file by trying again. youwhatknow just makes it consider the summary first."

### Working Set Eviction (new section)

- Sequence-based, not time-based
- Each file read increments a per-session monotonic sequence counter
- When `current_seq - file_last_seq > eviction_threshold` (default 40), the file's read count resets to 0
- Next read shows the summary again (fresh deny)
- This keeps Claude's working set current: files it hasn't touched in more than 40 reads fade out of the "already read" state
- Configurable via `eviction_threshold` in `.claude/youwhatknow.toml`
- `youwhatknow reset <path>` does the same thing manually

### Daemon Configuration

`~/.config/youwhatknow/config.toml` (all optional, sensible defaults):

| Key | Default | Description |
|-----|---------|-------------|
| port | 7849 | TCP port the daemon listens on. Must match the URL in hook config. |
| session_timeout_minutes | 60 | Minutes of inactivity before a session's read tracking data is cleaned up. |
| idle_shutdown_minutes | 30 | Minutes with zero incoming requests before daemon self-shuts-down. Set to 0 to disable. |

Callout: "You probably don't need this file. The defaults are sensible."

### Project Configuration

Updated table — add missing fields:

| Key | Default | Description |
|-----|---------|-------------|
| summary_path | ".claude/summaries" | Where TOML summary files are stored |
| max_file_size_kb | 100 | Files larger than this skipped during indexing |
| line_threshold | 30 | Files at or under this many lines pass through without summary |
| ignored_patterns | [] | Additional glob patterns to skip |
| max_concurrent_batches | 4 | Parallel indexing batch count |
| eviction_threshold | 40 | Sequence distance before a file's read count resets |

Built-in ignore patterns: same as current.

### API Endpoints

Updated — add missing endpoints:

- `POST /hook/pre-read` — PreToolUse hook handler (same)
- `POST /hook/session-start` — SessionStart hook handler (same)
- `POST /hook/summary` (new) — CLI summary fetch. JSON body with `cwd` (required), `file_path` (required), `session_id` (optional). Returns rendered summary as plain text.
- `POST /hook/reset-read` (new) — CLI reset read count. JSON body with `session_id` (required), `cwd` (required), `file_path` (required). Returns 400 without session_id.
- `POST /reindex` — same
- `GET /health` — same
- `GET /status` — same. Note: intentionally does NOT touch activity timer, so polling status won't prevent idle shutdown.

### Everything Else

Indexing & Symbols, Storage Format, Environment Variables, Lifecycle & PID, Nix Integration — same content as current docs, still accurate. No changes needed.

### Footer

Same as landing page.

---

## Design System Notes

- Keep the existing color scheme: `--bg: #0a0a0c`, `--glow: #39ff85`, `--amber: #ffb830`, `--red: #ff4f4f`
- Keep the font stack: JetBrains Mono, Space Mono, Instrument Serif
- Keep the dark terminal aesthetic, scanline overlay, noise texture
- Keep the animations: fadeSlideUp, reveal on scroll, terminal typing effect
- Keep the conversation bubble component
- The design quality matters — this is a portfolio piece
