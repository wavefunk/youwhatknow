use std::path::Path;

use crate::config::ProjectConfig;
use crate::indexer::Index;
use crate::session::SessionTracker;
use crate::summary;
use crate::types::{HookRequest, HookResponse};

/// Handle a PreToolUse hook for the Read tool.
///
/// Gating strategy (steps 1-5 are early exits with no tracking):
/// 1. No tool_input → allow, no context
/// 2. File outside project (strip_prefix fails) → allow, no context
/// 3. No summary in index → allow, no context
/// 4. Targeted read (offset or limit present) → allow, no context
/// 5. Small file (line_count <= threshold) → allow, no context
///
/// 6-7. track_read → count 1 deny with summary; count 2 allow clean; count 3+ allow with nudge
pub async fn handle_pre_read(
    index: &Index,
    session: &SessionTracker,
    project_root: &Path,
    config: &ProjectConfig,
    request: &HookRequest,
) -> HookResponse {
    let Some(tool_input) = &request.tool_input else {
        return HookResponse::allow_no_context("PreToolUse");
    };

    let file_path = &tool_input.file_path;

    // 1. Files outside the project
    let rel_path = match file_path.strip_prefix(project_root) {
        Ok(rel) => rel,
        Err(_) => return HookResponse::allow_no_context("PreToolUse"),
    };

    // 2. No summary available
    let Some(file_summary) = index.lookup_file(rel_path).await else {
        return HookResponse::allow_no_context("PreToolUse");
    };

    // 3. Targeted read (offset or limit present)
    if tool_input.offset.is_some() || tool_input.limit.is_some() {
        return HookResponse::allow_no_context("PreToolUse");
    }

    // 4. Small file (under threshold)
    if file_summary.line_count <= config.line_threshold {
        return HookResponse::allow_no_context("PreToolUse");
    }

    // 5-7: track the read
    let read_count = session
        .track_read(&request.session_id, file_path, config.eviction_threshold)
        .await;

    if read_count == 1 {
        // First read: deny with summary + line-range map
        let rendered = summary::render_file_summary(&file_summary, config);
        let reason = format!(
            "-- youwhatknow: {} --\n{}\n\
             If this summary is sufficient, do not read the file. \
             If you need the full file contents, read it again.",
            rel_path.display(),
            rendered,
        );
        HookResponse::deny_with_reason("PreToolUse", reason)
    } else if read_count == 2 {
        // Second read: allow, clean pass
        HookResponse::allow_no_context("PreToolUse")
    } else {
        // Third+ read: allow with nudge
        let context = format!(
            "This file has been read {} times this session. \
             Consider using offset/limit for targeted reads.",
            read_count
        );
        HookResponse::allow_with_context("PreToolUse", context)
    }
}

/// Handle a SessionStart hook.
pub async fn handle_session_start(index: &Index, config: &ProjectConfig) -> HookResponse {
    let map = summary::render_project_map(index).await;

    if map.is_empty() {
        let context = if !index.is_ready() {
            "-- youwhatknow: indexing in progress --\nProject summaries are being generated."
        } else {
            "-- youwhatknow: no summaries available --"
        };
        return HookResponse::session_start_context(context.to_owned());
    }

    let mut context = String::from("-- youwhatknow: project map --\n");
    context.push_str(&map);

    context.push_str("\n\n-- youwhatknow: instructions --\n");
    context.push_str(&summary::render_session_instructions(config));

    if !index.is_ready() {
        context.push_str("\n(indexing in progress — some summaries may be incomplete)");
    }

    HookResponse::session_start_context(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::Index;
    use crate::session::SessionTracker;
    use crate::types::{FileSummary, FolderSummary, LineRange, ToolInput};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Helper: create an index with a single file at src/main.rs.
    async fn setup_index(line_count: u32, line_ranges: Vec<LineRange>) -> Index {
        let index = Index::new();

        index
            .insert_file(FileSummary {
                path: PathBuf::from("src/main.rs"),
                description: "Entry point for the application".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count,
                line_ranges,
                summarized: Utc::now(),
            })
            .await;

        index
            .insert_folder(
                "src".to_owned(),
                FolderSummary {
                    generated: Utc::now(),
                    description: "Core logic".to_owned(),
                    files: HashMap::new(),
                },
            )
            .await;

        index.set_ready(true);
        index
    }

    fn default_config() -> ProjectConfig {
        ProjectConfig {
            line_threshold: 30,
            ..ProjectConfig::default()
        }
    }

    fn large_line_ranges() -> Vec<LineRange> {
        vec![
            LineRange {
                start: 1,
                end: 50,
                label: "Imports and setup".to_owned(),
            },
            LineRange {
                start: 52,
                end: 150,
                label: "Core logic".to_owned(),
            },
            LineRange {
                start: 152,
                end: 227,
                label: "Tests".to_owned(),
            },
        ]
    }

    #[tokio::test]
    async fn small_file_always_allowed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(15, vec![]).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: None,
                limit: None,
            }),
        };

        // First call: allowed (small file, no tracking)
        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());

        // Second call: still allowed, still no context (no tracking happened)
        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn targeted_read_always_allowed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(227, large_line_ranges()).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: Some(50),
                limit: Some(20),
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn large_file_first_read_denied_with_summary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(227, large_line_ranges()).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: None,
                limit: None,
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("deny")
        );

        let reason = resp
            .hook_specific_output
            .permission_decision_reason
            .expect("should have reason");
        assert!(reason.contains("227 lines"), "should contain line count");
        assert!(
            reason.contains("Entry point for the application"),
            "should contain description"
        );
        assert!(reason.contains("1-50"), "should contain line ranges");
        assert!(
            reason.contains("offset/limit"),
            "should mention offset/limit"
        );
    }

    #[tokio::test]
    async fn large_file_second_read_allowed_clean() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(227, large_line_ranges()).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: None,
                limit: None,
            }),
        };

        // First read — denied
        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("deny")
        );

        // Second read — allowed, no context
        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn large_file_third_read_allowed_with_nudge() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(227, large_line_ranges()).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: None,
                limit: None,
            }),
        };

        // First read — denied
        handle_pre_read(&index, &session, root, &config, &request).await;
        // Second read — allowed clean
        handle_pre_read(&index, &session, root, &config, &request).await;

        // Third read — allowed with nudge
        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(ctx.contains("3 times"), "should mention read count");
    }

    #[tokio::test]
    async fn outside_project_always_allowed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(227, large_line_ranges()).await;
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: PathBuf::from("/etc/hosts"),
                offset: None,
                limit: None,
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn no_summary_always_allowed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = Index::new(); // empty index
        let session = SessionTracker::new();
        let config = default_config();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
                offset: None,
                limit: None,
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &config, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn session_start_with_index() {
        let index = setup_index(52, vec![]).await;
        let config = default_config();

        let resp = handle_session_start(&index, &config).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(
            ctx.contains("-- youwhatknow: project map --"),
            "should have project map header"
        );
        assert!(ctx.contains("main.rs"), "should list files");
        assert!(
            ctx.contains("-- youwhatknow: instructions --"),
            "should have instructions"
        );
        assert!(
            ctx.contains("youwhatknow summary"),
            "should mention youwhatknow summary"
        );
    }

    #[tokio::test]
    async fn session_start_indexing_in_progress() {
        let index = Index::new(); // empty, not ready
        let config = default_config();

        let resp = handle_session_start(&index, &config).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(
            ctx.contains("indexing in progress"),
            "should mention indexing"
        );
    }
}
