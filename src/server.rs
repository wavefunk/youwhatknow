use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;

use crate::config::Config;
use crate::hooks;
use crate::registry::ProjectRegistry;
use crate::session::SessionTracker;
use crate::summary;
use crate::types::{HealthResponse, HookRequest, HookResponse, StatusResponse, SummaryRequest};

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
    pub config: Arc<Config>,
    pub activity: ActivityTracker,
    pub started_at: Instant,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/hook/pre-read", post(pre_read_handler))
        .route("/hook/session-start", post(session_start_handler))
        .route("/hook/summary", post(summary_handler))
        .route("/reindex", post(reindex_handler))
        .route("/health", get(health_handler))
        .route("/status", get(status_handler))
        .with_state(state)
}

async fn pre_read_handler(
    State(state): State<AppState>,
    Json(request): Json<HookRequest>,
) -> Json<HookResponse> {
    state.activity.touch();
    let (index, config) = state.registry.get_or_load(&request.cwd).await;
    let response =
        hooks::handle_pre_read(&index, &state.session, &request.cwd, &config, &request).await;
    Json(response)
}

async fn session_start_handler(
    State(state): State<AppState>,
    Json(request): Json<HookRequest>,
) -> Json<HookResponse> {
    state.activity.touch();
    let (index, config) = state.registry.get_or_load(&request.cwd).await;
    let response = hooks::handle_session_start(&index, &config).await;
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

async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    // Intentionally does NOT call activity.touch() —
    // polling status must not prevent idle shutdown.
    Json(StatusResponse {
        pid: std::process::id(),
        port: state.config.port,
        uptime_secs: state.started_at.elapsed().as_secs(),
        idle_secs: state.activity.idle_duration().as_secs(),
        active_sessions: state.session.session_count().await,
        loaded_projects: state.registry.project_count().await,
        idle_shutdown_minutes: state.config.idle_shutdown_minutes,
    })
}

async fn summary_handler(
    State(state): State<AppState>,
    Json(request): Json<SummaryRequest>,
) -> String {
    state.activity.touch();

    if request.session_id.is_none() {
        tracing::warn!("summary request without session_id");
    }

    let (index, config) = state.registry.get_or_load(&request.cwd).await;

    let Some(file_summary) = index.lookup_file(&request.file_path).await else {
        return format!(
            "No summary available for {}. Try running `youwhatknow init` to trigger reindexing.",
            request.file_path.display()
        );
    };

    // Conditionally set read count to 1 if session provided
    if let Some(session_id) = &request.session_id {
        let abs_path = request.cwd.join(&request.file_path);
        state.session.track_summary(session_id, &abs_path).await;
    }

    summary::render_file_summary(&file_summary, &config)
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
            started_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn status_endpoint_returns_status() {
        let app = router(test_state());

        let req = Request::builder()
            .uri("/status")
            .method("GET")
            .body(Body::empty())
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let status: crate::types::StatusResponse =
            serde_json::from_slice(&body).expect("deserialize");

        assert_eq!(status.pid, std::process::id());
        assert_eq!(status.port, Config::default().port);
        assert!(status.uptime_secs < 5);
        assert!(status.idle_secs < 5);
        assert_eq!(status.active_sessions, 0);
        assert_eq!(status.loaded_projects, 0);
        assert_eq!(
            status.idle_shutdown_minutes,
            Config::default().idle_shutdown_minutes
        );
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

    #[tokio::test]
    async fn summary_endpoint_returns_text() {
        use std::path::Path;

        let state = test_state();
        let (index, _) = state
            .registry
            .get_or_load(Path::new("/tmp/test-project"))
            .await;
        index
            .insert_file(crate::types::FileSummary {
                path: std::path::PathBuf::from("src/main.rs"),
                description: "Entry point".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count: 52,
                line_ranges: vec![],
                summarized: chrono::Utc::now(),
            })
            .await;

        let app = router(state);

        let body = serde_json::json!({
            "session_id": "test-session",
            "cwd": "/tmp/test-project",
            "file_path": "src/main.rs"
        });

        let req = Request::builder()
            .uri("/hook/summary")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).expect("json")))
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("Entry point"));
    }

    #[tokio::test]
    async fn summary_endpoint_no_summary() {
        let app = router(test_state());

        let body = serde_json::json!({
            "cwd": "/tmp/test-project",
            "file_path": "nonexistent.rs"
        });

        let req = Request::builder()
            .uri("/hook/summary")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).expect("json")))
            .expect("request");

        let resp = app.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("reindex"));
    }
}
