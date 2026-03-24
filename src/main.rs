mod cli;
mod config;
mod daemon;
mod hooks;
mod indexer;
mod registry;
mod session;
mod server;
mod storage;
mod summary;
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
    /// Show file summary without triggering a read
    Summary {
        /// File path relative to cwd
        path: String,
    },
    /// Initialize a project to use youwhatknow
    Setup {
        /// Write hooks to .claude/settings.json (shared with team)
        #[arg(long, conflicts_with = "local")]
        shared: bool,
        /// Write hooks to .claude/settings.local.json (default, per-developer)
        #[arg(long, conflicts_with = "shared")]
        local: bool,
        /// Skip initial indexing
        #[arg(long)]
        no_index: bool,
    },
    /// Show daemon status
    Status,
    /// Show daemon logs
    Logs {
        /// Follow log output (like tail -f)
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show (default: 50)
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
    /// Restart the daemon
    Restart,
    /// Reset read count for a file (shows summary again on next read)
    Reset {
        /// File path relative to cwd
        path: String,
        /// Session ID (defaults to $YOUWHATKNOW_SESSION)
        #[arg(long)]
        session: Option<String>,
    },
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Init) => {
            // No tracing for CLI — stdout is for Claude Code
            cli::init()
        }
        Some(Command::Summary { path }) => {
            // No tracing for CLI — stdout is for Claude Code
            cli::summary(&path)
        }
        Some(Command::Setup { shared, local: _, no_index }) => {
            cli::setup(shared, no_index)
        }
        Some(Command::Status) => cli::status(),
        Some(Command::Logs { follow, lines }) => cli::logs(follow, lines),
        Some(Command::Restart) => cli::restart(),
        Some(Command::Reset { path, session }) => cli::reset(&path, session.as_deref()),
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
