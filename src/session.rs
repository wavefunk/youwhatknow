use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::Instant;

/// Tracks per-session file read counts.
#[derive(Clone)]
pub struct SessionTracker {
    inner: Arc<RwLock<SessionState>>,
}

struct SessionState {
    /// (session_id, file_path) -> read count
    reads: HashMap<(String, PathBuf), u32>,
    /// session_id -> last activity time
    last_activity: HashMap<String, Instant>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SessionState {
                reads: HashMap::new(),
                last_activity: HashMap::new(),
            })),
        }
    }

    /// Record a read and return the new count for this (session, file) pair.
    pub async fn track_read(&self, session_id: &str, file_path: &Path) -> u32 {
        let mut state = self.inner.write().await;
        let key = (session_id.to_owned(), file_path.to_path_buf());
        let count = state.reads.entry(key).or_insert(0);
        *count += 1;
        let result = *count;
        state
            .last_activity
            .insert(session_id.to_owned(), Instant::now());
        result
    }

    /// Set the read count to 1 if currently 0. Used by the CLI summary endpoint
    /// so that a subsequent Read hook sees count=2 and allows through.
    /// Returns true if the count was set (was 0), false if already tracked.
    pub async fn track_summary(&self, session_id: &str, file_path: &Path) -> bool {
        let mut state = self.inner.write().await;
        let key = (session_id.to_owned(), file_path.to_path_buf());
        let count = state.reads.entry(key).or_insert(0);
        if *count == 0 {
            *count = 1;
            state
                .last_activity
                .insert(session_id.to_owned(), Instant::now());
            true
        } else {
            false
        }
    }

    /// Get the current read count without incrementing.
    #[cfg(test)]
    pub async fn read_count(&self, session_id: &str, file_path: &Path) -> u32 {
        let state = self.inner.read().await;
        let key = (session_id.to_owned(), file_path.to_path_buf());
        state.reads.get(&key).copied().unwrap_or(0)
    }

    /// Remove sessions that haven't been active for `timeout`.
    async fn cleanup(&self, timeout: Duration) {
        let mut state = self.inner.write().await;
        let cutoff = Instant::now() - timeout;

        let stale_sessions: Vec<String> = state
            .last_activity
            .iter()
            .filter(|&(_, last)| *last < cutoff)
            .map(|(id, _)| id.clone())
            .collect();

        for session_id in &stale_sessions {
            state.last_activity.remove(session_id);
            state.reads.retain(|(sid, _), _| sid != session_id);
        }

        if !stale_sessions.is_empty() {
            tracing::info!(
                count = stale_sessions.len(),
                "cleaned up stale sessions"
            );
        }
    }

    /// Spawn a background task that periodically cleans up stale sessions.
    pub fn spawn_cleanup_task(&self, timeout_minutes: u64) -> tokio::task::JoinHandle<()> {
        let tracker = self.clone();
        let timeout = Duration::from_secs(timeout_minutes * 60);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600)); // every 10 min
            loop {
                interval.tick().await;
                tracker.cleanup(timeout).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn track_read_increments() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        assert_eq!(tracker.track_read("s1", &path).await, 1);
        assert_eq!(tracker.track_read("s1", &path).await, 2);
        assert_eq!(tracker.track_read("s1", &path).await, 3);
    }

    #[tokio::test]
    async fn different_sessions_independent() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        assert_eq!(tracker.track_read("s1", &path).await, 1);
        assert_eq!(tracker.track_read("s2", &path).await, 1);
        assert_eq!(tracker.track_read("s1", &path).await, 2);
        assert_eq!(tracker.track_read("s2", &path).await, 2);
    }

    #[tokio::test]
    async fn different_files_independent() {
        let tracker = SessionTracker::new();
        let p1 = PathBuf::from("src/main.rs");
        let p2 = PathBuf::from("src/lib.rs");

        assert_eq!(tracker.track_read("s1", &p1).await, 1);
        assert_eq!(tracker.track_read("s1", &p2).await, 1);
        assert_eq!(tracker.track_read("s1", &p1).await, 2);
        assert_eq!(tracker.read_count("s1", &p2).await, 1);
    }

    #[tokio::test]
    async fn track_summary_sets_count_to_one_if_zero() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        let set = tracker.track_summary("s1", &path).await;
        assert!(set);
        assert_eq!(tracker.read_count("s1", &path).await, 1);
    }

    #[tokio::test]
    async fn track_summary_does_not_increment_if_already_tracked() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        tracker.track_read("s1", &path).await;
        assert_eq!(tracker.read_count("s1", &path).await, 1);

        let set = tracker.track_summary("s1", &path).await;
        assert!(!set);
        assert_eq!(tracker.read_count("s1", &path).await, 1);
    }

    #[tokio::test]
    async fn track_summary_then_read_allows() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        tracker.track_summary("s1", &path).await;
        assert_eq!(tracker.read_count("s1", &path).await, 1);

        let count = tracker.track_read("s1", &path).await;
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn cleanup_removes_stale_sessions() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        tracker.track_read("s1", &path).await;

        // Cleanup with zero timeout should remove everything
        tracker.cleanup(Duration::from_secs(0)).await;
        assert_eq!(tracker.read_count("s1", &path).await, 0);
    }
}
