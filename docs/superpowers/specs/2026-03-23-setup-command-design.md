# Design: `youwhatknow setup` Command

**Date:** 2026-03-23
**Task:** youwhatknow-0dt — Add setup script to initialize a project for youwhatknow

## Purpose

One-time project onboarding command that configures Claude Code hooks and generates initial file summaries. Eliminates manual setup so a user can go from install to working in a single command.

## CLI Interface

```
youwhatknow setup [--local] [--shared] [--no-index]
```

- **Default** (no flag or `--local`): writes to `.claude/settings.local.json` (gitignored, per-developer)
- `--shared`: writes to `.claude/settings.json` (checked in, whole-team adoption)
- `--no-index`: skip initial indexing after config setup

Runs from the project root directory (uses `cwd`).

## Generated Hook Configuration

```json
{
  "hooks": {
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
        "url": "http://localhost:<port>/hook/pre-read",
        "timeout": 5
      }]
    }]
  }
}
```

Port is read from `Config::load()`, not hardcoded.

## Setup Flow

1. Determine target file: `.claude/settings.local.json` (default) or `.claude/settings.json` (`--shared`)
2. Create `.claude/` directory if it doesn't exist
3. Read existing settings file if present, otherwise start with `{}`
4. Merge hooks:
   - Walk existing `hooks.SessionStart` and `hooks.PreToolUse` arrays
   - Remove entries whose inner hooks contain `"youwhatknow"` in the command or the configured localhost URL
   - Append youwhatknow hook entries
   - Preserve all other hook entries untouched
5. Write settings file (pretty-printed JSON)
6. Create `.claude/summaries/` directory if it doesn't exist
7. Print what was written and what was preserved
8. Unless `--no-index`: start daemon (reuse `spawn_daemon`/`wait_for_daemon`), POST `/reindex` with body `{"session_id": "setup", "cwd": "<project_root>", "hook_event_name": "Setup"}`
   - If daemon is already running, reuse it
   - If daemon fails to start, fail with error
   - If `/reindex` POST fails, fail with error (daemon keeps running)
9. Print success message

No `.claude/youwhatknow.toml` is generated — defaults are sensible; users create one manually if needed.

## Hook Merging Strategy

When existing settings are present, youwhatknow hooks are identified by string-matching `"youwhatknow"` in the hook command or URL. Port numbers are not used for matching.

Claude Code settings structure: `hooks.<EventName>` is an array of hook group objects. Each group has an optional `matcher` (string) and a `hooks` array of individual hook objects (each with `type`, `command`/`url`, `timeout`).

- If `hooks` key is missing, create it as `{}`
- If `hooks.SessionStart` or `hooks.PreToolUse` is missing, create as `[]`
- For each hook group (object in the event array), check its `hooks` array: if ANY hook has a `command` or `url` field containing `"youwhatknow"`, remove the entire group
- If a group has no `hooks` array or it's not an array, leave the group untouched
- All non-matching groups are preserved in their original order
- If the settings file exists but contains malformed JSON, fail with an error message

Setup is idempotent — running it multiple times updates hook entries and re-runs indexing without data loss.

## Code Changes

**Modified files:**
- `src/main.rs` — add `Setup` variant to `Command` enum with `--local`, `--shared`, `--no-index` clap flags
- `src/cli.rs` — add `pub fn setup(shared: bool, no_index: bool)` function and private `merge_hooks` helper

**No new files.**

Reuses existing `spawn_daemon`, `wait_for_daemon`, `daemon_is_running` from `cli.rs`.

## Test Surface

The `merge_hooks` helper is the primary test target:
- Empty/missing settings file
- Settings with no hooks key
- Settings with existing youwhatknow hooks (overwrite)
- Settings with other hooks (preserve)
- Settings with both youwhatknow and other hooks (overwrite ours, preserve theirs)
