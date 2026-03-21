use std::path::Path;

use crate::indexer::Index;
use crate::session::SessionTracker;
use crate::storage;
use crate::types::{HookRequest, HookResponse};

/// Handle a PreToolUse hook for the Read tool.
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

    // Check if file is inside the project
    let rel_path = match file_path.strip_prefix(project_root) {
        Ok(rel) => rel,
        Err(_) => return HookResponse::allow_no_context("PreToolUse"),
    };

    // Check if indexing is still in progress and file isn't indexed
    if !index.is_ready() && index.lookup_file(rel_path).await.is_none() {
        let context = "-- youwhatknow: indexing in progress --\n\
            File summaries are still being generated. This read will proceed without a summary."
            .to_owned();
        return HookResponse::allow_with_context("PreToolUse", context);
    }

    // Look up file summary
    let Some(file_summary) = index.lookup_file(rel_path).await else {
        return HookResponse::allow_no_context("PreToolUse");
    };

    // Track the read
    let read_count = session.track_read(&request.session_id, file_path).await;

    // Format the response
    let context = format_pre_read_context(rel_path, &file_summary, read_count, index).await;
    HookResponse::allow_with_context("PreToolUse", context)
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

/// Format the context string for a pre-read response.
async fn format_pre_read_context(
    rel_path: &Path,
    file_summary: &crate::types::FileSummary,
    read_count: u32,
    index: &Index,
) -> String {
    let path_display = rel_path.display();
    let mut lines = Vec::new();

    // Header with optional read count
    if read_count > 1 {
        lines.push(format!(
            "-- youwhatknow: {path_display} (read {read_count}x this session) --"
        ));
        lines.push(
            "This file was already read. Summary below — do you still need the full file?"
                .to_owned(),
        );
    } else {
        lines.push(format!("-- youwhatknow: {path_display} --"));
    }

    // Description
    lines.push(file_summary.description.clone());

    // Symbols
    if !file_summary.symbols.is_empty() {
        lines.push(format!("Public: {}", file_summary.symbols.join(", ")));
    }

    // Folder context
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
    async fn pre_read_first_read() {
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
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &request).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");

        assert!(ctx.contains("-- youwhatknow: src/main.rs --"));
        assert!(ctx.contains("Entry point for the application"));
        assert!(ctx.contains("Public: main()"));
        assert!(ctx.contains("Folder: src/ — Core logic"));
        assert!(!ctx.contains("already read"));
    }

    #[tokio::test]
    async fn pre_read_re_read_warns() {
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
            }),
        };

        // First read
        handle_pre_read(&index, &session, root, &request).await;
        // Second read
        let resp = handle_pre_read(&index, &session, root, &request).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");

        assert!(ctx.contains("read 2x this session"));
        assert!(ctx.contains("already read"));
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
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &request).await;
        assert!(resp.hook_specific_output.additional_context.is_none());
    }

    #[tokio::test]
    async fn pre_read_indexing_in_progress() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let index = Index::new();
        // NOT marking as ready
        let session = SessionTracker::new();

        let request = HookRequest {
            session_id: "s1".to_owned(),
            cwd: root.to_path_buf(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: Some("Read".to_owned()),
            tool_input: Some(ToolInput {
                file_path: root.join("src/main.rs"),
            }),
        };

        let resp = handle_pre_read(&index, &session, root, &request).await;
        let ctx = resp
            .hook_specific_output
            .additional_context
            .expect("should have context");
        assert!(ctx.contains("indexing in progress"));
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
