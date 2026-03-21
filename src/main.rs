mod config;
mod hooks;
mod indexer;
mod session;
mod server;
mod storage;
mod types;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eyre::Context;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Set up tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Determine project root (cwd)
    let project_root = std::env::current_dir().context("getting current directory")?;
    tracing::info!(root = %project_root.display(), "starting youwhatknow");

    // Load config
    let config = config::Config::load(&project_root)?;
    tracing::info!(port = config.port, "loaded config");

    // Write PID file
    let pid_file = pid_file_path(&project_root, &config);
    write_pid_file(&pid_file)?;

    // Clean up PID file on exit
    let pid_file_cleanup = pid_file.clone();
    let _pid_guard = scopeguard(move || {
        let _ = std::fs::remove_file(&pid_file_cleanup);
    });

    // Create index and load existing summaries
    let index = indexer::Index::new();
    index.load_from_disk(&project_root, &config).await;

    // Create session tracker with cleanup task
    let session = session::SessionTracker::new();
    session.spawn_cleanup_task(config.session_timeout_minutes);

    // Create activity tracker and idle watchdog
    let activity = server::ActivityTracker::new();
    if config.idle_shutdown_minutes > 0 {
        let timeout = Duration::from_secs(config.idle_shutdown_minutes * 60);
        activity.spawn_idle_watchdog(timeout);
        tracing::info!(
            minutes = config.idle_shutdown_minutes,
            "idle shutdown enabled"
        );
    }

    // Spawn background indexing
    let bg_index = index.clone();
    let bg_root = project_root.clone();
    let bg_config = config.clone();
    tokio::spawn(async move {
        let summary_dir = bg_root.join(&bg_config.summary_path);
        if storage::read_last_run(&summary_dir).is_some() {
            bg_index.incremental_index(&bg_root, &bg_config).await;
        } else {
            bg_index.full_index(&bg_root, &bg_config).await;
        }
    });

    // Build app state and router
    let state = server::AppState {
        index,
        session,
        config: Arc::new(config.clone()),
        project_root,
        activity,
    };
    let app = server::router(state);

    // Bind and serve
    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;

    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("running server")?;

    tracing::info!("shutting down");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("received shutdown signal");
}

/// Path to the PID file for this project instance.
fn pid_file_path(project_root: &Path, config: &config::Config) -> PathBuf {
    project_root
        .join(&config.summary_path)
        .join("youwhatknow.pid")
}

/// Write current process PID to file.
fn write_pid_file(path: &Path) -> eyre::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

/// Simple drop guard that runs a closure on drop.
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
