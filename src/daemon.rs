use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        started_at: Instant::now(),
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
            _ = activity.wait_for_idle_timeout(timeout) => {
                // idle shutdown logged inside wait_for_idle_timeout
            }
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
