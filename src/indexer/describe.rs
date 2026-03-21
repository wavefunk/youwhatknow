use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::stream::{self, StreamExt};

/// Generate descriptions for files by batching them through `claude` CLI with Haiku.
/// Returns a map from file path to one-line description.
pub async fn generate_descriptions(
    project_root: &Path,
    files: &[(PathBuf, Vec<String>)],
    concurrency: usize,
) -> HashMap<PathBuf, String> {
    let concurrency = concurrency.max(1);

    if !claude_available().await {
        tracing::warn!("claude CLI not found; using fallback descriptions");
        let mut descriptions = HashMap::new();
        for (path, symbols) in files {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            descriptions.insert(path.clone(), fallback_description(filename, symbols));
        }
        return descriptions;
    }

    let total_batches = files.chunks(15).len();
    tracing::info!(
        files = files.len(),
        batches = total_batches,
        concurrency,
        "generating file descriptions"
    );

    let batches: Vec<Vec<(PathBuf, Vec<String>)>> =
        files.chunks(15).map(|c| c.to_vec()).collect();

    let results: Vec<_> = stream::iter(
        batches.into_iter().enumerate().map(|(i, batch)| {
            let project_root = project_root.to_owned();
            async move {
                generate_batch(&project_root, &batch, i + 1, total_batches).await
            }
        })
    )
    .buffered(concurrency)
    .collect()
    .await;

    let mut descriptions = HashMap::new();
    for (result, batch) in results.into_iter().zip(files.chunks(15)) {
        match result {
            Ok(batch_descs) => {
                descriptions.extend(batch_descs);
            }
            Err(e) => {
                tracing::warn!(error = %e, "batch failed; using fallbacks");
                for (path, symbols) in batch {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    descriptions
                        .entry(path.clone())
                        .or_insert_with(|| fallback_description(filename, symbols));
                }
            }
        }
    }

    tracing::info!(
        described = descriptions.len(),
        files = files.len(),
        "file descriptions complete"
    );

    descriptions
}

/// Generate descriptions for a single batch via claude CLI.
async fn generate_batch(
    project_root: &Path,
    batch: &[(PathBuf, Vec<String>)],
    batch_num: usize,
    total_batches: usize,
) -> eyre::Result<HashMap<PathBuf, String>> {
    tracing::info!(
        batch = batch_num,
        total = total_batches,
        files = batch.len(),
        "starting batch"
    );

    let mut prompt = String::from(
        "For each file below, write exactly one short description (max 12 words). \
         Output format: one line per file as `PATH: description`. Nothing else.\n\n",
    );

    for (path, symbols) in batch {
        tracing::trace!(batch = batch_num, file = %path.display(), "describing file");
        let abs_path = project_root.join(path);
        let preview = read_preview(&abs_path, 100);
        prompt.push_str(&format!("FILE: {}\n", path.display()));
        if !symbols.is_empty() {
            prompt.push_str(&format!("SYMBOLS: {}\n", symbols.join(", ")));
        }
        if !preview.is_empty() {
            prompt.push_str(&format!("PREVIEW:\n{preview}\n"));
        }
        prompt.push_str("---\n");
    }

    let mut child = tokio::process::Command::new("claude")
        .args([
            "--dangerously-skip-permissions",
            "--model",
            "haiku",
            "--print",
        ])
        .current_dir(project_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        tokio::io::AsyncWriteExt::write_all(&mut stdin, prompt.as_bytes()).await?;
    }

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eyre::bail!("claude CLI failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut descriptions = HashMap::new();

    for line in stdout.lines() {
        if let Some((path_str, desc)) = line.split_once(':') {
            let path_str = path_str.trim();
            let desc = desc.trim();
            if !path_str.is_empty() && !desc.is_empty() {
                descriptions.insert(PathBuf::from(path_str), desc.to_owned());
            }
        }
    }

    // Fill in fallbacks for any files that weren't in the response
    for (path, symbols) in batch {
        if !descriptions.contains_key(path) {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            descriptions.insert(path.clone(), fallback_description(filename, symbols));
        }
    }

    tracing::info!(
        batch = batch_num,
        total = total_batches,
        described = descriptions.len(),
        "completed batch"
    );

    Ok(descriptions)
}

/// Read the first N lines of a file as a preview string.
fn read_preview(path: &Path, max_lines: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate a fallback description from the filename and symbols.
pub fn fallback_description(filename: &str, symbols: &[String]) -> String {
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);

    if symbols.is_empty() {
        // Convert filename stem to a readable form
        let readable = stem.replace(['_', '-'], " ");
        return capitalize_first(&readable);
    }

    // Take first few symbols to form a description
    let symbol_preview: Vec<&str> = symbols.iter().take(3).map(|s| s.as_str()).collect();
    let sym_str = symbol_preview.join(", ");

    let readable = stem.replace(['_', '-'], " ");
    format!("{} — {sym_str}", capitalize_first(&readable))
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            format!("{upper}{}", chars.as_str())
        }
    }
}

/// Check if `claude` CLI is available on PATH.
async fn claude_available() -> bool {
    tokio::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_with_symbols() {
        let desc = fallback_description("session.rs", &["SessionTracker".into(), "track_read".into()]);
        assert_eq!(desc, "Session — SessionTracker, track_read");
    }

    #[test]
    fn fallback_without_symbols() {
        let desc = fallback_description("main.rs", &[]);
        assert_eq!(desc, "Main");
    }

    #[test]
    fn fallback_with_underscores() {
        let desc = fallback_description("my_module.rs", &[]);
        assert_eq!(desc, "My module");
    }

    #[test]
    fn fallback_with_many_symbols_truncates() {
        let symbols: Vec<String> = (0..10).map(|i| format!("Sym{i}")).collect();
        let desc = fallback_description("lib.rs", &symbols);
        assert_eq!(desc, "Lib — Sym0, Sym1, Sym2");
    }

    #[test]
    fn capitalize_first_works() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("H"), "H");
    }
}
