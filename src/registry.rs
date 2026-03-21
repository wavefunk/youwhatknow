use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::ProjectConfig;
use crate::indexer::Index;

/// Manages per-project indexes. Thread-safe, lazily loads projects on first request.
#[derive(Clone)]
pub struct ProjectRegistry {
    inner: Arc<RwLock<RegistryState>>,
}

struct ProjectEntry {
    index: Index,
    config: ProjectConfig,
}

struct RegistryState {
    projects: HashMap<PathBuf, ProjectEntry>,
}

impl ProjectRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryState {
                projects: HashMap::new(),
            })),
        }
    }

    /// Get the index for a project, loading it lazily if not already known.
    /// Returns (Index, ProjectConfig) for the project.
    pub async fn get_or_load(&self, project_root: &Path) -> (Index, ProjectConfig) {
        // Fast path: already loaded (read lock)
        {
            let state = self.inner.read().await;
            if let Some(entry) = state.projects.get(project_root) {
                return (entry.index.clone(), entry.config.clone());
            }
        }

        // Slow path: acquire write lock and double-check before inserting
        let mut state = self.inner.write().await;

        // Another request may have loaded this project while we waited for the lock
        if let Some(entry) = state.projects.get(project_root) {
            return (entry.index.clone(), entry.config.clone());
        }

        let config = ProjectConfig::load(project_root).unwrap_or_default();
        let index = Index::new();

        // Load existing summaries from disk
        index.load_from_disk(project_root, &config).await;

        // Start background indexing
        let bg_index = index.clone();
        let bg_root = project_root.to_path_buf();
        let bg_config = config.clone();
        tokio::spawn(async move {
            let summary_dir = bg_root.join(&bg_config.summary_path);
            if crate::storage::read_last_run(&summary_dir).is_some() {
                bg_index.incremental_index(&bg_root, &bg_config).await;
            } else {
                bg_index.full_index(&bg_root, &bg_config).await;
            }
        });

        state.projects.insert(
            project_root.to_path_buf(),
            ProjectEntry {
                index: index.clone(),
                config: config.clone(),
            },
        );

        tracing::info!(project = %project_root.display(), "registered new project");
        (index, config)
    }

    /// Get the index for a project if it's already loaded, without triggering a load.
    #[cfg(test)]
    pub async fn get(&self, project_root: &Path) -> Option<(Index, ProjectConfig)> {
        let state = self.inner.read().await;
        state
            .projects
            .get(project_root)
            .map(|e| (e.index.clone(), e.config.clone()))
    }

    /// Number of registered projects.
    pub async fn project_count(&self) -> usize {
        self.inner.read().await.projects.len()
    }

    /// Trigger a full re-index for a specific project.
    pub async fn reindex(&self, project_root: &Path) {
        let (index, config) = self.get_or_load(project_root).await;
        let root = project_root.to_path_buf();
        tokio::spawn(async move {
            index.full_index(&root, &config).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lazy_load_creates_entry() {
        let registry = ProjectRegistry::new();
        let tmp = tempfile::tempdir().expect("tempdir");

        assert_eq!(registry.project_count().await, 0);

        let (index, _config) = registry.get_or_load(tmp.path()).await;
        assert_eq!(registry.project_count().await, 1);

        // Second call returns the same index
        let (index2, _) = registry.get_or_load(tmp.path()).await;
        assert_eq!(registry.project_count().await, 1);

        // They should be the same Arc
        assert!(index.is_ready() == index2.is_ready());
    }

    #[tokio::test]
    async fn different_projects_get_different_indexes() {
        let registry = ProjectRegistry::new();
        let tmp1 = tempfile::tempdir().expect("tempdir");
        let tmp2 = tempfile::tempdir().expect("tempdir");

        registry.get_or_load(tmp1.path()).await;
        registry.get_or_load(tmp2.path()).await;

        assert_eq!(registry.project_count().await, 2);
    }

    #[tokio::test]
    async fn get_without_load_returns_none() {
        let registry = ProjectRegistry::new();
        let tmp = tempfile::tempdir().expect("tempdir");

        assert!(registry.get(tmp.path()).await.is_none());
    }
}
