use std::fmt::Write;

use crate::config::ProjectConfig;
use crate::indexer::Index;
use crate::types::FileSummary;

/// Render a file summary with description, symbols, and line-range map.
pub fn render_file_summary(file_summary: &FileSummary, _config: &ProjectConfig) -> String {
    let path = file_summary.path.display();
    let mut out = String::with_capacity(256);

    let _ = writeln!(
        out,
        "{path} ({} lines) — {}",
        file_summary.line_count, file_summary.description
    );

    if !file_summary.symbols.is_empty() {
        let _ = writeln!(out, "Public: {}", file_summary.symbols.join(", "));
    }

    if !file_summary.line_ranges.is_empty() {
        let _ = writeln!(out);
        let max_range_width = file_summary
            .line_ranges
            .iter()
            .map(|r| {
                // digit count without allocating
                let s = digit_count(r.start) + 1 + digit_count(r.end);
                s
            })
            .max()
            .unwrap_or(0);
        for range in &file_summary.line_ranges {
            let _ = write!(out, "  {}-{}", range.start, range.end);
            let range_len = digit_count(range.start) + 1 + digit_count(range.end);
            // pad to align labels
            for _ in range_len..max_range_width {
                out.push(' ');
            }
            let _ = writeln!(out, "  {}", range.label);
        }
    }

    let _ = writeln!(out);
    let _ = write!(
        out,
        "Read specific sections with offset/limit, or read again for the full file."
    );

    out
}

/// Render the project map from the index.
pub async fn render_project_map(index: &Index) -> String {
    index.project_map().await
}

/// Render session instructions for the SessionStart context.
pub fn render_session_instructions(config: &ProjectConfig) -> String {
    format!(
        "Files over {} lines show a summary on first read. \
         Read again for the full file, or use offset/limit to target specific sections.\n\
         To preview any file without triggering a read: run `youwhatknow summary <path>` in the terminal.",
        config.line_threshold
    )
}

/// Count the number of digits in a u32 (avoids allocation vs format!).
fn digit_count(n: u32) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut v = n;
    while v > 0 {
        count += 1;
        v /= 10;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FolderSummary, LineRange};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn sample_file_summary() -> FileSummary {
        FileSummary {
            path: PathBuf::from("src/server.rs"),
            description: "Axum HTTP server, routing, activity tracking".to_owned(),
            symbols: vec![
                "ActivityTracker".to_owned(),
                "AppState".to_owned(),
                "router()".to_owned(),
            ],
            line_count: 227,
            line_ranges: vec![
                LineRange {
                    start: 1,
                    end: 67,
                    label: "ActivityTracker — idle timeout, touch/check".to_owned(),
                },
                LineRange {
                    start: 69,
                    end: 76,
                    label: "AppState struct".to_owned(),
                },
                LineRange {
                    start: 78,
                    end: 85,
                    label: "Router setup — 4 routes".to_owned(),
                },
                LineRange {
                    start: 87,
                    end: 125,
                    label: "Handlers: pre_read, session_start, reindex, health".to_owned(),
                },
                LineRange {
                    start: 126,
                    end: 227,
                    label: "Tests".to_owned(),
                },
            ],
            summarized: Utc::now(),
        }
    }

    #[test]
    fn render_file_summary_includes_all_parts() {
        let summary = sample_file_summary();
        let config = ProjectConfig::default();
        let rendered = render_file_summary(&summary, &config);

        assert!(rendered.contains("src/server.rs (227 lines)"));
        assert!(rendered.contains("Axum HTTP server"));
        assert!(rendered.contains("Public: ActivityTracker, AppState, router()"));
        assert!(rendered.contains("1-67"));
        assert!(rendered.contains("ActivityTracker"));
        assert!(rendered.contains("126-227"));
        assert!(rendered.contains("Tests"));
        assert!(rendered.contains("offset/limit"));
    }

    #[test]
    fn render_file_summary_no_line_ranges() {
        let summary = FileSummary {
            path: PathBuf::from("data.csv"),
            description: "Data file".to_owned(),
            symbols: vec![],
            line_count: 100,
            line_ranges: vec![],
            summarized: Utc::now(),
        };
        let config = ProjectConfig::default();
        let rendered = render_file_summary(&summary, &config);

        assert!(rendered.contains("data.csv (100 lines)"));
        assert!(rendered.contains("Data file"));
        assert!(!rendered.contains("Public:"));
        assert!(rendered.contains("offset/limit"));
    }

    #[tokio::test]
    async fn render_project_map_format() {
        let index = Index::new();
        index
            .insert_file(FileSummary {
                path: PathBuf::from("src/main.rs"),
                description: "Entry point".to_owned(),
                symbols: vec!["main()".to_owned()],
                line_count: 52,
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

        let map = render_project_map(&index).await;
        assert!(map.contains("src/ — Core logic"));
        assert!(map.contains("  main.rs — Entry point"));
    }

    #[test]
    fn render_session_instructions_includes_threshold() {
        let config = ProjectConfig::default();
        let instructions = render_session_instructions(&config);
        assert!(instructions.contains("30 lines"));
        assert!(instructions.contains("youwhatknow summary"));
        assert!(instructions.contains("offset/limit"));
    }

    #[test]
    fn render_session_instructions_custom_threshold() {
        let config = ProjectConfig {
            line_threshold: 50,
            ..ProjectConfig::default()
        };
        let instructions = render_session_instructions(&config);
        assert!(instructions.contains("50 lines"));
    }
}
