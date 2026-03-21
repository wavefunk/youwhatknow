# Parallel Async Indexing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make description generation parallel and async, add folder descriptions, add progress logging throughout indexing.

**Architecture:** Convert `describe.rs` from sync sequential batching to async parallel batching using `tokio::process::Command` and `futures::stream::buffered`. Add a second pass for folder-level descriptions. Add structured progress logging at info (batch-level) and trace (file-level) throughout the pipeline.

**Tech Stack:** tokio::process, futures (stream::buffered)

**Notes:**
- `read_preview` is intentionally left as blocking `std::fs::read_to_string` — it reads at most 100 lines of a single file, bounded and fast. Not worth async conversion.
- We use `buffered` (ordered) not `buffer_unordered` so results stay aligned with input batches for fallback handling.

---

### Task 1: Add `futures` dependency and `max_concurrent_batches` config

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/config.rs`

- [ ] **Step 1: Add futures to Cargo.toml**

In `Cargo.toml`, add to `[dependencies]`:
```toml
futures = "0.3"
```

- [ ] **Step 2: Add `max_concurrent_batches` to `ProjectConfig`**

In `src/config.rs`, add to `ProjectConfig`:
```rust
#[serde(default = "default_max_concurrent_batches")]
pub max_concurrent_batches: usize,
```

Add default function:
```rust
fn default_max_concurrent_batches() -> usize {
    4
}
```

Update `Default` impl to include:
```rust
max_concurrent_batches: default_max_concurrent_batches(),
```

- [ ] **Step 3: Add test for the new config field**

In `src/config.rs` tests:
```rust
#[test]
fn default_project_config_has_concurrent_batches() {
    let config = ProjectConfig::default();
    assert_eq!(config.max_concurrent_batches, 4);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test config`
Expected: all config tests pass including the new one.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/config.rs
git commit -m "feat: add max_concurrent_batches to ProjectConfig"
```

---

### Task 2: Convert all of `describe.rs` to async with parallel batching

This task converts `generate_batch`, `claude_available`, and `generate_descriptions` to async in a single atomic change, along with updating the call site in `mod.rs`. This avoids compilation gaps where an async callee has a sync caller.

**Files:**
- Modify: `src/indexer/describe.rs`
- Modify: `src/indexer/mod.rs` (call site only)

- [ ] **Step 1: Convert `generate_batch` to async**

Replace `std::process::Command` with `tokio::process::Command`. New signature:
```rust
async fn generate_batch(
    project_root: &Path,
    batch: &[(PathBuf, Vec<String>)],
    batch_num: usize,
    total_batches: usize,
) -> eyre::Result<HashMap<PathBuf, String>>
```

Add logging at function start:
```rust
tracing::info!(
    batch = batch_num,
    total = total_batches,
    files = batch.len(),
    "starting batch"
);
```

Add trace-level per-file logging inside the prompt-building loop:
```rust
tracing::trace!(batch = batch_num, file = %path.display(), "describing file");
```

Replace the `Command` spawn block with:
```rust
let mut child = tokio::process::Command::new("claude")
    .args([
        "--dangerously-skip-permissions",
        "--model",
        "haiku",
        "--print",
    ])
    .current_dir(project_root)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;

if let Some(mut stdin) = child.stdin.take() {
    tokio::io::AsyncWriteExt::write_all(&mut stdin, prompt.as_bytes()).await?;
}

let output = child.wait_with_output().await?;
```

Add at function end (before `Ok`):
```rust
tracing::info!(
    batch = batch_num,
    total = total_batches,
    described = descriptions.len(),
    "completed batch"
);
```

- [ ] **Step 2: Convert `claude_available` to async**

```rust
async fn claude_available() -> bool {
    tokio::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}
```

- [ ] **Step 3: Convert `generate_descriptions` to async with parallel batching**

New signature:
```rust
pub async fn generate_descriptions(
    project_root: &Path,
    files: &[(PathBuf, Vec<String>)],
    concurrency: usize,
) -> HashMap<PathBuf, String>
```

Clamp concurrency: `let concurrency = concurrency.max(1);`

Replace the sequential batch loop with:
```rust
use futures::stream::{self, StreamExt};

if !claude_available().await {
    tracing::warn!("claude CLI not found; using fallback descriptions");
    // ... existing fallback loop unchanged ...
    return descriptions;
}

let total_batches = files.chunks(15).len();
tracing::info!(
    files = files.len(),
    batches = total_batches,
    concurrency,
    "generating file descriptions"
);

let results: Vec<_> = stream::iter(
    files.chunks(15).enumerate().map(|(i, batch)| {
        let project_root = project_root.to_owned();
        let batch = batch.to_vec();
        async move {
            generate_batch(&project_root, &batch, i + 1, total_batches).await
        }
    })
)
.buffered(concurrency)
.collect()
.await;

let mut descriptions = HashMap::new();
for (result, batch) in results.into_iter().zip(files.chunks(15)) {
    match result {
        Ok(batch_descs) => {
            descriptions.extend(batch_descs);
        }
        Err(e) => {
            tracing::warn!(error = %e, "batch failed; using fallbacks");
            for (path, symbols) in batch {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                descriptions
                    .entry(path.clone())
                    .or_insert_with(|| fallback_description(filename, symbols));
            }
        }
    }
}

tracing::info!(
    described = descriptions.len(),
    files = files.len(),
    "file descriptions complete"
);
```

- [ ] **Step 4: Remove `use std::process::Command`**

The only remaining `std::process` usage is `Stdio` (for piped/null). Keep that import.

- [ ] **Step 5: Update call site in `mod.rs`**

In `src/indexer/mod.rs`, change line ~288 from:
```rust
let descriptions =
    describe::generate_descriptions(project_root, &file_symbols);
```
To:
```rust
let descriptions = describe::generate_descriptions(
    project_root,
    &file_symbols,
    config.max_concurrent_batches,
).await;
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check`
Expected: compiles clean.

- [ ] **Step 7: Run existing tests**

Run: `cargo test describe`
Expected: all 5 existing tests pass (they test pure sync functions: `fallback_description`, `capitalize_first`).

- [ ] **Step 8: Commit**

```bash
git add src/indexer/describe.rs src/indexer/mod.rs
git commit -m "feat: parallel async file description generation with progress logging"
```

---

### Task 3: Add `generate_folder_descriptions`

**Files:**
- Modify: `src/indexer/describe.rs`

- [ ] **Step 1: Add `fallback_folder_description`**

```rust
fn fallback_folder_description(folder_path: &str) -> String {
    folder_path.to_owned()
}
```

- [ ] **Step 2: Add `generate_folder_batch` function**

```rust
async fn generate_folder_batch(
    project_root: &Path,
    batch: &[(String, Vec<String>)],  // (folder_path, file_descriptions)
    batch_num: usize,
    total_batches: usize,
) -> eyre::Result<HashMap<String, String>>
```

Build the prompt:
```rust
let mut prompt = String::from(
    "For each folder below, write exactly one short description (max 12 words) \
     summarizing its purpose based on the files it contains. \
     Output format: one line per folder as `FOLDER_PATH: description`. Nothing else.\n\n",
);

for (folder_path, file_descs) in batch {
    tracing::trace!(batch = batch_num, folder = folder_path, "describing folder");
    prompt.push_str(&format!("FOLDER: {folder_path}\nFILES:\n"));
    for desc in file_descs {
        prompt.push_str(&format!("- {desc}\n"));
    }
    prompt.push_str("---\n");
}
```

Logging at start/end:
```rust
tracing::info!(batch = batch_num, total = total_batches, folders = batch.len(), "starting folder batch");
// ... spawn claude, parse output ...
tracing::info!(batch = batch_num, total = total_batches, described = descriptions.len(), "completed folder batch");
```

Spawn `claude` CLI identically to `generate_batch` (stdin pipe, async wait).

Parse output — same `split_once(':')` pattern but into `HashMap<String, String>`:
```rust
for line in stdout.lines() {
    if let Some((path_str, desc)) = line.split_once(':') {
        let path_str = path_str.trim();
        let desc = desc.trim();
        if !path_str.is_empty() && !desc.is_empty() {
            descriptions.insert(path_str.to_owned(), desc.to_owned());
        }
    }
}
```

Fill fallbacks for missing folders:
```rust
for (folder_path, _) in batch {
    descriptions
        .entry(folder_path.clone())
        .or_insert_with(|| fallback_folder_description(folder_path));
}
```

- [ ] **Step 3: Add `generate_folder_descriptions` function**

```rust
pub async fn generate_folder_descriptions(
    project_root: &Path,
    folders: &[(String, Vec<String>)],
    concurrency: usize,
) -> HashMap<String, String>
```

Same structure as `generate_descriptions`:
- `claude_available().await` check, fallback early return
- Clamp: `let concurrency = concurrency.max(1);`
- Batch into chunks of 10 (folders are lighter than files)
- `stream::iter(...).buffered(concurrency).collect().await`
- Zip results with batches for fallback on errors
- Info logging at start/end

- [ ] **Step 4: Add test for `fallback_folder_description`**

```rust
#[test]
fn fallback_folder_description_returns_path() {
    let desc = fallback_folder_description("src/indexer");
    assert_eq!(desc, "src/indexer");
}
```

- [ ] **Step 5: Verify it compiles and tests pass**

Run: `cargo check && cargo test describe`
Expected: compiles, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/indexer/describe.rs
git commit -m "feat: add parallel async folder description generation"
```

---

### Task 4: Wire folder descriptions into `index_files` and add progress logging

**Files:**
- Modify: `src/indexer/mod.rs`

The key challenge: the existing `index_files` holds a `folders` write lock (line 333) across the folder grouping and disk-save loop. We need to restructure to:
1. Group files into folders (write lock)
2. Drop the write lock
3. Collect folder inputs (read lock)
4. Generate folder descriptions (no lock held)
5. Update folder descriptions + save to disk (write lock)

- [ ] **Step 1: Add progress logging to symbol extraction**

Wrap the existing symbol extraction loop:
```rust
tracing::info!(files = files.len(), "extracting symbols");
for rel_path in files {
    tracing::trace!(file = %rel_path.display(), "extracting symbols");
    let abs_path = project_root.join(rel_path);
    let source = match std::fs::read(&abs_path) {
        Ok(s) => s,
        Err(_) => continue,
    };
    let syms = symbols::extract_symbols(rel_path, &source);
    file_symbols.push((rel_path.clone(), syms));
    self.inner.indexed_count.fetch_add(1, Ordering::Relaxed);
}
tracing::info!(files = file_symbols.len(), "symbol extraction complete");
```

- [ ] **Step 2: Restructure `index_files` lock scoping**

Replace the existing single block that holds the folders write lock with separate scoped blocks.

**Phase 1 — group files into folders (write lock, then drop):**
```rust
// Group into folders and update in-memory file index
{
    let mut folders = self.inner.folders.write().await;
    for (rel_path, syms) in &file_symbols {
        // ... existing FileSummary construction ...
        // ... existing folder grouping ...
        // Insert into folders map (but don't save to disk yet)
        let folder = folders.entry(key.clone()).or_insert_with(|| {
            let folder_path = storage::key_to_folder(&key);
            FolderSummary {
                generated: now,
                description: folder_path,
                files: HashMap::new(),
            }
        });
        folder.generated = now;
        for (file_key, summary) in new_files_for_folder {
            folder.files.insert(file_key.clone(), summary.clone());
        }
    }
} // write lock dropped here
```

**Phase 2 — collect folder inputs (read lock):**
```rust
let folder_inputs: Vec<(String, Vec<String>)> = {
    let folders = self.inner.folders.read().await;
    folders.iter().map(|(key, folder)| {
        let file_descs: Vec<String> = folder.files.values()
            .map(|f| {
                let name = f.path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                format!("{name}: {}", f.description)
            })
            .collect();
        (storage::key_to_folder(key), file_descs)
    }).collect()
}; // read lock dropped
```

**Phase 3 — generate folder descriptions (no lock):**
```rust
let folder_descs = describe::generate_folder_descriptions(
    project_root,
    &folder_inputs,
    config.max_concurrent_batches,
).await;
```

**Phase 4 — update descriptions + save to disk (write lock):**
```rust
{
    let mut folders = self.inner.folders.write().await;

    // Apply generated descriptions
    for (key, folder) in folders.iter_mut() {
        let folder_path = storage::key_to_folder(key);
        if let Some(desc) = folder_descs.get(&folder_path) {
            folder.description = desc.clone();
        }
    }

    // Save folder summaries to disk
    let summary_dir = project_root.join(&config.summary_path);
    for (key, folder) in folders.iter() {
        let path = summary_dir.join(format!("{key}.toml"));
        if let Err(e) = storage::save_folder_summary(&path, folder) {
            tracing::warn!("failed to save {}: {e}", path.display());
        }
    }

    // Build and save project summary
    let commit = discovery::current_commit(project_root).unwrap_or_default();
    let project = ProjectSummary {
        generated: now,
        last_commit: commit.clone(),
        folders: folders.iter().map(|(key, folder)| {
            (key.clone(), FolderEntry {
                path: format!("{}/", storage::key_to_folder(key)),
                description: folder.description.clone(),
            })
        }).collect(),
    };

    *self.inner.project.write().await = Some(project.clone());

    let project_path = summary_dir.join("project-summary.toml");
    if let Err(e) = storage::save_project_summary(&project_path, &project) {
        tracing::warn!("failed to save project summary: {e}");
    }

    if let Err(e) = storage::write_last_run(&summary_dir, &commit) {
        tracing::warn!("failed to write .last-run: {e}");
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/indexer/mod.rs
git commit -m "feat: wire folder descriptions into indexing, restructure lock scoping"
```

---

### Task 5: Manual integration test

- [ ] **Step 1: Build and run against youwhatknow itself**

```bash
cargo build && RUST_LOG=info cargo run
```

Delete `.claude/summaries` and trigger re-index. Verify:
- Info logs show: `extracting symbols: N files`, `generating file descriptions: N files (M batches, 4 concurrent)`, `starting batch 1/M`, `completed batch 1/M`, ..., `file descriptions complete`
- Folder descriptions in TOML files are real descriptions, not paths
- Project summary has meaningful folder descriptions

- [ ] **Step 2: Verify trace-level logging**

```bash
RUST_LOG=trace cargo run
```

Verify per-file trace entries appear: `extracting symbols: src/main.rs`, `describing file: src/main.rs`

- [ ] **Step 3: Test against immersiq**

Delete immersiq's `.claude/summaries` and trigger re-index. Verify:
- No `Argument list too long` errors
- Parallel batches visible in logs (batches starting/completing interleaved)
- All 470 files get real Haiku descriptions
- Folder descriptions generated
- Completes faster than sequential

- [ ] **Step 4: Commit any fixes**
