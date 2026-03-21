mod config;
mod hooks;
mod indexer;
mod session;
mod server;
mod storage;
mod types;

use std::net::SocketAddr;
use std::sync::Arc;

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

    // Create index and load existing summaries
    let index = indexer::Index::new();
    index.load_from_disk(&project_root, &config).await;

    // Create session tracker with cleanup task
    let session = session::SessionTracker::new();
    session.spawn_cleanup_task(config.session_timeout_minutes);

    // Spawn background indexing
    let bg_index = index.clone();
    let bg_root = project_root.clone();
    let bg_config = config.clone();
    tokio::spawn(async move {
        // Check if we have a .last-run to decide full vs incremental
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
