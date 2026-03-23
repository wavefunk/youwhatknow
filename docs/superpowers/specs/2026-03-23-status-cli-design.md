# Status CLI Design

## Overview

Add a `status` subcommand to the youwhatknow CLI that reports daemon health and operational state via a new `/status` endpoint.

## Server: `/status` endpoint

New `GET /status` route returning `StatusResponse`:

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

### Required changes

- **`AppState`**: Add `started_at: Instant` field, set in `daemon::run_on_port`.
- **`SessionTracker`**: Add `session_count()` method returning count of distinct active session IDs.
- **`server.rs`**: Add `status_handler` and wire `GET /status` in `router()`.

The handler reads all values from existing state — no new tracking infrastructure needed. `uptime_secs` is computed from `started_at.elapsed()`, `idle_secs` from `activity.idle_duration()`.

## CLI: `Status` subcommand

New `Command::Status` variant in `main.rs`. Handler in `cli.rs`:

1. Load `Config` to get port.
2. `GET http://127.0.0.1:{port}/status` with 5s timeout.
3. On success, format and print human-readable output.
4. On failure, print `"daemon is not running"` and exit with code 1.

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

Duration formatting: show largest two units (e.g., `1h 12m`, `45s`, `3d 2h`). Zero values show `0s`.

## Testing

- Unit test for duration formatting helper.
- Integration test: start daemon on random port, hit `/status`, verify all fields present and sensible.
- CLI test: verify "not running" case prints message and exits non-zero.
