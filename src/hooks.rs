use std::path::Path;

use crate::indexer::Index;
use crate::session::SessionTracker;
use crate::storage;
use crate::types::{HookRequest, HookResponse};

/// Handle a PreToolUse hook for the Read tool.
///
/// Strategy:
/// - No summary available → allow (nothing useful to provide)
/// - First read with summary → deny with summary, instruct to read again if needed
/// - Second+ read → allow with summary context (Claude insisted, let it through)
pub async fn handle_pre_read(
    index: &Index,
    session: &SessionTracker,
    project_root: &Path,
    request: &HookRequest,
) -> HookResponse {
    let Some(tool_input) = &request.tool_input else {
        return HookResponse::allow_no_context("PreToolUse");
    };

    let file_path = &tool_input.file_path;

    // Files outside the project — allow without interference
    let rel_path = match file_path.strip_prefix(project_root) {
        Ok(rel) => rel,
        Err(_) => return HookResponse::allow_no_context("PreToolUse"),
    };

    // No summary available — allow the read
    let Some(file_summary) = index.lookup_file(rel_path).await else {
        return HookResponse::allow_no_context("PreToolUse");
    };

    // Track the read
    let read_count = session.track_read(&request.session_id, file_path).await;

    // Build summary context
    let summary = format_summary(rel_path, &file_summary, index).await;

    if read_count == 1 {
        // First read: deny with summary, let Claude decide if it needs the full file
        let reason = format!(
            "{summary}\n\
             If this summary is sufficient, do not read the file. \
             If you need the full file contents, read it again."
        );
        HookResponse::deny_with_reason("PreToolUse", reason)
    } else {
        // Second+ read: Claude insisted, allow it through with summary as context
        HookResponse::allow_with_context("PreToolUse", summary)
    }
}

/// Handle a SessionStart hook.
pub async fn handle_session_start(index: &Index) -> HookResponse {
    let map = index.project_map().await;

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
    if !index.is_ready() {
        context.push_str("\n(indexing in progress — some summaries may be stale)");
    }
    HookResponse::session_start_context(context)
}

/// Format a file summary string with description, symbols, and folder context.
async fn format_summary(
    rel_path: &Path,
    file_summary: &crate::types::FileSummary,
    index: &Index,
) -> String {
    let path_display = rel_path.display();
    let mut lines = Vec::with_capacity(4);

    lines.push(format!("-- youwhatknow: {path_display} --"));
    lines.push(file_summary.description.clone());

    if !file_summary.symbols.is_empty() {
        lines.push(format!("Public: {}", file_summary.symbols.join(", ")));
    }

    if let Some(folder) = index.lookup_folder(rel_path).await {
        let folder_path = rel_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        let folder_key = storage::folder_to_key(folder_path);
        let display = storage::key_to_folder(&folder_key);
        let display = if display.is_empty() { "." } else { &display };
        lines.push(format!("Folder: {display}/ — {}", folder.description));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::Index;
    use crate::session::SessionTracker;
    use crate::types::{FileSummary, FolderSummary, ToolInput};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    async fn setup_index(_project_root: &Path) -> Index {
        let index = Index::new();
        let rel = PathBuf::from("src/main.rs");

        index
            .insert_file(FileSummary {
                path: rel,
                description: "Entry point for the application".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count: 0,
                line_ranges: vec![],
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

    #[tokio::test]
    async fn pre_read_first_read_denies_with_summary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(root).await;
        let session = SessionTracker::new();

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

        let resp = handle_pre_read(&index, &session, root, &request).await;

        // First read should be denied
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("deny")
        );

        // Reason should contain the summary
        let reason = resp
            .hook_specific_output
            .permission_decision_reason
            .expect("should have reason");
        assert!(reason.contains("-- youwhatknow: src/main.rs --"));
        assert!(reason.contains("Entry point for the application"));
        assert!(reason.contains("Public: main()"));
        assert!(reason.contains("Folder: src/ — Core logic"));
        assert!(reason.contains("read it again"));
    }

    #[tokio::test]
    async fn pre_read_second_read_allows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(root).await;
        let session = SessionTracker::new();

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
        let resp = handle_pre_read(&index, &session, root, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("deny")
        );

        // Second read — allowed with summary context
        let resp = handle_pre_read(&index, &session, root, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(ctx.contains("Entry point for the application"));
    }

    #[tokio::test]
    async fn pre_read_outside_project() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(root).await;
        let session = SessionTracker::new();

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

        let resp = handle_pre_read(&index, &session, root, &request).await;
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn pre_read_no_summary_allows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = Index::new();
        let session = SessionTracker::new();

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

        let resp = handle_pre_read(&index, &session, root, &request).await;
        assert_eq!(
            resp.hook_specific_output.permission_decision.as_deref(),
            Some("allow")
        );
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn session_start_with_index() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = setup_index(root).await;

        let resp = handle_session_start(&index).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(ctx.contains("-- youwhatknow: project map --"));
        assert!(ctx.contains("main.rs"));
    }

    #[tokio::test]
    async fn session_start_indexing_in_progress() {
        let index = Index::new();
        let resp = handle_session_start(&index).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(ctx.contains("indexing in progress"));
    }
}
