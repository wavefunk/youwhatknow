pub mod describe;
pub mod discovery;
pub mod symbols;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::config::ProjectConfig;
use crate::storage;
use crate::types::{FileSummary, FolderEntry, FolderSummary, ProjectSummary};

/// The in-memory index of all file summaries.
#[derive(Clone)]
pub struct Index {
    inner: Arc<IndexInner>,
}

struct IndexInner {
    /// file path (relative) -> FileSummary
    files: RwLock<HashMap<PathBuf, FileSummary>>,
    /// folder key -> FolderSummary
    folders: RwLock<HashMap<String, FolderSummary>>,
    /// project summary
    project: RwLock<Option<ProjectSummary>>,
    /// whether initial indexing is complete
    ready: AtomicBool,
    /// number of files indexed so far
    indexed_count: AtomicUsize,
    /// total files to index (set at start of indexing)
    total_count: AtomicUsize,
}

impl Index {
    /// Create a new empty index.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(IndexInner {
                files: RwLock::new(HashMap::new()),
                folders: RwLock::new(HashMap::new()),
                project: RwLock::new(None),
                ready: AtomicBool::new(false),
                indexed_count: AtomicUsize::new(0),
                total_count: AtomicUsize::new(0),
            }),
        }
    }

    /// Load existing summaries from disk into memory.
    pub async fn load_from_disk(&self, project_root: &Path, config: &ProjectConfig) {
        let summary_dir = project_root.join(&config.summary_path);

        match storage::load_all_summaries(&summary_dir) {
            Ok(folder_summaries) => {
                let mut files = self.inner.files.write().await;
                let mut folders = self.inner.folders.write().await;

                for (key, folder) in &folder_summaries {
                    for file_summary in folder.files.values() {
                        files.insert(file_summary.path.clone(), file_summary.clone());
                    }
                    folders.insert(key.clone(), folder.clone());
                }

                let file_count = files.len();
                tracing::info!(
                    folders = folder_summaries.len(),
                    files = file_count,
                    "loaded summaries from disk"
                );
            }
            Err(e) => {
                tracing::warn!("failed to load summaries: {e}");
            }
        }

        let project_path = summary_dir.join("project-summary.toml");
        if let Ok(project) = storage::load_project_summary(&project_path) {
            *self.inner.project.write().await = Some(project);
        }
    }

    /// Look up a file summary by relative path.
    pub async fn lookup_file(&self, rel_path: &Path) -> Option<FileSummary> {
        self.inner.files.read().await.get(rel_path).cloned()
    }

    /// Look up the folder summary for a given relative path.
    pub async fn lookup_folder(&self, rel_path: &Path) -> Option<FolderSummary> {
        let folder = discovery::file_folder(rel_path);
        let key = storage::folder_to_key(&folder);
        self.inner.folders.read().await.get(&key).cloned()
    }

    /// Generate the project map text for session start.
    pub async fn project_map(&self) -> String {
        let folders = self.inner.folders.read().await;
        let files = self.inner.files.read().await;

        let mut lines = Vec::new();

        // Collect and sort folder keys
        let mut keys: Vec<&String> = folders.keys().collect();
        keys.sort();

        for key in keys {
            let folder = &folders[key];
            let folder_path = storage::key_to_folder(key);
            let display_path = if folder_path.is_empty() {
                "./"
            } else {
                &folder_path
            };
            lines.push(format!("{display_path}/ — {}", folder.description));

            // List files in this folder
            let mut folder_files: Vec<(&PathBuf, &FileSummary)> = files
                .iter()
                .filter(|(path, _)| {
                    let file_folder = discovery::file_folder(path);
                    storage::folder_to_key(&file_folder) == *key
                })
                .collect();
            folder_files.sort_by_key(|(path, _)| *path);

            for (path, summary) in folder_files {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                lines.push(format!("  {filename} — {}", summary.description));
            }
        }

        lines.join("\n")
    }

    /// Insert a file summary directly (for testing and manual use).
    #[cfg(test)]
    pub async fn insert_file(&self, summary: FileSummary) {
        self.inner
            .files
            .write()
            .await
            .insert(summary.path.clone(), summary);
    }

    /// Insert a folder summary directly (for testing and manual use).
    #[cfg(test)]
    pub async fn insert_folder(&self, key: String, summary: FolderSummary) {
        self.inner.folders.write().await.insert(key, summary);
    }

    /// Mark the index as ready.
    pub fn set_ready(&self, ready: bool) {
        self.inner.ready.store(ready, Ordering::Relaxed);
    }

    pub fn is_ready(&self) -> bool {
        self.inner.ready.load(Ordering::Relaxed)
    }

    pub fn indexed_count(&self) -> usize {
        self.inner.indexed_count.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn total_count(&self) -> usize {
        self.inner.total_count.load(Ordering::Relaxed)
    }

    /// Run a full index of the project. Call from a background task.
    pub async fn full_index(&self, project_root: &Path, config: &ProjectConfig) {
        tracing::info!("starting full index");
        self.inner.indexed_count.store(0, Ordering::Relaxed);

        let files = match discovery::discover_files(project_root, config) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("file discovery failed: {e}");
                self.inner.ready.store(true, Ordering::Relaxed);
                return;
            }
        };

        self.inner
            .total_count
            .store(files.len(), Ordering::Relaxed);
        tracing::info!(count = files.len(), "discovered files");

        self.index_files(project_root, config, &files).await;

        self.inner.ready.store(true, Ordering::Relaxed);
        tracing::info!("full index complete");
    }

    /// Run an incremental index for changed files only.
    pub async fn incremental_index(&self, project_root: &Path, config: &ProjectConfig) {
        self.inner.indexed_count.store(0, Ordering::Relaxed);
        let summary_dir = project_root.join(&config.summary_path);
        let last_commit = storage::read_last_run(&summary_dir);

        let Some(last_commit) = last_commit else {
            tracing::info!("no .last-run found, running full index");
            self.full_index(project_root, config).await;
            return;
        };

        let changed = match discovery::discover_changed_files(project_root, &last_commit) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("change detection failed: {e}; running full index");
                self.full_index(project_root, config).await;
                return;
            }
        };

        if changed.is_empty() {
            tracing::info!("no files changed since last run");
            self.inner.ready.store(true, Ordering::Relaxed);
            return;
        }

        tracing::info!(count = changed.len(), "re-indexing changed files");
        self.inner
            .total_count
            .store(changed.len(), Ordering::Relaxed);

        // Filter changed files through the same rules
        let max_size = config.max_file_size_kb * 1024;
        let ignore_patterns = config.all_ignore_patterns();
        let indexable: Vec<PathBuf> = changed
            .into_iter()
            .filter(|rel_path| {
                let abs_path = project_root.join(rel_path);
                if !abs_path.is_file() {
                    return false;
                }
                if abs_path
                    .metadata()
                    .is_ok_and(|meta| meta.len() > max_size)
                {
                    return false;
                }
                let filename = rel_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                let rel_str = rel_path.to_str().unwrap_or_default();
                for pattern in &ignore_patterns {
                    if glob_match::glob_match(pattern, filename)
                        || glob_match::glob_match(pattern, rel_str)
                    {
                        return false;
                    }
                }
                true
            })
            .collect();

        self.index_files(project_root, config, &indexable).await;

        self.inner.ready.store(true, Ordering::Relaxed);
        tracing::info!("incremental index complete");
    }

    /// Index a set of files: extract symbols, generate descriptions, update in-memory + disk.
    async fn index_files(&self, project_root: &Path, config: &ProjectConfig, files: &[PathBuf]) {
        // Extract symbols
        let mut file_symbols: Vec<(PathBuf, Vec<String>)> = Vec::new();
        tracing::info!(files = files.len(), "extracting symbols");
        for rel_path in files {
            tracing::trace!(file = %rel_path.display(), "extracting symbols");
            let abs_path = project_root.join(rel_path);
            let source = match std::fs::read(&abs_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let syms = symbols::extract_symbols(rel_path, &source);
            file_symbols.push((rel_path.clone(), syms));
            self.inner.indexed_count.fetch_add(1, Ordering::Relaxed);
        }
        tracing::info!(files = file_symbols.len(), "symbol extraction complete");

        // Generate file descriptions
        let descriptions = describe::generate_descriptions(
            project_root,
            &file_symbols,
            config.max_concurrent_batches,
        ).await;

        // Build FileSummary entries and group by folder
        let now = Utc::now();
        let mut folder_files: HashMap<String, Vec<(String, FileSummary)>> = HashMap::new();

        for (rel_path, syms) in &file_symbols {
            let desc = descriptions
                .get(rel_path)
                .cloned()
                .unwrap_or_else(|| {
                    let filename = rel_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    describe::fallback_description(filename, syms)
                });

            let summary = FileSummary {
                path: rel_path.clone(),
                description: desc,
                symbols: syms.clone(),
                summarized: now,
            };

            let folder = discovery::file_folder(rel_path);
            let key = storage::folder_to_key(&folder);
            let file_key = discovery::file_key(rel_path);

            self.inner
                .files
                .write()
                .await
                .insert(rel_path.clone(), summary.clone());

            folder_files
                .entry(key)
                .or_default()
                .push((file_key, summary));
        }

        // Phase 1: merge file data into folders (write lock, then drop)
        {
            let mut folders = self.inner.folders.write().await;
            for (key, new_files) in &folder_files {
                let folder = folders.entry(key.clone()).or_insert_with(|| {
                    let folder_path = storage::key_to_folder(key);
                    FolderSummary {
                        generated: now,
                        description: folder_path,
                        files: HashMap::new(),
                    }
                });
                folder.generated = now;
                for (file_key, summary) in new_files {
                    folder.files.insert(file_key.clone(), summary.clone());
                }
            }
        } // write lock dropped here

        // Phase 2: collect folder inputs (read lock)
        let folder_inputs: Vec<(String, Vec<String>)> = {
            let folders = self.inner.folders.read().await;
            folders.iter().map(|(key, folder)| {
                let file_descs: Vec<String> = folder.files.values()
                    .map(|f| {
                        let name = f.path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default();
                        format!("{name}: {}", f.description)
                    })
                    .collect();
                (storage::key_to_folder(key), file_descs)
            }).collect()
        }; // read lock dropped

        // Phase 3: generate folder descriptions (no lock held)
        let folder_descs = describe::generate_folder_descriptions(
            project_root,
            &folder_inputs,
            config.max_concurrent_batches,
        ).await;

        // Phase 4: update descriptions + save to disk (write lock)
        {
            let mut folders = self.inner.folders.write().await;

            // Apply generated descriptions
            for (key, folder) in folders.iter_mut() {
                let folder_path = storage::key_to_folder(key);
                if let Some(desc) = folder_descs.get(&folder_path) {
                    folder.description = desc.clone();
                }
            }

            // Save folder summaries to disk
            let summary_dir = project_root.join(&config.summary_path);
            for (key, folder) in folders.iter() {
                let path = summary_dir.join(format!("{key}.toml"));
                if let Err(e) = storage::save_folder_summary(&path, folder) {
                    tracing::warn!("failed to save {}: {e}", path.display());
                }
            }

            // Build and save project summary
            let commit = discovery::current_commit(project_root).unwrap_or_default();
            let project = ProjectSummary {
                generated: now,
                last_commit: commit.clone(),
                folders: folders.iter().map(|(key, folder)| {
                    (key.clone(), FolderEntry {
                        path: format!("{}/", storage::key_to_folder(key)),
                        description: folder.description.clone(),
                    })
                }).collect(),
            };

            *self.inner.project.write().await = Some(project.clone());

            let project_path = summary_dir.join("project-summary.toml");
            if let Err(e) = storage::save_project_summary(&project_path, &project) {
                tracing::warn!("failed to save project summary: {e}");
            }

            if let Err(e) = storage::write_last_run(&summary_dir, &commit) {
                tracing::warn!("failed to write .last-run: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn project_map_formatting() {
        let index = Index::new();

        index
            .insert_file(FileSummary {
                path: PathBuf::from("src/main.rs"),
                description: "Entry point".to_owned(),
                symbols: vec!["main()".to_owned()],
                summarized: Utc::now(),
            })
            .await;
        index
            .insert_file(FileSummary {
                path: PathBuf::from("src/config.rs"),
                description: "Configuration loading".to_owned(),
                symbols: vec!["Config".to_owned()],
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

        let map = index.project_map().await;
        assert!(map.contains("src/ — Core logic"));
        assert!(map.contains("  main.rs — Entry point"));
        assert!(map.contains("  config.rs — Configuration loading"));
    }

    #[tokio::test]
    async fn lookup_file_returns_none_for_missing() {
        let index = Index::new();
        assert!(
            index
                .lookup_file(Path::new("nonexistent.rs"))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn ready_flag() {
        let index = Index::new();
        assert!(!index.is_ready());
        index.set_ready(true);
        assert!(index.is_ready());
    }
}
