use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::Instant;

/// Tracks read count and recency for a single (session, file) pair.
struct ReadEntry {
    /// Number of tracked reads for this file in this session.
    count: u32,
    /// Sequence number at the time of the last tracked read.
    last_seq: u64,
}

/// Tracks per-session file read counts.
#[derive(Clone)]
pub struct SessionTracker {
    inner: Arc<RwLock<SessionState>>,
}

struct SessionState {
    /// (session_id, file_path) -> read tracking entry
    reads: HashMap<(String, PathBuf), ReadEntry>,
    /// session_id -> last activity time
    last_activity: HashMap<String, Instant>,
    /// session_id -> monotonic sequence counter
    sequences: HashMap<String, u64>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SessionState {
                reads: HashMap::new(),
                last_activity: HashMap::new(),
                sequences: HashMap::new(),
            })),
        }
    }

    /// Record a read and return the new count for this (session, file) pair.
    /// If the file is stale (last_seq + threshold < current_seq), resets count to 0 first.
    pub async fn track_read(
        &self,
        session_id: &str,
        file_path: &Path,
        eviction_threshold: u32,
    ) -> u32 {
        let mut state = self.inner.write().await;

        // Increment session sequence
        let seq = state
            .sequences
            .entry(session_id.to_owned())
            .or_insert(0);
        *seq += 1;
        let current_seq = *seq;

        let key = (session_id.to_owned(), file_path.to_path_buf());
        let entry = state.reads.entry(key).or_insert(ReadEntry {
            count: 0,
            last_seq: 0,
        });

        // Evict if stale
        if eviction_threshold > 0
            && entry.count > 0
            && current_seq - entry.last_seq > u64::from(eviction_threshold)
        {
            entry.count = 0;
        }

        entry.count += 1;
        entry.last_seq = current_seq;

        let result = entry.count;

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

        // Get current sequence for this session (don't increment -- summaries don't count as reads)
        let current_seq = *state
            .sequences
            .entry(session_id.to_owned())
            .or_insert(0);

        let key = (session_id.to_owned(), file_path.to_path_buf());
        let entry = state.reads.entry(key).or_insert(ReadEntry {
            count: 0,
            last_seq: 0,
        });

        if entry.count == 0 {
            entry.count = 1;
            entry.last_seq = current_seq;
            state
                .last_activity
                .insert(session_id.to_owned(), Instant::now());
            true
        } else {
            false
        }
    }

    /// Number of tracked sessions (not yet cleaned up).
    pub async fn session_count(&self) -> usize {
        self.inner.read().await.last_activity.len()
    }

    /// Get the current read count without incrementing.
    #[cfg(test)]
    pub async fn read_count(&self, session_id: &str, file_path: &Path) -> u32 {
        let state = self.inner.read().await;
        let key = (session_id.to_owned(), file_path.to_path_buf());
        state.reads.get(&key).map(|e| e.count).unwrap_or(0)
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
            state.sequences.remove(session_id);
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

        assert_eq!(tracker.track_read("s1", &path, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &path, 40).await, 2);
        assert_eq!(tracker.track_read("s1", &path, 40).await, 3);
    }

    #[tokio::test]
    async fn different_sessions_independent() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        assert_eq!(tracker.track_read("s1", &path, 40).await, 1);
        assert_eq!(tracker.track_read("s2", &path, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &path, 40).await, 2);
        assert_eq!(tracker.track_read("s2", &path, 40).await, 2);
    }

    #[tokio::test]
    async fn different_files_independent() {
        let tracker = SessionTracker::new();
        let p1 = PathBuf::from("src/main.rs");
        let p2 = PathBuf::from("src/lib.rs");

        assert_eq!(tracker.track_read("s1", &p1, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &p2, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &p1, 40).await, 2);
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

        tracker.track_read("s1", &path, 40).await;
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

        let count = tracker.track_read("s1", &path, 40).await;
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn session_count_reflects_active_sessions() {
        let tracker = SessionTracker::new();
        assert_eq!(tracker.session_count().await, 0);

        tracker.track_read("session-1", Path::new("/a.rs"), 40).await;
        assert_eq!(tracker.session_count().await, 1);

        tracker.track_read("session-2", Path::new("/b.rs"), 40).await;
        assert_eq!(tracker.session_count().await, 2);

        // Same session, different file — still 2
        tracker.track_read("session-1", Path::new("/c.rs"), 40).await;
        assert_eq!(tracker.session_count().await, 2);
    }

    #[tokio::test]
    async fn cleanup_removes_stale_sessions() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        tracker.track_read("s1", &path, 40).await;

        // Cleanup with zero timeout should remove everything
        tracker.cleanup(Duration::from_secs(0)).await;
        assert_eq!(tracker.read_count("s1", &path).await, 0);
    }

    #[tokio::test]
    async fn track_read_evicts_stale_file() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        // Read target twice: count = 2
        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);

        // Read 41 other distinct files (threshold = 40, need > 40 intervening)
        for i in 0..41 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // Read target again: should be evicted, count resets to 1
        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);
    }

    #[tokio::test]
    async fn track_read_does_not_evict_recent_file() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);

        // Only 39 intervening reads (< threshold of 40)
        for i in 0..39 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // Target should NOT be evicted: count = 3
        assert_eq!(tracker.track_read("s1", &target, 40).await, 3);
    }

    #[tokio::test]
    async fn track_read_eviction_disabled_when_threshold_zero() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        assert_eq!(tracker.track_read("s1", &target, 0).await, 1);
        assert_eq!(tracker.track_read("s1", &target, 0).await, 2);

        // 100 intervening reads with threshold=0 should NOT evict
        for i in 0..100 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 0).await;
        }

        assert_eq!(tracker.track_read("s1", &target, 0).await, 3);
    }

    #[tokio::test]
    async fn track_read_boundary_at_exact_threshold() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        // Read target twice: count=2, seq=2
        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);

        // Read exactly 40 other files: seq advances to 42
        for i in 0..40 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // Read target: seq=43, 43-2=41 > 40 => EVICTED, count=1
        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);

        // Now target is at seq=43, count=1. Read again: count=2, seq=44
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);

        // Read exactly 39 other files: seq advances to 83
        for i in 0..39 {
            let other = PathBuf::from(format!("src/x_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // Read target: seq=84, 84-44=40 which is NOT > 40 => NOT evicted, count=3
        assert_eq!(tracker.track_read("s1", &target, 40).await, 3);
    }

    #[tokio::test]
    async fn eviction_does_not_affect_other_sessions() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        // Session s1: read target twice
        tracker.track_read("s1", &target, 40).await;
        tracker.track_read("s1", &target, 40).await;

        // Session s2: read target twice
        tracker.track_read("s2", &target, 40).await;
        tracker.track_read("s2", &target, 40).await;

        // Session s1: 41 intervening reads (evicts target in s1)
        for i in 0..41 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // s1 target is evicted
        assert_eq!(tracker.track_read("s1", &target, 40).await, 1);
        // s2 target is NOT evicted (no intervening reads in s2)
        assert_eq!(tracker.track_read("s2", &target, 40).await, 3);
    }

    #[tokio::test]
    async fn track_summary_updates_last_seq() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        // track_summary as first interaction sets last_seq to current session seq (0)
        tracker.track_summary("s1", &target).await;

        // Read 39 other files (< threshold of 40)
        for i in 0..39 {
            let other = PathBuf::from(format!("src/other_{i}.rs"));
            tracker.track_read("s1", &other, 40).await;
        }

        // track_read for target: seq=40, 40-0=40 which is NOT > 40 => not evicted, count=2
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);
    }

    #[tokio::test]
    async fn track_summary_first_interaction_creates_sequence() {
        let tracker = SessionTracker::new();
        let target = PathBuf::from("src/target.rs");

        // track_summary as the very first call for this session
        assert!(tracker.track_summary("s1", &target).await);
        assert_eq!(tracker.read_count("s1", &target).await, 1);

        // Subsequent track_read should see count=2 (not evicted)
        assert_eq!(tracker.track_read("s1", &target, 40).await, 2);
    }

    #[tokio::test]
    async fn cleanup_removes_sequences() {
        let tracker = SessionTracker::new();
        let path = PathBuf::from("src/main.rs");

        tracker.track_read("s1", &path, 40).await;

        // Cleanup with zero timeout should remove everything
        tracker.cleanup(Duration::from_secs(0)).await;
        assert_eq!(tracker.read_count("s1", &path).await, 0);

        // After cleanup, re-reading should start fresh at count=1
        assert_eq!(tracker.track_read("s1", &path, 40).await, 1);
    }
}
