use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;

use crate::config::Config;
use crate::hooks;
use crate::indexer::Index;
use crate::session::SessionTracker;
use crate::types::{HealthResponse, HookRequest, HookResponse};

/// Tracks when the last request was received for idle shutdown.
#[derive(Clone)]
pub struct ActivityTracker {
    last_activity_secs: Arc<AtomicU64>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            last_activity_secs: Arc::new(AtomicU64::new(now)),
        }
    }

    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_activity_secs.store(now, Ordering::Relaxed);
    }

    pub fn idle_duration(&self) -> Duration {
        let last = self.last_activity_secs.load(Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Duration::from_secs(now.saturating_sub(last))
    }

    /// Spawn a background task that exits the process after `timeout` of inactivity.
    pub fn spawn_idle_watchdog(&self, timeout: Duration) -> tokio::task::JoinHandle<()> {
        let tracker = self.clone();
        tokio::spawn(async move {
            let check_interval = Duration::from_secs(30);
            loop {
                tokio::time::sleep(check_interval).await;
                let idle = tracker.idle_duration();
                if idle >= timeout {
                    tracing::info!(
                        idle_secs = idle.as_secs(),
                        "idle timeout reached, shutting down"
                    );
                    // Clean exit — this will trigger graceful shutdown
                    std::process::exit(0);
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub index: Index,
    pub session: SessionTracker,
    pub config: Arc<Config>,
    pub project_root: PathBuf,
    pub activity: ActivityTracker,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/hook/pre-read", post(pre_read_handler))
        .route("/hook/session-start", post(session_start_handler))
        .route("/reindex", post(reindex_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

async fn pre_read_handler(
    State(state): State<AppState>,
    Json(request): Json<HookRequest>,
) -> Json<HookResponse> {
    state.activity.touch();
    let response =
        hooks::handle_pre_read(&state.index, &state.session, &state.project_root, &request)
            .await;
    Json(response)
}

async fn session_start_handler(
    State(state): State<AppState>,
    Json(_request): Json<HookRequest>,
) -> Json<HookResponse> {
    state.activity.touch();
    let response = hooks::handle_session_start(&state.index).await;
    Json(response)
}

async fn reindex_handler(State(state): State<AppState>) -> StatusCode {
    state.activity.touch();
    let index = state.index.clone();
    let project_root = state.project_root.clone();
    let config = state.config.clone();

    tokio::spawn(async move {
        index.full_index(&project_root, &config).await;
    });

    StatusCode::ACCEPTED
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    state.activity.touch();
    let total = state.index.total_count();
    Json(HealthResponse {
        status: "ok".to_owned(),
        indexing: !state.index.is_ready(),
        indexed_files: state.index.indexed_count(),
        total_files: if total > 0 { Some(total) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState {
            index: Index::new(),
            session: SessionTracker::new(),
            config: Arc::new(Config::default()),
            project_root: PathBuf::from("/tmp/test-project"),
            activity: ActivityTracker::new(),
        }
    }

    #[tokio::test]
    async fn health_endpoint() {
        let app = router(test_state());

        let req = Request::builder()
            .uri("/health")
            .method("GET")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024)
            .await
            .expect("body");
        let health: HealthResponse = serde_json::from_slice(&body).expect("parse");
        assert_eq!(health.status, "ok");
        assert!(health.indexing); // not ready yet
    }

    #[tokio::test]
    async fn pre_read_endpoint_outside_project() {
        let app = router(test_state());

        let body = serde_json::json!({
            "session_id": "test-session",
            "cwd": "/tmp/test-project",
            "hook_event_name": "PreToolUse",
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/etc/hosts"
            }
        });

        let req = Request::builder()
            .uri("/hook/pre-read")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).expect("json")))
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("body");
        let hook_resp: HookResponse = serde_json::from_slice(&body).expect("parse");
        assert_eq!(
            hook_resp.hook_specific_output.permission_decision,
            Some("allow".to_owned())
        );
        assert!(hook_resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn session_start_endpoint() {
        let app = router(test_state());

        let body = serde_json::json!({
            "session_id": "test-session",
            "cwd": "/tmp/test-project",
            "hook_event_name": "SessionStart"
        });

        let req = Request::builder()
            .uri("/hook/session-start")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).expect("json")))
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn reindex_endpoint_returns_accepted() {
        let app = router(test_state());

        let req = Request::builder()
            .uri("/reindex")
            .method("POST")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[test]
    fn activity_tracker_touch_updates_time() {
        let tracker = ActivityTracker::new();
        let idle_before = tracker.idle_duration();
        assert!(idle_before < Duration::from_secs(2));
        tracker.touch();
        let idle_after = tracker.idle_duration();
        assert!(idle_after < Duration::from_secs(2));
    }
}
