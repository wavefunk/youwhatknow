use std::io::{self, Read};
use std::time::{Duration, Instant};

use crate::config::Config;

/// Handle the `init` subcommand: ensure daemon is running, proxy SessionStart.
pub fn init() -> eyre::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Write session ID to CLAUDE_ENV_FILE if available
    if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&input) {
            if let Some(session_id) = parsed
                .get("sessionId")
                .or_else(|| parsed.get("session_id"))
                .and_then(|v| v.as_str())
            {
                if let Err(e) = write_env_file(&env_file, session_id) {
                    eprintln!("warning: failed to write CLAUDE_ENV_FILE: {e}");
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
