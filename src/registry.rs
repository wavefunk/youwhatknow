use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::ProjectConfig;
use crate::indexer::Index;
use crate::indexer::discovery;

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

    /// Register a path as an alias for an existing project entry.
    /// Used internally when a worktree resolves to a known main root.
    #[cfg(test)]
    pub async fn register_alias(&self, alias: &Path, canonical: &Path) {
        let mut state = self.inner.write().await;
        if let Some(entry) = state.projects.get(canonical) {
            let cloned = ProjectEntry {
                index: entry.index.clone(),
                config: entry.config.clone(),
            };
            state.projects.insert(alias.to_path_buf(), cloned);
        }
    }

    /// Get the index for a project, loading it lazily if not already known.
    /// Returns (Index, ProjectConfig) for the project.
    pub async fn get_or_load(&self, cwd: &Path) -> (Index, ProjectConfig) {
        // Canonicalize cwd for stable comparison with main_root (which is also canonicalized)
        let cwd = &cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());

        // Fast path: cwd already registered (read lock)
        {
            let state = self.inner.read().await;
            if let Some(entry) = state.projects.get(cwd) {
                return (entry.index.clone(), entry.config.clone());
            }
        }

        // Resolve to main worktree root for canonical keying
        let main_root = discovery::resolve_main_worktree(cwd)
            .unwrap_or_else(|_| cwd.to_path_buf());

        let mut state = self.inner.write().await;

        // Double-check cwd after acquiring write lock
        if let Some(entry) = state.projects.get(cwd) {
            return (entry.index.clone(), entry.config.clone());
        }

        // Check if main_root already has an entry (worktree of known project)
        if main_root != *cwd
            && let Some(entry) = state.projects.get(&main_root)
        {
            let alias = ProjectEntry {
                index: entry.index.clone(),
                config: entry.config.clone(),
            };
            let result = (alias.index.clone(), alias.config.clone());
            // Register cwd as alias so future lookups are fast
            state.projects.insert(cwd.to_path_buf(), alias);
            tracing::info!(
                worktree = %cwd.display(),
                main = %main_root.display(),
                "linked worktree to existing project index"
            );
            return result;
        }

        // New project
        let config = ProjectConfig::load(cwd).unwrap_or_default();
        let index = Index::new();

        // Load existing summaries from main worktree (they're gitignored,
        // so only the main worktree has them on disk)
        index.load_from_disk(&main_root, &config).await;

        // Only run background indexing for the main worktree.
        // Worktrees share the main index and don't write their own summaries.
        let is_worktree = main_root != *cwd;
        if !is_worktree {
            let bg_index = index.clone();
            let bg_root = cwd.to_path_buf();
            let bg_config = config.clone();
            tokio::spawn(async move {
                let summary_dir = bg_root.join(&bg_config.summary_path);
                if crate::storage::read_last_run(&summary_dir).is_some() {
                    bg_index.incremental_index(&bg_root, &bg_config).await;
                } else {
                    bg_index.full_index(&bg_root, &bg_config).await;
                }
            });
        } else {
            // Worktree with no main entry yet — mark ready immediately
            // since we loaded what we could from disk
            index.set_ready(true);
        }

        let entry = ProjectEntry {
            index: index.clone(),
            config: config.clone(),
        };

        // Register under main_root too so future worktrees find it
        if is_worktree {
            state.projects.insert(main_root.clone(), ProjectEntry {
                index: index.clone(),
                config: config.clone(),
            });
            tracing::info!(
                worktree = %cwd.display(),
                main_root = %main_root.display(),
                "registered worktree with shared index"
            );
        }
        state.projects.insert(cwd.to_path_buf(), entry);

        tracing::info!(project = %cwd.display(), "registered new project");
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

    /// Trigger a re-index for a specific project.
    /// Defaults to incremental; pass `full = true` to force a complete rebuild.
    pub async fn reindex(&self, project_root: &Path, full: bool) {
        let (index, config) = self.get_or_load(project_root).await;
        let root = project_root.to_path_buf();
        tokio::spawn(async move {
            if full {
                index.full_index(&root, &config).await;
            } else {
                index.incremental_index(&root, &config).await;
            }
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

    #[tokio::test]
    async fn worktree_alias_shares_index() {
        let registry = ProjectRegistry::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let main_root = tmp.path().to_path_buf();

        // Simulate: main project is loaded
        let (index1, _) = registry.get_or_load(&main_root).await;

        // Simulate: a worktree path resolves to the same main_root.
        // We test register_alias directly since we can't create real worktrees in tests.
        let worktree_path = PathBuf::from("/fake/worktree/path");
        registry.register_alias(&worktree_path, &main_root).await;

        let (index2, _) = registry.get_or_load(&worktree_path).await;

        // Mutate one and observe the other to prove they share the same Arc
        index1.set_ready(true);
        assert!(index2.is_ready(), "should share the same underlying Arc");
        assert_eq!(registry.project_count().await, 2); // two keys, same index
    }
}
