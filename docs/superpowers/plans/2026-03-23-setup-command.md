# Setup Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `youwhatknow setup` CLI subcommand that configures Claude Code hooks and triggers initial indexing for a project.

**Architecture:** New `Setup` variant in the clap `Command` enum, routed to a `cli::setup()` function. A pure `merge_hooks()` helper handles JSON merging (testable in isolation). Daemon lifecycle reuses existing helpers.

**Tech Stack:** Rust, clap, serde_json, reqwest (blocking)

**Spec:** `docs/superpowers/specs/2026-03-23-setup-command-design.md`

---

### File Map

- **Modify:** `src/main.rs` — add `Setup` variant to `Command` enum, route to `cli::setup()`
- **Modify:** `src/cli.rs` — add `pub fn setup()`, private `merge_hooks()`, private `build_hooks_value()`

---

### Task 1: Add `merge_hooks` helper with tests

**Files:**
- Modify: `src/cli.rs`

This is the core logic — a pure function that takes existing settings JSON and a port, returns merged JSON. TDD this first.

- [ ] **Step 1: Write failing tests for `merge_hooks`**

Add to the `#[cfg(test)] mod tests` block in `src/cli.rs`:

```rust
#[test]
fn merge_hooks_empty_settings() {
    let existing = serde_json::json!({});
    let MergeResult { settings, preserved } = merge_hooks(existing, 7849);
    let hooks = settings.get("hooks").expect("hooks key");
    let session_start = hooks.get("SessionStart").expect("SessionStart");
    assert!(session_start.is_array());
    assert_eq!(session_start.as_array().unwrap().len(), 1);
    let pre_tool = hooks.get("PreToolUse").expect("PreToolUse");
    assert!(pre_tool.is_array());
    assert_eq!(pre_tool.as_array().unwrap().len(), 1);
    assert_eq!(preserved, 0);
}

#[test]
fn merge_hooks_preserves_other_hooks() {
    let existing = serde_json::json!({
        "hooks": {
            "SessionStart": [{
                "hooks": [{
                    "type": "command",
                    "command": "my-linter check",
                    "timeout": 10
                }]
            }],
            "PreToolUse": [{
                "matcher": "Write",
                "hooks": [{
                    "type": "command",
                    "command": "my-formatter",
                    "timeout": 5
                }]
            }]
        }
    });
    let MergeResult { settings, preserved } = merge_hooks(existing, 7849);
    let hooks = &settings["hooks"];
    // Our hooks + the existing one
    assert_eq!(hooks["SessionStart"].as_array().unwrap().len(), 2);
    // PreToolUse: existing Write matcher + our Read matcher
    assert_eq!(hooks["PreToolUse"].as_array().unwrap().len(), 2);
    // Verify existing hooks are preserved
    let first_ss = &hooks["SessionStart"][0];
    assert_eq!(first_ss["hooks"][0]["command"], "my-linter check");
    assert_eq!(preserved, 2);
}

#[test]
fn merge_hooks_replaces_existing_youwhatknow() {
    let existing = serde_json::json!({
        "hooks": {
            "SessionStart": [
                {
                    "hooks": [{
                        "type": "command",
                        "command": "youwhatknow init",
                        "timeout": 30
                    }]
                },
                {
                    "hooks": [{
                        "type": "command",
                        "command": "other-tool start",
                        "timeout": 10
                    }]
                }
            ],
            "PreToolUse": [{
                "matcher": "Read",
                "hooks": [{
                    "type": "http",
                    "url": "http://localhost:7849/hook/pre-read",
                    "timeout": 5
                }]
            }]
        }
    });
    let MergeResult { settings, preserved } = merge_hooks(existing, 7849);
    let hooks = &settings["hooks"];
    // other-tool preserved + our new one = 2
    assert_eq!(hooks["SessionStart"].as_array().unwrap().len(), 2);
    // Old youwhatknow replaced, so still 1
    assert_eq!(hooks["PreToolUse"].as_array().unwrap().len(), 1);
    assert_eq!(preserved, 1);
}

#[test]
fn merge_hooks_preserves_non_hook_settings() {
    let existing = serde_json::json!({
        "permissions": { "allow": ["Read"] },
        "hooks": {}
    });
    let MergeResult { settings, .. } = merge_hooks(existing, 7849);
    assert_eq!(settings["permissions"]["allow"][0], "Read");
}

#[test]
fn merge_hooks_group_without_hooks_array_preserved() {
    let existing = serde_json::json!({
        "hooks": {
            "SessionStart": [{ "matcher": "odd-entry" }]
        }
    });
    let MergeResult { settings, preserved } = merge_hooks(existing, 7849);
    // Odd entry preserved + our entry = 2
    assert_eq!(settings["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
    assert_eq!(preserved, 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::tests -- --nocapture 2>&1 | head -40`
Expected: compilation error — `merge_hooks` not found

- [ ] **Step 3: Implement `merge_hooks` and `build_hooks_value`**

Add above the `#[cfg(test)]` block in `src/cli.rs`:

```rust
/// Build the youwhatknow hook entries for a given port.
fn build_hooks_value(port: u16) -> serde_json::Value {
    serde_json::json!({
        "SessionStart": [{
            "hooks": [{
                "type": "command",
                "command": "youwhatknow init",
                "timeout": 30
            }]
        }],
        "PreToolUse": [{
            "matcher": "Read",
            "hooks": [{
                "type": "http",
                "url": format!("http://localhost:{port}/hook/pre-read"),
                "timeout": 5
            }]
        }]
    })
}

/// Returns true if a hook group contains any youwhatknow hook.
fn is_youwhatknow_group(group: &serde_json::Value) -> bool {
    let Some(hooks) = group.get("hooks").and_then(|h| h.as_array()) else {
        return false;
    };
    hooks.iter().any(|hook| {
        let cmd = hook.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let url = hook.get("url").and_then(|v| v.as_str()).unwrap_or("");
        cmd.contains("youwhatknow") || url.contains("youwhatknow")
    })
}

/// Result of merging hooks: the updated settings and count of preserved groups.
struct MergeResult {
    settings: serde_json::Value,
    preserved: usize,
}

/// Merge youwhatknow hooks into existing settings JSON.
/// Removes old youwhatknow entries, appends new ones, preserves everything else.
fn merge_hooks(mut settings: serde_json::Value, port: u16) -> MergeResult {
    let our_hooks = build_hooks_value(port);
    let mut preserved = 0;

    let hooks = settings
        .as_object_mut()
        .expect("settings must be an object")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks.as_object_mut().expect("hooks must be an object");

    for event_name in ["SessionStart", "PreToolUse"] {
        let existing: Vec<serde_json::Value> = hooks_obj
            .get(event_name)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Keep non-youwhatknow entries
        let mut filtered: Vec<serde_json::Value> = existing
            .into_iter()
            .filter(|group| !is_youwhatknow_group(group))
            .collect();

        preserved += filtered.len();

        // Append our entries
        if let Some(our_entries) = our_hooks.get(event_name).and_then(|v| v.as_array()) {
            filtered.extend(our_entries.iter().cloned());
        }

        hooks_obj.insert(event_name.to_owned(), serde_json::Value::Array(filtered));
    }

    MergeResult { settings, preserved }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib cli::tests -- --nocapture`
Expected: all 7 tests pass (5 new + 2 existing)

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat(setup): add merge_hooks helper with tests"
```

---

### Task 2: Add `Setup` subcommand to CLI

**Files:**
- Modify: `src/main.rs`
- Modify: `src/cli.rs`

Wire up the CLI argument parsing and the main `setup()` function.

- [ ] **Step 1: Add `Setup` variant to `Command` enum in `src/main.rs`**

Add after the `Summary` variant:

```rust
    /// Initialize a project to use youwhatknow
    Setup {
        /// Write hooks to .claude/settings.json (shared with team)
        #[arg(long)]
        shared: bool,
        /// Write hooks to .claude/settings.local.json (default, per-developer)
        #[arg(long)]
        local: bool,
        /// Skip initial indexing
        #[arg(long)]
        no_index: bool,
    },
```

Add the match arm in `main()`:

```rust
        Some(Command::Setup { shared, local: _, no_index }) => {
            cli::setup(shared, no_index)
        }
```

- [ ] **Step 2: Add `setup()` function to `src/cli.rs`**

```rust
/// Handle the `setup` subcommand: configure hooks and optionally trigger indexing.
pub fn setup(shared: bool, no_index: bool) -> eyre::Result<()> {
    let cwd = std::env::current_dir()?;
    let config = Config::load()?;

    // 1. Determine target file
    let claude_dir = cwd.join(".claude");
    let target_file = if shared {
        claude_dir.join("settings.json")
    } else {
        claude_dir.join("settings.local.json")
    };

    // 2. Create .claude/ directory
    std::fs::create_dir_all(&claude_dir)?;

    // 3. Read existing settings or start fresh
    let existing: serde_json::Value = if target_file.exists() {
        let content = std::fs::read_to_string(&target_file)?;
        serde_json::from_str(&content)
            .map_err(|e| eyre::eyre!("malformed JSON in {}: {e}", target_file.display()))?
    } else {
        serde_json::json!({})
    };

    // 4. Merge hooks
    let MergeResult { settings: merged, preserved } = merge_hooks(existing, config.port);

    // 5. Write settings file
    let json_str = serde_json::to_string_pretty(&merged)?;
    std::fs::write(&target_file, format!("{json_str}\n"))?;
    eprintln!("Wrote hooks to {}", target_file.display());
    if preserved > 0 {
        eprintln!("Preserved {preserved} existing hook group(s).");
    }

    // 6. Create summaries directory
    let summaries_dir = claude_dir.join("summaries");
    std::fs::create_dir_all(&summaries_dir)?;

    // 7. Optionally trigger indexing
    if !no_index {
        let base_url = format!("http://127.0.0.1:{}", config.port);
        if !daemon_is_running(&base_url) {
            eprintln!("Starting daemon...");
            spawn_daemon()?;
            wait_for_daemon(&base_url)?;
        }

        eprintln!("Indexing project...");
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let resp = client
            .post(format!("{base_url}/reindex"))
            .header("content-type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({
                "session_id": "setup",
                "cwd": cwd,
                "hook_event_name": "Setup"
            }))?)
            .send()?;

        if !resp.status().is_success() {
            eyre::bail!("reindex failed: {}", resp.status());
        }
    }

    eprintln!("Setup complete.");
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/cli.rs
git commit -m "feat(setup): add setup subcommand with hook config and indexing"
```

---

### Task 3: Manual smoke test

**Files:** none (verification only)

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Test `--help` output**

Run: `cargo run --quiet -- setup --help`
Expected: shows setup subcommand with `--shared`, `--local`, `--no-index` flags

- [ ] **Step 4: Dry-run setup with `--no-index` in a temp directory**

```bash
cd $(mktemp -d) && git init
cargo run --quiet --manifest-path /home/nambiar/projects/wavefunk/youwhatknow/Cargo.toml -- setup --no-index
cat .claude/settings.local.json
ls .claude/summaries/
```

Expected: settings.local.json has the hooks config, summaries/ directory exists

- [ ] **Step 5: Test idempotency — run again**

```bash
cargo run --quiet --manifest-path /home/nambiar/projects/wavefunk/youwhatknow/Cargo.toml -- setup --no-index
cat .claude/settings.local.json
```

Expected: same output, no duplicate hooks

- [ ] **Step 6: Test `--shared` flag**

```bash
cargo run --quiet --manifest-path /home/nambiar/projects/wavefunk/youwhatknow/Cargo.toml -- setup --shared --no-index
cat .claude/settings.json
```

Expected: hooks written to settings.json instead
