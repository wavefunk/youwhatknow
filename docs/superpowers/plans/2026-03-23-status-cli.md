# Status CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `youwhatknow status` CLI command that reports daemon health via a new `/status` endpoint.

**Architecture:** New `StatusResponse` type, a `/status` GET endpoint in the server that reads existing state (activity, sessions, registry, config), and a CLI subcommand that hits this endpoint and formats the output as human-readable text.

**Tech Stack:** Rust, Axum, clap, reqwest (blocking), serde

**Spec:** `docs/superpowers/specs/2026-03-23-status-cli-design.md`

---

### Task 1: Add `StatusResponse` type

**Files:**
- Modify: `src/types.rs:161-167` (after `HealthResponse`)

- [ ] **Step 1: Add `StatusResponse` struct**

In `src/types.rs`, after the `HealthResponse` struct (line 167), add:

```rust
// ── Status check ──

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub pid: u32,
    pub port: u16,
    pub uptime_secs: u64,
    pub idle_secs: u64,
    pub active_sessions: usize,
    pub loaded_projects: usize,
    pub idle_shutdown_minutes: u64,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "feat(status): add StatusResponse type"
```

---

### Task 2: Add `session_count()` to `SessionTracker`

**Files:**
- Modify: `src/session.rs:22-109` (impl block)
- Test: `src/session.rs` (tests module)

- [ ] **Step 1: Write the failing test**

In `src/session.rs` tests module, add:

```rust
#[tokio::test]
async fn session_count_reflects_active_sessions() {
    let tracker = SessionTracker::new();
    assert_eq!(tracker.session_count().await, 0);

    tracker.track_read("session-1", Path::new("/a.rs")).await;
    assert_eq!(tracker.session_count().await, 1);

    tracker.track_read("session-2", Path::new("/b.rs")).await;
    assert_eq!(tracker.session_count().await, 2);

    // Same session, different file — still 2
    tracker.track_read("session-1", Path::new("/c.rs")).await;
    assert_eq!(tracker.session_count().await, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test session_count_reflects_active_sessions`
Expected: FAIL — `session_count` method doesn't exist

- [ ] **Step 3: Implement `session_count()`**

In `src/session.rs`, add this method to the `impl SessionTracker` block (after `track_summary`, around line 61):

```rust
/// Number of tracked sessions (not yet cleaned up).
pub async fn session_count(&self) -> usize {
    self.inner.read().await.last_activity.len()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test session_count_reflects_active_sessions`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/session.rs
git commit -m "feat(status): add session_count() to SessionTracker"
```

---

### Task 3: Add `started_at` to `AppState` and `/status` endpoint

**Files:**
- Modify: `src/server.rs:16` (import StatusResponse)
- Modify: `src/server.rs:71-77` (AppState struct)
- Modify: `src/server.rs:79-87` (router fn)
- Modify: `src/server.rs:163-170` (test_state helper)
- Modify: `src/daemon.rs:49-54` (AppState construction)

- [ ] **Step 1: Write the failing test**

In `src/server.rs` tests, add:

```rust
#[tokio::test]
async fn status_endpoint_returns_status() {
    let app = router(test_state());

    let req = Request::builder()
        .uri("/status")
        .method("GET")
        .body(Body::empty())
        .expect("request");

    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let status: crate::types::StatusResponse =
        serde_json::from_slice(&body).expect("deserialize");

    assert_eq!(status.pid, std::process::id());
    assert_eq!(status.port, Config::default().port);
    assert!(status.uptime_secs < 5);
    assert_eq!(status.active_sessions, 0);
    assert_eq!(status.loaded_projects, 0);
    assert_eq!(
        status.idle_shutdown_minutes,
        Config::default().idle_shutdown_minutes
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test status_endpoint_returns_status`
Expected: FAIL — no `/status` route, `started_at` field missing

- [ ] **Step 3: Add `started_at` to `AppState`**

In `src/server.rs`, extend the existing import at line 3 to include `Instant`:

```rust
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
```

Update `AppState` (line 71) to add the field:

```rust
pub struct AppState {
    pub registry: ProjectRegistry,
    pub session: SessionTracker,
    #[allow(dead_code)]
    pub config: Arc<Config>,
    pub activity: ActivityTracker,
    pub started_at: Instant,
}
```

- [ ] **Step 4: Add `status_handler` and wire route**

In `src/server.rs`, add the import of `StatusResponse` to line 16:

```rust
use crate::types::{HealthResponse, HookRequest, HookResponse, StatusResponse, SummaryRequest};
```

Add the handler after `health_handler` (after line 126):

```rust
async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    // Intentionally does NOT call activity.touch() —
    // polling status must not prevent idle shutdown.
    Json(StatusResponse {
        pid: std::process::id(),
        port: state.config.port,
        uptime_secs: state.started_at.elapsed().as_secs(),
        idle_secs: state.activity.idle_duration().as_secs(),
        active_sessions: state.session.session_count().await,
        loaded_projects: state.registry.project_count().await,
        idle_shutdown_minutes: state.config.idle_shutdown_minutes,
    })
}
```

Wire the route in `router()`:

```rust
.route("/status", get(status_handler))
```

- [ ] **Step 5: Update `test_state()` helper**

In `src/server.rs` tests, update `test_state()`:

```rust
fn test_state() -> AppState {
    AppState {
        registry: ProjectRegistry::new(),
        session: SessionTracker::new(),
        config: Arc::new(Config::default()),
        activity: ActivityTracker::new(),
        started_at: Instant::now(),
    }
}
```

- [ ] **Step 6: Update `daemon::run_on_port` to pass `started_at`**

In `src/daemon.rs`, add import at top:

```rust
use std::time::Instant;
```

Update AppState construction (line 49):

```rust
let state = server::AppState {
    registry,
    session,
    config: Arc::new(config.clone()),
    activity: activity.clone(),
    started_at: Instant::now(),
};
```

- [ ] **Step 7: Run tests**

Run: `cargo test status_endpoint_returns_status`
Expected: PASS

Run: `cargo test` (full suite to verify nothing broke)
Expected: all pass

- [ ] **Step 8: Commit**

```bash
git add src/server.rs src/daemon.rs
git commit -m "feat(status): add /status endpoint with uptime, sessions, projects"
```

---

### Task 4: Add `format_duration` helper and CLI `status` subcommand

**Files:**
- Modify: `src/cli.rs` (add `status()` fn and `format_duration()`)
- Modify: `src/main.rs:24-46` (Command enum)
- Modify: `src/main.rs:48-77` (main match)

- [ ] **Step 1: Write failing tests for `format_duration`**

In `src/cli.rs` tests module, add:

```rust
#[test]
fn format_duration_zero() {
    assert_eq!(format_duration(0), "0s");
}

#[test]
fn format_duration_seconds_only() {
    assert_eq!(format_duration(45), "45s");
}

#[test]
fn format_duration_minutes_and_seconds() {
    assert_eq!(format_duration(72), "1m 12s");
}

#[test]
fn format_duration_hours_and_minutes() {
    assert_eq!(format_duration(4320), "1h 12m");
}

#[test]
fn format_duration_days_and_hours() {
    assert_eq!(format_duration(90000), "1d 1h");
}

#[test]
fn format_duration_exact_boundary() {
    assert_eq!(format_duration(60), "1m");
    assert_eq!(format_duration(3600), "1h");
    assert_eq!(format_duration(86400), "1d");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test format_duration`
Expected: FAIL — function doesn't exist

- [ ] **Step 3: Implement `format_duration`**

In `src/cli.rs`, add (above the tests module):

```rust
fn format_duration(total_secs: u64) -> String {
    if total_secs == 0 {
        return "0s".to_owned();
    }

    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let parts: Vec<String> = [
        (days, "d"),
        (hours, "h"),
        (minutes, "m"),
        (seconds, "s"),
    ]
    .into_iter()
    .filter(|(v, _)| *v > 0)
    .take(2)
    .map(|(v, u)| format!("{v}{u}"))
    .collect();

    parts.join(" ")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test format_duration`
Expected: all PASS

- [ ] **Step 5: Add `status()` function in `cli.rs`**

Add import at top of `src/cli.rs`:

```rust
use crate::types::StatusResponse;
```

Add the function:

```rust
/// Handle the `status` subcommand: query daemon and display status.
pub fn status() -> eyre::Result<()> {
    let config = Config::load()?;
    let base_url = format!("http://127.0.0.1:{}", config.port);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resp = match client.get(format!("{base_url}/status")).send() {
        Ok(r) => r,
        Err(e) if e.is_connect() => {
            eprintln!("daemon is not running");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("failed to reach daemon: {e}");
            std::process::exit(1);
        }
    };

    if !resp.status().is_success() {
        eprintln!("daemon returned {}", resp.status());
        std::process::exit(1);
    }

    let status: StatusResponse = resp.json()?;

    println!("youwhatknow daemon running (pid {})", status.pid);
    println!("  port:             {}", status.port);
    println!("  uptime:           {}", format_duration(status.uptime_secs));
    println!("  idle:             {}", format_duration(status.idle_secs));
    println!("  sessions:         {}", status.active_sessions);
    println!("  projects:         {}", status.loaded_projects);
    println!("  idle shutdown:    {}m", status.idle_shutdown_minutes);

    Ok(())
}
```

- [ ] **Step 6: Wire `Status` subcommand in `main.rs`**

In `src/main.rs`, add variant to `Command` enum (after `Setup`):

```rust
/// Show daemon status
Status,
```

Add match arm in `main()` (before the `Serve` catch-all):

```rust
Some(Command::Status) => cli::status(),
```

- [ ] **Step 7: Run full test suite**

Run: `cargo test`
Expected: all pass

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(status): add status CLI subcommand with duration formatting"
```

**Note:** The CLI error path (`daemon is not running`) uses `process::exit(1)` which is not unit-testable. This is acceptable for a simple CLI command — manual verification is sufficient. If the error handling grows more complex in future, refactor to return typed errors.

---

### Task 5: Add bd task to tracker

- [ ] **Step 1: Create bd task**

```bash
bd create --title "Add status CLI command" --type task --priority P1 --state done
```

Or close the existing task if one was already created.

- [ ] **Step 2: Close the bd task**

```bash
bd close <id> --reason "implemented"
```
