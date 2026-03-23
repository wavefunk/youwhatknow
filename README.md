<p align="center">
  <img alt="!?" src="docs/logo.svg" width="120" height="120">
</p>

<h3 align="center"><em>you what (k)now?</em></h3>
<p align="center">"Wait, are you reading that file <em>again</em>?"</p>
<p align="center">
  <a href="https://youwhatknow.wavefunk.dev">Website</a> &middot;
  <a href="https://youwhatknow.wavefunk.dev/docs.html">Docs</a>
</p>

---

A [Claude Code](https://docs.anthropic.com/en/docs/claude-code) hook server that gently intervenes when Claude reaches for a file it's already read. Instead of 2000 lines of code, Claude gets a 3-line summary. Instead of reading `main.rs` for the fifth time, it gets a tap on the shoulder: *"hey buddy, you already read that."*

One daemon. All your projects. One command to set up.

## The problem

Claude reads files like someone who opens the fridge every 10 minutes hoping new food appeared. It didn't. It's the same fridge. Same food. Same 847 lines of `main.rs`.

## What this does

1. **Pre-read summaries** — Before every `Read`, Claude gets: file description, public symbols, folder context. Three lines. Usually enough.
2. **Repeat-read tracking** — Second read gets a gentle note. Third read gets *"this file was already read 3x this session — do you still need the full file?"*
3. **Line-count gating** — Files under 30 lines are passed through without intervention. They're small enough to just read. Only files over the threshold get the summary-first treatment. (Configurable via `line_threshold` in project config.)
4. **Session-start orientation** — On `SessionStart`, Claude receives a full project map. No more "let me explore the codebase" spirals.
5. **Background indexing** — Tree-sitter extracts symbols, Haiku generates one-line descriptions, everything lands in TOML summaries under `.claude/summaries/`.
6. **Worktree sharing** — Projects accessed from different git worktrees share the same index. It resolves to the git root, so your summaries stay in one place.

## Architecture

```
┌──────────────┐  ┌──────────────┐  ┌──────────┐
│ Claude Sess A│  │ Claude Sess B│  │ Subagent │
└──────┬───────┘  └──────┬───────┘  └────┬─────┘
       │                 │               │
       └────────┬────────┴───────┬───────┘
                │  HTTP hooks    │
        ┌───────▼────────────────▼───────┐
        │  youwhatknow — localhost:7849  │
        └───────┬────────────────┬───────┘
                │  routes by cwd │
       ┌────────┴──┐     ┌──────┴───────┐
       │ Project A │     │  Project B   │
       │   Index   │     │    Index     │
       └───────────┘     └──────────────┘
                │
    tree-sitter · haiku · TOML

youwhatknow setup → hooks + daemon + initial index
```

## Setup

### The easy way (recommended)

```sh
cd your-project
youwhatknow setup
```

That's it. This creates `.claude/` and `.claude/summaries/`, merges hook config into `.claude/settings.local.json`, starts the daemon if it isn't running, and triggers initial indexing. Existing hooks in the settings file are preserved — it merges, not overwrites.

```sh
# Variants:
youwhatknow setup --shared     # writes to .claude/settings.json (team-shared)
youwhatknow setup --no-index   # skip initial indexing
```

### The manual way

If you prefer to wire things up yourself, add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Read",
      "hooks": [{
        "type": "http",
        "url": "http://localhost:7849/hook/pre-read"
      }]
    }],
    "SessionStart": [{
      "matcher": "startup",
      "hooks": [{
        "type": "http",
        "url": "http://localhost:7849/hook/session-start"
      }]
    }]
  }
}
```

Then start the daemon:

```sh
youwhatknow
```

It indexes your projects lazily on first request. Shuts itself down after 30 minutes of inactivity. Next session brings it back.

### CLI commands

```sh
youwhatknow              # start the daemon
youwhatknow setup        # set up hooks for current project
youwhatknow status       # show daemon uptime, active sessions, registered projects
youwhatknow summary <path>  # preview a file's summary (without triggering a read count)
youwhatknow init         # SessionStart hook handler (called automatically by hooks)
```

### Configuration (optional)

`~/.config/youwhatknow/config.toml`:

```toml
port = 7849                   # daemon port
session_timeout_minutes = 60  # stale session cleanup
idle_shutdown_minutes = 30    # auto-shutdown when bored
```

All settings have sensible defaults. You probably don't need this file.

## Building from source

```sh
cargo build --release
```

Requires Rust 2024 edition. A Nix flake is included for reproducible builds:

```sh
nix build
```

## Built with

Rust, Tokio, Axum, tree-sitter, and a mild sense of exasperation.

---

<p align="center"><sub>"No Claude was harmed in the making of this tool. Just mildly inconvenienced."</sub></p>
