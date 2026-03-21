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
use crate::registry::ProjectRegistry;
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

    /// Spawn a background task that signals shutdown after `timeout` of inactivity.
    /// Returns a future that resolves when idle timeout is reached.
    pub async fn wait_for_idle_timeout(&self, timeout: Duration) {
        let check_interval = Duration::from_secs(30);
        loop {
            tokio::time::sleep(check_interval).await;
            let idle = self.idle_duration();
            if idle >= timeout {
                tracing::info!(
                    idle_secs = idle.as_secs(),
                    "idle timeout reached, shutting down"
                );
                return;
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub registry: ProjectRegistry,
    pub session: SessionTracker,
    #[allow(dead_code)]
    pub config: Arc<Config>,
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
    let (index, _config) = state.registry.get_or_load(&request.cwd).await;
    let response =
        hooks::handle_pre_read(&index, &state.session, &request.cwd, &request).await;
    Json(response)
}

async fn session_start_handler(
    State(state): State<AppState>,
    Json(request): Json<HookRequest>,
) -> Json<HookResponse> {
    state.activity.touch();
    let (index, _config) = state.registry.get_or_load(&request.cwd).await;
    let response = hooks::handle_session_start(&index).await;
    Json(response)
}

async fn reindex_handler(
    State(state): State<AppState>,
    Json(request): Json<HookRequest>,
) -> StatusCode {
    state.activity.touch();
    state.registry.reindex(&request.cwd).await;
    StatusCode::ACCEPTED
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    state.activity.touch();
    let projects = state.registry.project_count().await;
    Json(HealthResponse {
        status: "ok".to_owned(),
        projects,
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
            registry: ProjectRegistry::new(),
            session: SessionTracker::new(),
            config: Arc::new(Config::default()),
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
