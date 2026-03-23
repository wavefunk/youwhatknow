use std::collections::HashMap;
use std::path::Path;

use eyre::Context;

use crate::types::{FolderSummary, ProjectSummary};

/// Load a single folder summary from a TOML file.
pub fn load_folder_summary(path: &Path) -> eyre::Result<FolderSummary> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let summary: FolderSummary = toml::from_str(&content)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(summary)
}

/// Save a folder summary to a TOML file using atomic write (temp + rename).
pub fn save_folder_summary(path: &Path, summary: &FolderSummary) -> eyre::Result<()> {
    let content = toml::to_string_pretty(summary)
        .context("serializing folder summary")?;
    atomic_write(path, content.as_bytes())
}

/// Load the project summary from a TOML file.
pub fn load_project_summary(path: &Path) -> eyre::Result<ProjectSummary> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let summary: ProjectSummary = toml::from_str(&content)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(summary)
}

/// Save the project summary to a TOML file using atomic write.
pub fn save_project_summary(path: &Path, summary: &ProjectSummary) -> eyre::Result<()> {
    let content = toml::to_string_pretty(summary)
        .context("serializing project summary")?;
    atomic_write(path, content.as_bytes())
}

/// Load all folder summaries from a directory.
/// Returns a map from folder key (filename stem) to FolderSummary.
pub fn load_all_summaries(
    summary_dir: &Path,
) -> eyre::Result<HashMap<String, FolderSummary>> {
    let mut summaries = HashMap::new();

    if !summary_dir.exists() {
        return Ok(summaries);
    }

    let entries = std::fs::read_dir(summary_dir)
        .with_context(|| format!("reading directory {}", summary_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "toml") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned();

            // Skip project-summary.toml — it's loaded separately
            if stem == "project-summary" {
                continue;
            }

            match load_folder_summary(&path) {
                Ok(summary) => {
                    summaries.insert(stem, summary);
                }
                Err(e) => {
                    tracing::warn!("skipping {}: {e}", path.display());
                }
            }
        }
    }

    Ok(summaries)
}

/// Write data to a file atomically: write to a temp file in the same directory,
/// then rename to the target path.
fn atomic_write(path: &Path, data: &[u8]) -> eyre::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating directory {}", parent.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .context("creating temp file")?;
    std::io::Write::write_all(&mut tmp, data).context("writing temp file")?;

    // persist_noclobber would fail if target exists, so use persist which overwrites
    tmp.persist(path)
        .map_err(|e| {
            let msg = format!("renaming temp file to {}: {e}", path.display());
            eyre::eyre!(msg)
        })?;

    Ok(())
}

/// Convert a folder path to a TOML filename key.
/// e.g., "src" -> "src", "src/server" -> "src--server", "" or "." -> "root"
pub fn folder_to_key(folder: &str) -> String {
    let folder = folder.trim_matches('/');
    if folder.is_empty() || folder == "." {
        "root".to_owned()
    } else {
        folder.replace('/', "--")
    }
}

/// Convert a TOML key back to a folder path.
/// e.g., "src" -> "src", "src--server" -> "src/server", "root" -> ""
pub fn key_to_folder(key: &str) -> String {
    if key == "root" {
        String::new()
    } else {
        key.replace("--", "/")
    }
}

/// Read the last-run commit hash from `.last-run` file.
pub fn read_last_run(summary_dir: &Path) -> Option<String> {
    let path = summary_dir.join(".last-run");
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}

/// Write the last-run commit hash.
pub fn write_last_run(summary_dir: &Path, commit: &str) -> eyre::Result<()> {
    let path = summary_dir.join(".last-run");
    std::fs::create_dir_all(summary_dir)?;
    std::fs::write(path, commit).context("writing .last-run")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileSummary;
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_folder_summary() -> FolderSummary {
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
        FolderSummary {
            generated: Utc::now(),
            description: "Source code".to_owned(),
            files,
        }
    }

    #[test]
    fn roundtrip_folder_summary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("src.toml");
        let summary = make_folder_summary();

        save_folder_summary(&path, &summary).expect("save");
        let loaded = load_folder_summary(&path).expect("load");
        assert_eq!(summary, loaded);
    }

    #[test]
    fn roundtrip_project_summary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("project-summary.toml");

        let mut folders = HashMap::new();
        folders.insert(
            "src".to_owned(),
            crate::types::FolderEntry {
                path: "src/".to_owned(),
                description: "Core logic".to_owned(),
            },
        );
        let summary = ProjectSummary {
            generated: Utc::now(),
            last_commit: "abc123".to_owned(),
            folders,
        };

        save_project_summary(&path, &summary).expect("save");
        let loaded = load_project_summary(&path).expect("load");
        assert_eq!(summary, loaded);
    }

    #[test]
    fn load_all_summaries_from_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();

        let s1 = make_folder_summary();
        save_folder_summary(&dir.join("src.toml"), &s1).expect("save");

        let mut s2 = make_folder_summary();
        s2.description = "Tests".to_owned();
        save_folder_summary(&dir.join("tests.toml"), &s2).expect("save");

        // This should be skipped
        save_project_summary(
            &dir.join("project-summary.toml"),
            &ProjectSummary {
                generated: Utc::now(),
                last_commit: "x".to_owned(),
                folders: HashMap::new(),
            },
        )
        .expect("save");

        let loaded = load_all_summaries(dir).expect("load");
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains_key("src"));
        assert!(loaded.contains_key("tests"));
    }

    #[test]
    fn load_all_from_nonexistent_dir() {
        let loaded =
            load_all_summaries(Path::new("/nonexistent/path")).expect("load");
        assert!(loaded.is_empty());
    }

    #[test]
    fn folder_key_conversions() {
        assert_eq!(folder_to_key("src"), "src");
        assert_eq!(folder_to_key("src/server"), "src--server");
        assert_eq!(folder_to_key(""), "root");
        assert_eq!(folder_to_key("."), "root");
        assert_eq!(folder_to_key("src/indexer/"), "src--indexer");

        assert_eq!(key_to_folder("src"), "src");
        assert_eq!(key_to_folder("src--server"), "src/server");
        assert_eq!(key_to_folder("root"), "");
    }

    #[test]
    fn last_run_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(read_last_run(tmp.path()).is_none());

        write_last_run(tmp.path(), "abc123").expect("write");
        assert_eq!(read_last_run(tmp.path()), Some("abc123".to_owned()));
    }
}
