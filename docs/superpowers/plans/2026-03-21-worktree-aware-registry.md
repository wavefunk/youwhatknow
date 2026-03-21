# Worktree-Aware Project Registry

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent full reindexing when Claude Code opens a git worktree of an already-indexed project.

**Architecture:** Resolve every incoming `cwd` to its main worktree root via `git rev-parse --git-common-dir`, then use that canonical path as the registry key. If the main project is already loaded, worktrees share the existing `Index` instantly with no reindex. If it's a first-time load, summaries are read from the main worktree's `.claude/summaries/` directory (where they actually live, since the dir is gitignored).

**Tech Stack:** Rust, Tokio, git CLI

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/indexer/discovery.rs` | Modify | Add `resolve_main_worktree()` function |
| `src/indexer/mod.rs` | Modify | Make `set_ready` available outside `#[cfg(test)]` |
| `src/registry.rs` | Modify | Use canonical root as key, share indexes across worktrees |

---

### Task 1: Add `resolve_main_worktree` to discovery

**Files:**
- Modify: `src/indexer/discovery.rs`

- [ ] **Step 1: Write the test**

Add to the `tests` module at the bottom of `discovery.rs`:

```rust
#[test]
fn resolve_main_worktree_in_regular_repo() {
    // In a non-worktree repo, resolve_main_worktree should return
    // the same path as git rev-parse --show-toplevel
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolved = resolve_main_worktree(root).expect("resolve");
    let expected = root.canonicalize().expect("canonicalize");
    assert_eq!(resolved, expected);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test resolve_main_worktree -- --nocapture`
Expected: FAIL — `resolve_main_worktree` doesn't exist yet.

- [ ] **Step 3: Implement `resolve_main_worktree`**

Add this public function to `discovery.rs`, above `file_folder()`:

```rust
/// Resolve the main worktree root for a given path.
///
/// For regular repos, returns the repo root (same as `--show-toplevel`).
/// For linked worktrees, returns the main worktree's root.
/// This lets worktrees share an already-loaded project index.
pub fn resolve_main_worktree(cwd: &Path) -> eyre::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(cwd)
        .output()
        .context("running git rev-parse --git-common-dir")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse --git-common-dir failed: {stderr}");
    }

    let git_common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    // git-common-dir points to the .git directory of the main worktree.
    // Its parent is the main worktree root.
    let main_root = git_common
        .parent()
        .ok_or_else(|| eyre::eyre!("git-common-dir has no parent: {}", git_common.display()))?;

    // Canonicalize to resolve symlinks and get a stable key
    main_root
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", main_root.display()))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test resolve_main_worktree -- --nocapture`
Expected: PASS. Note: uses `#[ignore]` pattern if needed for CI, but this test should pass locally in a real git repo.

- [ ] **Step 5: Commit**

```bash
git add src/indexer/discovery.rs
git commit -m "feat: add resolve_main_worktree for worktree-aware project keying"
```

---

### Task 2: Make registry worktree-aware

**Files:**
- Modify: `src/indexer/mod.rs`
- Modify: `src/registry.rs`

- [ ] **Step 0: Make `set_ready` available outside tests**

In `src/indexer/mod.rs`, remove the `#[cfg(test)]` gate from `set_ready`:

```rust
    // Before:
    #[cfg(test)]
    pub fn set_ready(&self, ready: bool) {

    // After:
    pub fn set_ready(&self, ready: bool) {
```

This is needed because worktrees that load from disk without running background indexing need to mark the index as ready immediately.

- [ ] **Step 1: Write the test for worktree alias sharing**

Add to the `tests` module in `registry.rs`:

```rust
#[tokio::test]
async fn worktree_alias_shares_index() {
    let registry = ProjectRegistry::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let main_root = tmp.path().to_path_buf();

    // Simulate: main project is loaded
    let (index1, _) = registry.get_or_load(&main_root).await;

    // Simulate: a worktree path resolves to the same main_root.
    // We test register_alias directly since we can't create real worktrees in tests.
    let worktree_path = PathBuf::from("/fake/worktree/path");
    registry.register_alias(&worktree_path, &main_root).await;

    let (index2, _) = registry.get_or_load(&worktree_path).await;

    // They should share the same ready state (same underlying Arc)
    assert_eq!(index1.is_ready(), index2.is_ready());
    assert_eq!(registry.project_count().await, 2); // two keys, same index
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test worktree_alias -- --nocapture`
Expected: FAIL — `register_alias` doesn't exist.

- [ ] **Step 3: Implement worktree-aware `get_or_load` and `register_alias`**

Replace the `get_or_load` method and add `register_alias` in `registry.rs`:

```rust
use crate::indexer::discovery;

/// Register a path as an alias for an existing project entry.
/// Used internally when a worktree resolves to a known main root.
#[cfg(test)]
pub async fn register_alias(&self, alias: &Path, canonical: &Path) {
    let mut state = self.inner.write().await;
    if let Some(entry) = state.projects.get(canonical) {
        state.projects.insert(
            alias.to_path_buf(),
            ProjectEntry {
                index: entry.index.clone(),
                config: entry.config.clone(),
            },
        );
    }
}

pub async fn get_or_load(&self, cwd: &Path) -> (Index, ProjectConfig) {
    // Fast path: cwd already registered (read lock)
    {
        let state = self.inner.read().await;
        if let Some(entry) = state.projects.get(cwd) {
            return (entry.index.clone(), entry.config.clone());
        }
    }

    // Resolve to main worktree root for canonical keying
    let main_root = discovery::resolve_main_worktree(cwd)
        .unwrap_or_else(|_| cwd.to_path_buf());

    let mut state = self.inner.write().await;

    // Double-check cwd after acquiring write lock
    if let Some(entry) = state.projects.get(cwd) {
        return (entry.index.clone(), entry.config.clone());
    }

    // Check if main_root already has an entry (worktree of known project)
    if main_root != cwd {
        if let Some(entry) = state.projects.get(&main_root) {
            let result = (entry.index.clone(), entry.config.clone());
            // Register cwd as alias so future lookups are fast
            state.projects.insert(
                cwd.to_path_buf(),
                ProjectEntry {
                    index: entry.index.clone(),
                    config: entry.config.clone(),
                },
            );
            tracing::info!(
                worktree = %cwd.display(),
                main = %main_root.display(),
                "linked worktree to existing project index"
            );
            return result;
        }
    }

    // New project
    let config = ProjectConfig::load(cwd).unwrap_or_default();
    let index = Index::new();

    // Load existing summaries from main worktree (they're gitignored,
    // so only the main worktree has them on disk)
    index.load_from_disk(&main_root, &config).await;

    // Only run background indexing for the main worktree.
    // Worktrees share the main index and don't write their own summaries.
    let is_worktree = main_root != cwd;
    if !is_worktree {
        let bg_index = index.clone();
        let bg_root = cwd.to_path_buf();
        let bg_config = config.clone();
        tokio::spawn(async move {
            let summary_dir = bg_root.join(&bg_config.summary_path);
            if crate::storage::read_last_run(&summary_dir).is_some() {
                bg_index.incremental_index(&bg_root, &bg_config).await;
            } else {
                bg_index.full_index(&bg_root, &bg_config).await;
            }
        });
    } else {
        // Worktree with no main entry yet — mark ready immediately
        // since we loaded what we could from disk
        index.set_ready(true);
    }

    let entry = ProjectEntry {
        index: index.clone(),
        config: config.clone(),
    };

    // Register under main_root too so future worktrees find it
    if is_worktree {
        state.projects.insert(main_root, ProjectEntry {
            index: index.clone(),
            config: config.clone(),
        });
        tracing::info!(
            worktree = %cwd.display(),
            main_root = %main_root.display(),
            "registered worktree with shared index"
        );
    }
    state.projects.insert(cwd.to_path_buf(), entry);

    tracing::info!(project = %cwd.display(), "registered new project");
    (index, config)
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test -- --nocapture`
Expected: All pass, including existing `lazy_load_creates_entry`, `different_projects_get_different_indexes`, etc.

- [ ] **Step 5: Commit**

```bash
git add src/registry.rs
git commit -m "feat: share project index across git worktrees"
```

---

### Task 3: Verify end-to-end with manual test

- [ ] **Step 1: Build and run the server**

Run: `cargo build && cargo run`

- [ ] **Step 2: In another terminal, create a worktree and test**

```bash
cd /path/to/a/project-already-indexed
git worktree add /tmp/test-worktree -b test-wt-branch

# Hit the hook endpoint with the worktree path
curl -s -X POST http://localhost:7849/hook/session-start \
  -H 'Content-Type: application/json' \
  -d '{"session_id":"wt-test","cwd":"/tmp/test-worktree","hook_event_name":"SessionStart"}'
```

Expected: Response contains the project map from the main worktree's index, NOT "indexing in progress". Server logs should show "linked worktree to existing project index".

- [ ] **Step 3: Check health for project count**

```bash
curl -s http://localhost:7849/health | jq .
```

Expected: `projects` count is 2 (main + worktree alias), not 2 independent indexes.

- [ ] **Step 4: Clean up**

```bash
git worktree remove /tmp/test-worktree
```
