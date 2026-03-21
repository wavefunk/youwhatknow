# youwhatknow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a local HTTP server that integrates with Claude Code hooks to inject file summaries before reads, track re-reads, and provide project structure at session start.

**Architecture:** Axum HTTP server with background Tokio indexing task. Index stored as TOML on disk, loaded into memory for fast lookup. Tree-sitter for symbol extraction, claude CLI for description generation. Session tracking is ephemeral in-memory state.

**Tech Stack:** Rust (nightly-2026-01-05), Tokio, Axum, tree-sitter, Figment, serde, TOML

**Spec:** `docs/superpowers/specs/2026-03-21-youwhatknow-design.md`

---

## File Structure

```
src/
├── main.rs           — Entry point, server startup, signal handling
├── config.rs         — Figment-based configuration
├── types.rs          — Shared types: FolderSummary, FileSummary, HookRequest, HookResponse
├── storage.rs        — TOML read/write, atomic file operations
├── session.rs        — Per-session read count tracking, stale session cleanup
├── server.rs         — Axum router, shared AppState, endpoint wiring
├── hooks.rs          — Hook handler logic: pre-read, session-start, response formatting
└── indexer/
    ├── mod.rs        — Orchestration: full scan, incremental re-index, is_ready flag
    ├── discovery.rs  — git ls-files, file filtering (binary, size, patterns)
    ├── symbols.rs    — tree-sitter symbol extraction, LanguageSupport trait
    └── describe.rs   — Haiku CLI batch description generation, fallback heuristic
```

---

## Milestone 1: Foundation (types, config, storage)

### Task 1: Types and data structures

**Files:**
- Create: `src/types.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Define core types**

`FileSummary` (path, description, symbols, summarized timestamp), `FolderSummary` (generated, description, files map), `ProjectSummary` (generated, last_commit, folders map). Also `HookRequest` and `HookResponse` serde types matching the Claude Code hook contract.

- [ ] **Step 2: Write tests for serde round-trip**

Verify that `FolderSummary` serializes to expected TOML format and deserializes back. Verify `HookRequest` deserializes from the exact JSON Claude Code sends. Verify `HookResponse` serializes to the exact JSON Claude Code expects.

- [ ] **Step 3: Commit**

### Task 2: Configuration

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Define Config struct with Figment**

Fields: `port` (u16, default 7849), `summary_path` (String, default ".claude/summaries"), `ignored_patterns` (Vec<String>), `session_timeout_minutes` (u64, default 60), `max_file_size_kb` (u64, default 100). Load from `.claude/youwhatknow.toml` with env override prefix `YOUWHATKNOW_`.

- [ ] **Step 2: Write tests**

Test default values when no config file exists. Test loading from a TOML file. Test env var override.

- [ ] **Step 3: Commit**

### Task 3: Storage (TOML read/write)

**Files:**
- Create: `src/storage.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement storage functions**

`load_folder_summary(path) -> Result<FolderSummary>`, `save_folder_summary(path, summary) -> Result<()>` (atomic write via temp+rename), `load_project_summary(path) -> Result<ProjectSummary>`, `save_project_summary(path, summary) -> Result<()>`, `load_all_summaries(summary_dir) -> Result<HashMap<String, FolderSummary>>`.

- [ ] **Step 2: Write tests**

Test round-trip write+read for folder summary. Test atomic write doesn't corrupt on concurrent reads. Test loading from a directory with multiple TOML files.

- [ ] **Step 3: Commit**

---

## Milestone 2: Indexer

### Task 4: File discovery

**Files:**
- Create: `src/indexer/mod.rs`, `src/indexer/discovery.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement file discovery**

`discover_files(project_root, config) -> Result<Vec<PathBuf>>` — runs `git ls-files`, filters by binary detection (null bytes in first 512 bytes), size limit, and ignore patterns. Also `discover_changed_files(project_root, last_commit) -> Result<Vec<PathBuf>>` for incremental re-indexing.

- [ ] **Step 2: Write tests**

Test against the youwhatknow repo itself (it has known files). Test filtering skips binary files and files matching ignore patterns.

- [ ] **Step 3: Commit**

### Task 5: Symbol extraction

**Files:**
- Create: `src/indexer/symbols.rs`

- [ ] **Step 1: Implement LanguageSupport trait and Rust extractor**

Trait: `fn extract_symbols(source: &[u8]) -> Vec<String>` + `fn extensions() -> &[&str]`. Implement for Rust: extract `pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub type` names using tree-sitter. Add a dispatcher function `extract_symbols_for_file(path, source) -> Vec<String>` that picks the right extractor by extension.

- [ ] **Step 2: Write tests**

Test Rust extraction against a known Rust source string with pub fns, structs, enums. Test that non-pub items are excluded. Test unknown extensions return empty vec.

- [ ] **Step 3: Add TypeScript, Python, Go extractors**

Same pattern as Rust. TS: export declarations. Python: top-level def/class. Go: capitalized names.

- [ ] **Step 4: Write tests for other languages**

- [ ] **Step 5: Commit**

### Task 6: Description generation

**Files:**
- Create: `src/indexer/describe.rs`

- [ ] **Step 1: Implement batch description generation**

`generate_descriptions(files: &[(PathBuf, &[u8], Vec<String>)]) -> Result<HashMap<PathBuf, String>>` — batches files (10-20 per call), shells out to `claude --dangerously-skip-permissions -m haiku --print`, parses response. Include first 100 lines + symbols per file in prompt.

- [ ] **Step 2: Implement fallback**

`fallback_description(filename: &str, symbols: &[String]) -> String` — derives description from filename and symbols when claude CLI is unavailable.

- [ ] **Step 3: Write tests**

Test fallback description generation. Test batch prompt formatting. (claude CLI call itself tested in integration tests.)

- [ ] **Step 4: Commit**

### Task 7: Index orchestration

**Files:**
- Modify: `src/indexer/mod.rs`

- [ ] **Step 1: Implement Index struct**

Holds `HashMap<PathBuf, FileSummary>` and `HashMap<String, FolderSummary>` and `ProjectSummary`. Methods: `lookup_file(path) -> Option<&FileSummary>`, `lookup_folder(path) -> Option<&FolderSummary>`, `project_map() -> String` (formatted project map text), `is_ready() -> bool`.

- [ ] **Step 2: Implement full_index and incremental_index**

`full_index(project_root, config) -> Result<Index>` — discover files, extract symbols, generate descriptions, build folder/project summaries, save to disk, return index. `incremental_index(project_root, config, existing: &mut Index) -> Result<()>` — discover changed files, re-index only those, update affected folder TOMLs.

- [ ] **Step 3: Write tests**

Test `project_map()` formatting. Test `lookup_file` returns correct summary. Test that `incremental_index` only re-indexes changed files.

- [ ] **Step 4: Commit**

---

## Milestone 3: Server

### Task 8: Session tracker

**Files:**
- Create: `src/session.rs`

- [ ] **Step 1: Implement SessionTracker**

`SessionTracker` wrapping `Arc<RwLock<...>>` with methods: `track_read(session_id, file_path) -> u32` (returns new count), `spawn_cleanup_task(timeout_minutes)` (background tokio task every 10 min).

- [ ] **Step 2: Write tests**

Test that track_read increments correctly. Test that consecutive reads for same file return increasing counts. Test that different sessions are independent.

- [ ] **Step 3: Commit**

### Task 9: Hook handlers

**Files:**
- Create: `src/hooks.rs`

- [ ] **Step 1: Implement handler functions**

`handle_pre_read(index, session_tracker, request) -> HookResponse` — looks up file, tracks read count, formats response with summary and re-read warning. `handle_session_start(index, request) -> HookResponse` — returns project map. Handle edge cases: file outside project, file not indexed, indexing in progress.

- [ ] **Step 2: Write tests**

Test first-read response format. Test re-read response includes warning and count. Test file outside project returns allow with no context. Test indexing-in-progress response. Test session-start response includes project map.

- [ ] **Step 3: Commit**

### Task 10: HTTP server and main

**Files:**
- Create: `src/server.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement Axum server**

`AppState` holding `Arc<RwLock<Index>>`, `SessionTracker`, `Config`. Routes: `POST /hook/pre-read`, `POST /hook/session-start`, `POST /reindex`, `GET /health`. Wire up handlers from `hooks.rs`.

- [ ] **Step 2: Implement main**

Load config, load existing summaries, build initial index, spawn background indexer, start HTTP server with graceful shutdown on SIGTERM.

- [ ] **Step 3: Write integration tests**

Spawn the server in a test, send hook requests via HTTP, verify responses match expected format. Test health endpoint. Test pre-read with known indexed file. Test re-read warning.

- [ ] **Step 4: Add `serve` command to justfile**

- [ ] **Step 5: Commit**
