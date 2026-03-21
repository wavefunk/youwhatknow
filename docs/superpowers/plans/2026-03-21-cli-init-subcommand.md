# CLI Init Subcommand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `youwhatknow init` CLI subcommand that handles Claude Code's `SessionStart` hook (command-only), starting the daemon if needed and proxying the request.

**Architecture:** Add `clap` for subcommand parsing. `youwhatknow` bare or `youwhatknow serve` runs the daemon as before. `youwhatknow init` reads hook JSON from stdin, checks if the daemon is running via health check, spawns it in the background if not, then proxies the request to `/hook/session-start` and pipes the response to stdout.

**Tech Stack:** clap (derive), reqwest (blocking client for CLI), existing Tokio/Axum daemon

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add `clap`, `reqwest` dependencies |
| `src/main.rs` | Modify | Replace direct daemon startup with clap subcommand dispatch |
| `src/cli.rs` | Create | `init` subcommand: daemon probe, spawn, proxy, stdout |
| `src/daemon.rs` | Create | Extracted daemon startup logic (from current `main.rs`) |

---

## Milestone 1: Add clap and restructure main

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add clap and reqwest to Cargo.toml**

```toml
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["blocking", "json"] }
```

Add these to `[dependencies]`.

- [ ] **Step 2: Run `cargo check` to verify deps resolve**

Run: `just check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add clap and reqwest dependencies for CLI subcommands"
```

---

### Task 2: Extract daemon startup into `src/daemon.rs`

**Files:**
- Create: `src/daemon.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/daemon.rs` with extracted daemon logic**

Move the daemon startup logic from `main()` into `run()` which delegates to `run_on_port()`. The `run_on_port` takes an explicit port so tests can use a random port. Keep `shutdown_signal`, `pid_file_path`, `write_pid_file`, and `scopeguard` as private helpers in this module.

```rust
// src/daemon.rs
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use eyre::Context;
use tokio::net::TcpListener;

use crate::config::Config;
use crate::registry::ProjectRegistry;
use crate::server;
use crate::session::SessionTracker;

/// Start the daemon using port from config.
pub async fn run() -> eyre::Result<()> {
    let config = Config::load()?;
    run_on_port(config.port).await
}

/// Start the daemon on a specific port. Used by `run()` and tests.
pub async fn run_on_port(port: u16) -> eyre::Result<()> {
    tracing::info!("starting youwhatknow daemon");

    let config = Config::load()?;
    tracing::info!(port, "loaded config");

    let pid_file = pid_file_path();
    write_pid_file(&pid_file)?;

    let pid_file_cleanup = pid_file.clone();
    let _pid_guard = scopeguard(move || {
        let _ = std::fs::remove_file(&pid_file_cleanup);
    });

    let registry = ProjectRegistry::new();

    let session = SessionTracker::new();
    session.spawn_cleanup_task(config.session_timeout_minutes);

    let activity = server::ActivityTracker::new();
    let idle_shutdown_minutes = config.idle_shutdown_minutes;
    if idle_shutdown_minutes > 0 {
        tracing::info!(
            minutes = idle_shutdown_minutes,
            "idle shutdown enabled"
        );
    }

    let state = server::AppState {
        registry,
        session,
        config: Arc::new(config.clone()),
        activity: activity.clone(),
    };
    let app = server::router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;

    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(activity, idle_shutdown_minutes))
        .await
        .context("running server")?;

    tracing::info!("shutting down");
    Ok(())
}

async fn shutdown_signal(activity: server::ActivityTracker, idle_minutes: u64) {
    if idle_minutes > 0 {
        let timeout = Duration::from_secs(idle_minutes * 60);
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received CTRL+C");
            }
            _ = activity.wait_for_idle_timeout(timeout) => {}
        }
    } else {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
        tracing::info!("received CTRL+C");
    }
}

fn pid_file_path() -> PathBuf {
    crate::config::data_dir().join("youwhatknow.pid")
}

fn write_pid_file(path: &std::path::Path) -> eyre::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

fn scopeguard<F: FnOnce()>(f: F) -> impl Drop {
    struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }
    Guard(Some(f))
}
```

- [ ] **Step 2: Update `src/main.rs` to use `daemon::run()`**

Replace the current `main()` body. Add `mod daemon;` but do NOT add `mod cli;` yet (that file doesn't exist until Task 3).

```rust
mod config;
mod daemon;
mod hooks;
mod indexer;
mod registry;
mod session;
mod server;
mod storage;
mod types;

use tracing_subscriber::EnvFilter;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(daemon::run())
}
```

Note: We switch from `#[tokio::main]` to manual runtime construction so that `main()` is a regular `fn`, which clap needs in Task 3.

- [ ] **Step 3: Run tests to verify extraction didn't break anything**

Run: `just test`
Expected: all existing tests pass

- [ ] **Step 4: Commit**

```bash
git add src/daemon.rs src/main.rs
git commit -m "refactor: extract daemon startup into daemon module"
```

---

### Task 3: Add clap subcommands and CLI module

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/cli.rs`**

Note: `reqwest::blocking` creates its own Tokio runtime internally and must NOT be called from within an existing Tokio runtime context.

```rust
// src/cli.rs
use std::io::{self, Read};
use std::time::{Duration, Instant};

use crate::config::Config;

/// Handle the `init` subcommand: ensure daemon is running, proxy SessionStart.
pub fn init() -> eyre::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let config = Config::load()?;
    let base_url = format!("http://127.0.0.1:{}", config.port);

    // Ensure daemon is running
    if !daemon_is_running(&base_url) {
        spawn_daemon()?;
        wait_for_daemon(&base_url)?;
    }

    // Proxy to daemon
    proxy_session_start(&base_url, input)
}

fn daemon_is_running(base_url: &str) -> bool {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build();

    let Ok(client) = client else {
        return false;
    };

    client
        .get(format!("{base_url}/health"))
        .send()
        .is_ok_and(|r| r.status().is_success())
}

fn spawn_daemon() -> eyre::Result<()> {
    let exe = std::env::current_exe()?;
    let log_path = crate::config::data_dir().join("daemon.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = std::fs::File::create(&log_path)?;
    let stderr_file = log_file.try_clone()?;

    std::process::Command::new(exe)
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr_file))
        .spawn()?;
    Ok(())
}

fn wait_for_daemon(base_url: &str) -> eyre::Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(100);

    while start.elapsed() < timeout {
        if daemon_is_running(base_url) {
            return Ok(());
        }
        std::thread::sleep(poll_interval);
    }

    eyre::bail!("daemon did not start within {}s", timeout.as_secs())
}

fn proxy_session_start(base_url: &str, body: String) -> eyre::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let resp = client
        .post(format!("{base_url}/hook/session-start"))
        .header("content-type", "application/json")
        .body(body)
        .send()?;

    let status = resp.status();
    let response_body = resp.text()?;

    if !status.is_success() {
        eyre::bail!("daemon returned {status}: {response_body}");
    }

    // Write response JSON to stdout for Claude Code
    print!("{response_body}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_not_running_on_random_port() {
        assert!(!daemon_is_running("http://127.0.0.1:19999"));
    }
}
```

- [ ] **Step 2: Add `mod cli` and clap CLI parsing to `src/main.rs`**

```rust
mod cli;
mod config;
mod daemon;
mod hooks;
mod indexer;
mod registry;
mod session;
mod server;
mod storage;
mod types;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "youwhatknow", about = "Claude Code hook server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the daemon server
    Serve,
    /// Handle SessionStart hook (reads stdin, proxies to daemon)
    Init,
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Init) => {
            // No tracing for CLI — stdout is for Claude Code
            cli::init()
        }
        Some(Command::Serve) | None => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info")),
                )
                .init();

            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(daemon::run())
        }
    }
}
```

Note: `youwhatknow` with no subcommand still starts the daemon (backwards compatible). No tracing init for `init` subcommand since stdout must be clean JSON for Claude Code.

- [ ] **Step 3: Run `just check` to verify compilation**

Run: `just check`
Expected: compiles with no errors

- [ ] **Step 4: Run tests**

Run: `just test`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add clap subcommands — init and serve"
```

---

## Summary

| Task | What | Files |
|------|------|-------|
| 1 | Add clap + reqwest deps | `Cargo.toml` |
| 2 | Extract daemon into module | `src/daemon.rs`, `src/main.rs` |
| 3 | Add clap subcommands + CLI module | `src/cli.rs`, `src/main.rs` |
