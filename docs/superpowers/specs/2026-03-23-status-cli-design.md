# Status CLI Design

## Overview

Add a `status` subcommand to the youwhatknow CLI that reports daemon health and operational state via a new `/status` endpoint.

## Server: `/status` endpoint

New `GET /status` route returning `StatusResponse` (lives in `types.rs` alongside `HealthResponse`, derives `Serialize, Deserialize`):

```rust
struct StatusResponse {
    pid: u32,
    port: u16,
    uptime_secs: u64,
    idle_secs: u64,
    active_sessions: usize,
    loaded_projects: usize,
    idle_shutdown_minutes: u64,
}
```

Field sources:
- `pid`: `std::process::id()` — called in the handler, not read from PID file.
- `port`: `state.config.port`
- `uptime_secs`: `state.started_at.elapsed().as_secs()` — `started_at` is `std::time::Instant`.
- `idle_secs`: `state.activity.idle_duration().as_secs()`
- `active_sessions`: `state.session.session_count()` — returns `last_activity.len()` (sessions not yet cleaned up).
- `loaded_projects`: `state.registry.project_count()`
- `idle_shutdown_minutes`: `state.config.idle_shutdown_minutes` (configured value, not remaining time).

### Required changes

- **`AppState`** in `server.rs`: Add `started_at: std::time::Instant` field, set in `daemon::run_on_port` right before creating `AppState`.
- **`SessionTracker`**: Add `session_count()` → `usize`, returns `last_activity.len()`.
- **`server.rs`**: Add `status_handler` and wire `GET /status` in `router()`. The handler must NOT call `activity.touch()` — polling status should not prevent idle shutdown.
- **`types.rs`**: Add `StatusResponse` struct.
- **Test helpers**: Update `test_state()` in `server.rs` tests to include `started_at`.

## CLI: `Status` subcommand

New `Command::Status` variant in `main.rs`. Handler `cli::status()` in `cli.rs`:

1. Load `Config` to get port.
2. `GET http://127.0.0.1:{port}/status` with 5s timeout.
3. On success, format and print human-readable output.
4. On connection refused, print `"daemon is not running"` and exit with code 1.
5. On other errors (timeout, HTTP error), print `"failed to reach daemon: {error}"` and exit with code 1.

### Output format

```
youwhatknow daemon running (pid 12345)
  port:             7849
  uptime:           1h 12m
  idle:             45s
  sessions:         2
  projects:         3
  idle shutdown:    30m
```

### Duration formatting

Helper function `format_duration(secs: u64) -> String` in `cli.rs`. Shows up to two non-zero units from `{d, h, m, s}`, skipping zero sub-units. Examples: `1h 12m`, `45s`, `3d 2h`, `1m`. Zero input returns `0s`. The `idle shutdown` line always displays in minutes since it's a config value (e.g., `30m`).

## Testing

- **Unit tests** for `format_duration`: boundary cases (0, 59, 60, 3600, 86400, mixed).
- **Integration test**: start daemon on random port, `GET /status`, assert `pid == std::process::id()`, `uptime_secs < 5`, `idle_secs` small, `active_sessions == 0`, `loaded_projects == 0`, `port` matches random port, `idle_shutdown_minutes` matches config default.
- **CLI error path**: verify "not running" message when no daemon is listening.
