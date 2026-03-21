use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Server port. When 0, auto-derived from the project root path.
    #[serde(default)]
    pub port: u16,
    #[serde(default = "default_summary_path")]
    pub summary_path: String,
    #[serde(default)]
    pub ignored_patterns: Vec<String>,
    #[serde(default = "default_session_timeout")]
    pub session_timeout_minutes: u64,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_kb: u64,
    /// Minutes of inactivity before the server shuts itself down.
    /// Set to 0 to disable idle shutdown.
    #[serde(default = "default_idle_shutdown")]
    pub idle_shutdown_minutes: u64,
}

fn default_summary_path() -> String {
    ".claude/summaries".to_owned()
}
fn default_session_timeout() -> u64 {
    60
}
fn default_max_file_size() -> u64 {
    100
}
fn default_idle_shutdown() -> u64 {
    10
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 0,
            summary_path: default_summary_path(),
            ignored_patterns: Vec::new(),
            session_timeout_minutes: default_session_timeout(),
            max_file_size_kb: default_max_file_size(),
            idle_shutdown_minutes: default_idle_shutdown(),
        }
    }
}

impl Config {
    pub fn load(project_root: &Path) -> eyre::Result<Self> {
        let config_path = project_root.join(".claude/youwhatknow.toml");

        let mut config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(config_path))
            .merge(Env::prefixed("YOUWHATKNOW_"))
            .extract()?;

        // Auto-derive port from project root if not explicitly set
        if config.port == 0 {
            config.port = port_for_project(project_root);
        }

        Ok(config)
    }

    /// All default ignore patterns for files that should never be indexed.
    pub fn default_ignore_patterns() -> &'static [&'static str] {
        &[
            "*.min.js",
            "*.min.css",
            "*.generated.*",
            "*.bundle.*",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "Cargo.lock",
            "composer.lock",
            "Gemfile.lock",
            "poetry.lock",
            "*.map",
        ]
    }

    /// Returns the combined set of ignore patterns (defaults + user-configured).
    pub fn all_ignore_patterns(&self) -> Vec<&str> {
        let mut patterns: Vec<&str> = Self::default_ignore_patterns().to_vec();
        for p in &self.ignored_patterns {
            patterns.push(p.as_str());
        }
        patterns
    }
}

/// Derive a deterministic port in the range 7850..8849 from the project root path.
/// Each project gets a consistent port so hooks can find it.
pub fn port_for_project(project_root: &Path) -> u16 {
    let mut hasher = DefaultHasher::new();
    project_root.hash(&mut hasher);
    let hash = hasher.finish();
    // Map into range 7850..8849 (1000 ports)
    7850 + (hash % 1000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = Config::default();
        assert_eq!(config.port, 0);
        assert_eq!(config.summary_path, ".claude/summaries");
        assert_eq!(config.session_timeout_minutes, 60);
        assert_eq!(config.max_file_size_kb, 100);
        assert_eq!(config.idle_shutdown_minutes, 10);
        assert!(config.ignored_patterns.is_empty());
    }

    #[test]
    fn load_from_missing_config_derives_port() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = Config::load(tmp.path()).expect("load");
        // Port should be auto-derived, not 0
        assert!(config.port >= 7850);
        assert!(config.port < 8850);
    }

    #[test]
    fn port_is_deterministic() {
        let root = Path::new("/home/user/project");
        let p1 = port_for_project(root);
        let p2 = port_for_project(root);
        assert_eq!(p1, p2);
    }

    #[test]
    fn different_projects_get_different_ports() {
        let p1 = port_for_project(Path::new("/home/user/project-a"));
        let p2 = port_for_project(Path::new("/home/user/project-b"));
        // Not guaranteed to differ, but extremely likely with good hash
        assert_ne!(p1, p2);
    }

    #[test]
    fn explicit_port_overrides_auto() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            "port = 9999\n",
        )
        .expect("write");

        let config = Config::load(tmp.path()).expect("load");
        assert_eq!(config.port, 9999);
    }

    #[test]
    fn load_from_toml_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            r#"
port = 9999
summary_path = "custom/summaries"
ignored_patterns = ["*.bak"]
session_timeout_minutes = 30
max_file_size_kb = 50
idle_shutdown_minutes = 5
"#,
        )
        .expect("write");

        let config = Config::load(tmp.path()).expect("load");
        assert_eq!(config.port, 9999);
        assert_eq!(config.summary_path, "custom/summaries");
        assert_eq!(config.ignored_patterns, vec!["*.bak"]);
        assert_eq!(config.session_timeout_minutes, 30);
        assert_eq!(config.max_file_size_kb, 50);
        assert_eq!(config.idle_shutdown_minutes, 5);
    }

    #[test]
    fn all_ignore_patterns_includes_defaults_and_user() {
        let config = Config {
            ignored_patterns: vec!["*.bak".to_owned(), "*.tmp".to_owned()],
            ..Config::default()
        };
        let patterns = config.all_ignore_patterns();
        assert!(patterns.contains(&"*.min.js"));
        assert!(patterns.contains(&"Cargo.lock"));
        assert!(patterns.contains(&"*.bak"));
        assert!(patterns.contains(&"*.tmp"));
    }
}
