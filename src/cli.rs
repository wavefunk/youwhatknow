use std::io::{self, Read};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::types::StatusResponse;

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

/// Handle the `init` subcommand: ensure daemon is running, proxy SessionStart.
pub fn init() -> eyre::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Write session ID to CLAUDE_ENV_FILE if available
    if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE")
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&input)
        && let Some(session_id) = parsed
            .get("sessionId")
            .or_else(|| parsed.get("session_id"))
            .and_then(|v| v.as_str())
        && let Err(e) = write_env_file(&env_file, session_id)
    {
        eprintln!("warning: failed to write CLAUDE_ENV_FILE: {e}");
    }

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

/// Handle the `summary` subcommand: fetch and print a file summary from the daemon.
pub fn summary(file_path: &str) -> eyre::Result<()> {
    let config = Config::load()?;
    let base_url = format!("http://127.0.0.1:{}", config.port);

    // Ensure daemon is running
    if !daemon_is_running(&base_url) {
        spawn_daemon()?;
        wait_for_daemon(&base_url)?;
    }

    let session_id = std::env::var("YOUWHATKNOW_SESSION").ok();
    let cwd = std::env::current_dir()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut body = serde_json::json!({
        "cwd": cwd,
        "file_path": file_path,
    });

    if let Some(sid) = &session_id {
        body["session_id"] = serde_json::Value::String(sid.clone());
    }

    let resp = client
        .post(format!("{base_url}/hook/summary"))
        .header("content-type", "application/json")
        .body(serde_json::to_string(&body)?)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;

    if !status.is_success() {
        eyre::bail!("daemon returned {status}: {text}");
    }

    print!("{text}");
    Ok(())
}

/// Handle the `reset` subcommand: reset read count for a file.
pub fn reset(file_path: &str, session_override: Option<&str>) -> eyre::Result<()> {
    let session_id = match session_override {
        Some(s) => s.to_owned(),
        None => match std::env::var("YOUWHATKNOW_SESSION") {
            Ok(s) if !s.is_empty() => s,
            _ => {
                eprintln!("error: no session available (set $YOUWHATKNOW_SESSION or use --session)");
                std::process::exit(1);
            }
        },
    };

    let config = Config::load()?;
    let base_url = format!("http://127.0.0.1:{}", config.port);

    // Ensure daemon is running
    if !daemon_is_running(&base_url) {
        spawn_daemon()?;
        wait_for_daemon(&base_url)?;
    }

    let cwd = std::env::current_dir()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let body = serde_json::json!({
        "session_id": session_id,
        "cwd": cwd,
        "file_path": file_path,
    });

    let resp = client
        .post(format!("{base_url}/hook/reset-read"))
        .header("content-type", "application/json")
        .body(serde_json::to_string(&body)?)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;

    if !status.is_success() {
        eyre::bail!("daemon returned {status}: {text}");
    }

    println!("{text}");
    Ok(())
}

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

/// Handle the `logs` subcommand: show daemon log output.
pub fn logs(follow: bool, lines: usize) -> eyre::Result<()> {
    let log_path = crate::config::data_dir().join("daemon.log");

    if !log_path.exists() {
        eprintln!("no log file found at {}", log_path.display());
        eprintln!("daemon may not have been started yet");
        std::process::exit(1);
    }

    if follow {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new("tail")
                .args(["-f", "-n"])
                .arg(lines.to_string())
                .arg(&log_path)
                .exec();
            // exec only returns on error
            eyre::bail!("failed to exec tail: {err}");
        }
        #[cfg(not(unix))]
        {
            eyre::bail!("--follow is only supported on Unix");
        }
    }

    let content = std::fs::read_to_string(&log_path)?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    for line in &all_lines[start..] {
        println!("{line}");
    }

    Ok(())
}

/// Handle the `restart` subcommand: stop and restart the daemon.
pub fn restart() -> eyre::Result<()> {
    let config = Config::load()?;
    let base_url = format!("http://127.0.0.1:{}", config.port);

    if daemon_is_running(&base_url) {
        eprintln!("Stopping daemon...");
        stop_daemon()?;

        // Wait for the daemon to actually stop
        let start = Instant::now();
        let timeout = Duration::from_secs(10);
        while start.elapsed() < timeout {
            if !daemon_is_running(&base_url) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if daemon_is_running(&base_url) {
            eyre::bail!("daemon did not stop within {}s", timeout.as_secs());
        }
    } else {
        eprintln!("Daemon is not running.");
    }

    eprintln!("Starting daemon...");
    spawn_daemon()?;
    wait_for_daemon(&base_url)?;
    eprintln!("Daemon restarted.");
    Ok(())
}

/// Stop the daemon by sending SIGTERM to the PID in the pid file.
fn stop_daemon() -> eyre::Result<()> {
    let pid_path = crate::config::data_dir().join("youwhatknow.pid");
    if !pid_path.exists() {
        eyre::bail!("no PID file found");
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|e| eyre::eyre!("invalid PID in {}: {e}", pid_path.display()))?;

    let status = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()?;

    if !status.success() {
        // Exit code 1 from kill usually means process already gone
        eprintln!("warning: kill exited with {status}");
    }

    Ok(())
}

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
                "cwd": cwd,
                "full": true
            }))?)
            .send()?;

        if !resp.status().is_success() {
            eyre::bail!("reindex failed: {}", resp.status());
        }
    }

    eprintln!("Setup complete.");
    Ok(())
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

fn write_env_file(env_file_path: &str, session_id: &str) -> eyre::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env_file_path)?;
    writeln!(file, "YOUWHATKNOW_SESSION={session_id}")?;
    Ok(())
}

/// Result of merging hooks: the updated settings and count of preserved groups.
struct MergeResult {
    settings: serde_json::Value,
    preserved: usize,
}

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
        cmd.contains("youwhatknow")
            || url.contains("youwhatknow")
            || (url.contains("localhost") && url.ends_with("/hook/pre-read"))
    })
}

/// Merge youwhatknow hooks into existing settings JSON.
/// Removes old youwhatknow entries, appends new ones, preserves everything else.
fn merge_hooks(mut settings: serde_json::Value, port: u16) -> MergeResult {
    let our_hooks = build_hooks_value(port);
    let mut preserved = 0;

    // Ensure settings is an object
    if !settings.is_object() {
        settings = serde_json::json!({});
    }

    let obj = settings.as_object_mut().expect("just verified");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    // If hooks is not an object, replace it
    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }
    let hooks_obj = hooks.as_object_mut().expect("just verified");

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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn daemon_not_running_on_random_port() {
        assert!(!daemon_is_running("http://127.0.0.1:19999"));
    }

    #[test]
    fn write_env_file_creates_entry() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_str().unwrap();
        write_env_file(path, "sess-123").expect("write");
        let content = std::fs::read_to_string(path).expect("read");
        assert!(content.contains("YOUWHATKNOW_SESSION=sess-123"));
    }

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
    fn logs_tails_last_n_lines() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let log = tmp.path().join("daemon.log");
        std::fs::write(&log, "line1\nline2\nline3\nline4\nline5\n").expect("write");

        let content = std::fs::read_to_string(&log).expect("read");
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(3);
        let tailed: Vec<&str> = all_lines[start..].to_vec();

        assert_eq!(tailed, vec!["line3", "line4", "line5"]);
    }

    #[test]
    fn logs_fewer_lines_than_requested() {
        let content = "line1\nline2\n";
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(50);
        let tailed: Vec<&str> = all_lines[start..].to_vec();

        assert_eq!(tailed, vec!["line1", "line2"]);
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
}
