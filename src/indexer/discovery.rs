use std::path::{Path, PathBuf};
use std::process::Command;

use eyre::{Context, bail};

use crate::config::ProjectConfig;

/// Discover all indexable files in the project using `git ls-files`.
pub fn discover_files(project_root: &Path, config: &ProjectConfig) -> eyre::Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(project_root)
        .output()
        .context("running git ls-files")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git ls-files failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let max_size = config.max_file_size_kb * 1024;
    let ignore_patterns = config.all_ignore_patterns();

    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|rel_path| {
            let abs_path = project_root.join(rel_path);
            should_index(&abs_path, rel_path, max_size, &ignore_patterns)
        })
        .collect();

    Ok(files)
}

/// Discover files that changed since a given commit hash.
pub fn discover_changed_files(
    project_root: &Path,
    last_commit: &str,
) -> eyre::Result<Vec<PathBuf>> {
    let mut changed = Vec::new();

    // Committed changes since last run
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{last_commit}..HEAD")])
        .current_dir(project_root)
        .output()
        .context("running git diff for committed changes")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            changed.push(PathBuf::from(line));
        }
    }

    // Unstaged changes
    let output = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(project_root)
        .output()
        .context("running git diff for unstaged changes")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if !changed.contains(&path) {
                changed.push(path);
            }
        }
    }

    // Untracked files
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(project_root)
        .output()
        .context("running git ls-files for untracked")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if !changed.contains(&path) {
                changed.push(path);
            }
        }
    }

    Ok(changed)
}

/// Get the current HEAD commit hash.
pub fn current_commit(project_root: &Path) -> eyre::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(project_root)
        .output()
        .context("running git rev-parse")?;

    if !output.status.success() {
        bail!("git rev-parse failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Check whether a file should be indexed.
fn should_index(
    abs_path: &Path,
    rel_path: &Path,
    max_size: u64,
    ignore_patterns: &[&str],
) -> bool {
    // Must be a file
    if !abs_path.is_file() {
        return false;
    }

    // Check file size
    if abs_path
        .metadata()
        .is_ok_and(|meta| meta.len() > max_size)
    {
        return false;
    }

    // Check ignore patterns against the filename
    let filename = rel_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let rel_str = rel_path.to_str().unwrap_or_default();

    for pattern in ignore_patterns {
        if glob_match::glob_match(pattern, filename)
            || glob_match::glob_match(pattern, rel_str)
        {
            return false;
        }
    }

    // Check for binary content (null bytes in first 512 bytes)
    if is_binary(abs_path) {
        return false;
    }

    true
}

/// Detect binary files by checking for null bytes in the first 512 bytes.
fn is_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 512];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

/// Resolve the main worktree root for a given path.
///
/// For regular repos, returns the repo root (same as `--show-toplevel`).
/// For linked worktrees, returns the main worktree's root.
/// This lets worktrees share an already-loaded project index.
pub fn resolve_main_worktree(cwd: &Path) -> eyre::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(cwd)
        .output()
        .context("running git rev-parse --git-common-dir")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse --git-common-dir failed: {stderr}");
    }

    let git_common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    // git-common-dir points to the .git directory of the main worktree.
    // Its parent is the main worktree root.
    let main_root = git_common
        .parent()
        .ok_or_else(|| eyre::eyre!("git-common-dir has no parent: {}", git_common.display()))?;

    // Canonicalize to resolve symlinks and get a stable key
    main_root
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", main_root.display()))
}

/// Get the parent folder of a relative path as a string.
/// e.g., "src/main.rs" -> "src", "main.rs" -> ""
pub fn file_folder(rel_path: &Path) -> String {
    rel_path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_owned()
}

/// Get a key name for a file (full filename with extension).
/// e.g., "src/main.rs" -> "main.rs", "src/lib.rs" -> "lib.rs"
pub fn file_key(rel_path: &Path) -> String {
    rel_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_folder() {
        assert_eq!(file_folder(Path::new("src/main.rs")), "src");
        assert_eq!(file_folder(Path::new("src/indexer/mod.rs")), "src/indexer");
        assert_eq!(file_folder(Path::new("main.rs")), "");
    }

    #[test]
    fn test_file_key() {
        assert_eq!(file_key(Path::new("src/main.rs")), "main.rs");
        assert_eq!(file_key(Path::new("lib.rs")), "lib.rs");
        assert_eq!(file_key(Path::new("src/indexer/mod.rs")), "mod.rs");
    }

    #[test]
    fn test_is_binary() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // Text file
        let text_path = tmp.path().join("text.rs");
        std::fs::write(&text_path, "fn main() {}").expect("write");
        assert!(!is_binary(&text_path));

        // Binary file
        let bin_path = tmp.path().join("binary.bin");
        std::fs::write(&bin_path, b"hello\x00world").expect("write");
        assert!(is_binary(&bin_path));
    }

    #[test]
    fn test_should_index_respects_patterns() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("package-lock.json");
        std::fs::write(&file, "{}").expect("write");

        let patterns = vec!["package-lock.json"];
        assert!(!should_index(
            &file,
            Path::new("package-lock.json"),
            102400,
            &patterns
        ));
    }

    #[test]
    fn test_should_index_respects_size_limit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("big.rs");
        // Write 200KB
        let content = "x".repeat(200 * 1024);
        std::fs::write(&file, content).expect("write");

        let patterns: Vec<&str> = vec![];
        // 100KB limit
        assert!(!should_index(&file, Path::new("big.rs"), 102400, &patterns));
    }

    #[test]
    #[ignore = "requires git, skipped in sandboxed builds"]
    fn resolve_main_worktree_in_regular_repo() {
        // In a non-worktree repo, resolve_main_worktree should return
        // the same path as git rev-parse --show-toplevel
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let resolved = resolve_main_worktree(root).expect("resolve");
        let expected = root.canonicalize().expect("canonicalize");
        assert_eq!(resolved, expected);
    }

    #[test]
    #[ignore = "requires git repo, skipped in sandboxed builds"]
    fn test_discover_files_in_real_repo() {
        // Test against this repo
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = ProjectConfig::default();
        let files = discover_files(root, &config).expect("discover");

        // Should find src/main.rs at minimum
        assert!(
            files.iter().any(|p| p.ends_with("main.rs")),
            "expected main.rs in {files:?}"
        );

        // Should NOT find Cargo.lock (in default ignore patterns)
        assert!(
            !files.iter().any(|p| p.ends_with("Cargo.lock")),
            "Cargo.lock should be filtered"
        );
    }
}
