use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Storage types (TOML on disk) ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSummary {
    pub path: PathBuf,
    pub description: String,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub line_count: u32,
    #[serde(default)]
    pub line_ranges: Vec<LineRange>,
    pub summarized: DateTime<Utc>,
}

/// Result of analyzing a file with tree-sitter. Internal to the indexer.
#[allow(dead_code)]
pub struct FileAnalysis {
    pub symbols: Vec<String>,
    pub line_ranges: Vec<LineRange>,
    pub line_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FolderSummary {
    pub generated: DateTime<Utc>,
    pub description: String,
    #[serde(default)]
    pub files: HashMap<String, FileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FolderEntry {
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSummary {
    pub generated: DateTime<Utc>,
    pub last_commit: String,
    #[serde(default)]
    pub folders: HashMap<String, FolderEntry>,
}

// ── Hook request/response types (JSON over HTTP) ──

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HookRequest {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    pub cwd: PathBuf,
    #[serde(alias = "hookEventName")]
    pub hook_event_name: String,
    #[serde(default, alias = "toolName")]
    pub tool_name: Option<String>,
    #[serde(default, alias = "toolInput")]
    pub tool_input: Option<ToolInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolInput {
    #[serde(alias = "filePath")]
    pub file_path: PathBuf,
    #[serde(default)]
    #[allow(dead_code)]
    pub offset: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponse {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "permissionDecision", skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(
        rename = "permissionDecisionReason",
        skip_serializing_if = "Option::is_none"
    )]
    pub permission_decision_reason: Option<String>,
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

impl HookResponse {
    pub fn allow_with_context(event_name: &str, context: String) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: event_name.to_owned(),
                permission_decision: Some("allow".to_owned()),
                permission_decision_reason: None,
                additional_context: Some(context),
            },
        }
    }

    pub fn allow_no_context(event_name: &str) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: event_name.to_owned(),
                permission_decision: Some("allow".to_owned()),
                permission_decision_reason: None,
                additional_context: None,
            },
        }
    }

    pub fn deny_with_reason(event_name: &str, reason: String) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: event_name.to_owned(),
                permission_decision: Some("deny".to_owned()),
                permission_decision_reason: Some(reason),
                additional_context: None,
            },
        }
    }

    pub fn session_start_context(context: String) -> Self {
        Self {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "SessionStart".to_owned(),
                permission_decision: None,
                permission_decision_reason: None,
                additional_context: Some(context),
            },
        }
    }
}

// ── Health check ──

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub projects: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_summary_toml_roundtrip() {
        let mut files = HashMap::new();
        files.insert(
            "main".to_owned(),
            FileSummary {
                path: PathBuf::from("src/main.rs"),
                description: "Entry point".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count: 0,
                line_ranges: vec![],
                summarized: Utc::now(),
            },
        );

        let summary = FolderSummary {
            generated: Utc::now(),
            description: "Core logic".to_owned(),
            files,
        };

        let toml_str = toml::to_string_pretty(&summary).expect("serialize");
        let parsed: FolderSummary = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(summary, parsed);
    }

    #[test]
    fn hook_request_deserializes_pre_tool_use() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "hook_event_name": "PreToolUse",
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/home/user/project/src/main.rs"
            }
        }"#;

        let req: HookRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.session_id, "abc123");
        assert_eq!(req.hook_event_name, "PreToolUse");
        assert_eq!(req.tool_name.as_deref(), Some("Read"));
        assert_eq!(
            req.tool_input.as_ref().map(|t| &t.file_path),
            Some(&PathBuf::from("/home/user/project/src/main.rs"))
        );
    }

    #[test]
    fn hook_request_deserializes_session_start() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "hook_event_name": "SessionStart"
        }"#;

        let req: HookRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.hook_event_name, "SessionStart");
        assert!(req.tool_name.is_none());
        assert!(req.tool_input.is_none());
    }

    #[test]
    fn hook_request_deserializes_camel_case() {
        let json = r#"{
            "sessionId": "abc123",
            "cwd": "/home/user/project",
            "hookEventName": "PreToolUse",
            "toolName": "Read",
            "toolInput": {
                "file_path": "/home/user/project/src/main.rs"
            }
        }"#;

        let req: HookRequest = serde_json::from_str(json).expect("deserialize camelCase");
        assert_eq!(req.session_id, "abc123");
        assert_eq!(req.hook_event_name, "PreToolUse");
        assert_eq!(req.tool_name.as_deref(), Some("Read"));
        assert_eq!(
            req.tool_input.as_ref().map(|t| &t.file_path),
            Some(&PathBuf::from("/home/user/project/src/main.rs"))
        );
    }

    #[test]
    fn hook_request_deserializes_camel_case_session_start() {
        let json = r#"{
            "sessionId": "sess-456",
            "cwd": "/home/user/project",
            "hookEventName": "SessionStart"
        }"#;

        let req: HookRequest = serde_json::from_str(json).expect("deserialize camelCase");
        assert_eq!(req.session_id, "sess-456");
        assert_eq!(req.hook_event_name, "SessionStart");
        assert!(req.tool_name.is_none());
        assert!(req.tool_input.is_none());
    }

    #[test]
    fn hook_response_serializes_allow_with_context() {
        let resp = HookResponse::allow_with_context("PreToolUse", "summary text".to_owned());
        let json = serde_json::to_value(&resp).expect("serialize");

        assert_eq!(
            json["hookSpecificOutput"]["hookEventName"],
            "PreToolUse"
        );
        assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
        assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            "summary text"
        );
    }

    #[test]
    fn hook_response_serializes_allow_no_context() {
        let resp = HookResponse::allow_no_context("PreToolUse");
        let json = serde_json::to_value(&resp).expect("serialize");

        assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
        assert!(json["hookSpecificOutput"].get("additionalContext").is_none());
    }

    #[test]
    fn line_range_display() {
        let range = LineRange {
            start: 1,
            end: 67,
            label: "ActivityTracker — idle timeout, touch/check".to_owned(),
        };
        assert_eq!(range.start, 1);
        assert_eq!(range.end, 67);
    }

    #[test]
    fn file_summary_with_line_ranges_toml_roundtrip() {
        let mut files = HashMap::new();
        files.insert(
            "main".to_owned(),
            FileSummary {
                path: PathBuf::from("src/main.rs"),
                description: "Entry point".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count: 52,
                line_ranges: vec![
                    LineRange {
                        start: 1,
                        end: 10,
                        label: "Imports".to_owned(),
                    },
                    LineRange {
                        start: 12,
                        end: 52,
                        label: "main() function".to_owned(),
                    },
                ],
                summarized: Utc::now(),
            },
        );

        let summary = FolderSummary {
            generated: Utc::now(),
            description: "Core logic".to_owned(),
            files,
        };

        let toml_str = toml::to_string_pretty(&summary).expect("serialize");
        let parsed: FolderSummary = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(summary, parsed);
    }

    #[test]
    fn file_summary_without_line_ranges_deserializes() {
        let toml_str = r#"
path = "src/main.rs"
description = "Entry point"
symbols = ["main()"]
summarized = "2026-03-22T00:00:00Z"
"#;
        let parsed: FileSummary = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(parsed.line_count, 0);
        assert!(parsed.line_ranges.is_empty());
    }

    #[test]
    fn tool_input_deserializes_with_offset_limit() {
        let json = r#"{
            "file_path": "/home/user/project/src/main.rs",
            "offset": 10,
            "limit": 50
        }"#;
        let input: ToolInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(input.offset, Some(10));
        assert_eq!(input.limit, Some(50));
    }

    #[test]
    fn tool_input_deserializes_without_offset_limit() {
        let json = r#"{"file_path": "/home/user/project/src/main.rs"}"#;
        let input: ToolInput = serde_json::from_str(json).expect("deserialize");
        assert!(input.offset.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn project_summary_toml_roundtrip() {
        let mut folders = HashMap::new();
        folders.insert(
            "src".to_owned(),
            FolderEntry {
                path: "src/".to_owned(),
                description: "Core logic".to_owned(),
            },
        );

        let summary = ProjectSummary {
            generated: Utc::now(),
            last_commit: "abc123".to_owned(),
            folders,
        };

        let toml_str = toml::to_string_pretty(&summary).expect("serialize");
        let parsed: ProjectSummary = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(summary, parsed);
    }
}
