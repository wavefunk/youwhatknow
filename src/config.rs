use std::path::{Path, PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

/// System-wide daemon configuration.
/// Lives at ~/.config/youwhatknow/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_session_timeout")]
    pub session_timeout_minutes: u64,
    /// Minutes of inactivity (no requests at all) before the daemon shuts down.
    /// Set to 0 to disable.
    #[serde(default = "default_idle_shutdown")]
    pub idle_shutdown_minutes: u64,
}

fn default_port() -> u16 {
    7849
}
fn default_session_timeout() -> u64 {
    60
}
fn default_idle_shutdown() -> u64 {
    30
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            session_timeout_minutes: default_session_timeout(),
            idle_shutdown_minutes: default_idle_shutdown(),
        }
    }
}

impl Config {
    pub fn load() -> eyre::Result<Self> {
        let config_path = config_dir().join("config.toml");

        let config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(config_path))
            .merge(Env::prefixed("YOUWHATKNOW_"))
            .extract()?;

        Ok(config)
    }
}

/// Per-project settings, read from `.claude/youwhatknow.toml` inside a project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectConfig {
    #[serde(default = "default_summary_path")]
    pub summary_path: String,
    #[serde(default)]
    pub ignored_patterns: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_kb: u64,
    #[serde(default = "default_max_concurrent_batches")]
    pub max_concurrent_batches: usize,
    #[serde(default = "default_line_threshold")]
    pub line_threshold: u32,
    #[serde(default = "default_eviction_threshold")]
    pub eviction_threshold: u32,
}

fn default_summary_path() -> String {
    ".claude/summaries".to_owned()
}
fn default_max_file_size() -> u64 {
    100
}
fn default_max_concurrent_batches() -> usize {
    4
}
fn default_line_threshold() -> u32 {
    30
}
fn default_eviction_threshold() -> u32 {
    40
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            summary_path: default_summary_path(),
            ignored_patterns: Vec::new(),
            max_file_size_kb: default_max_file_size(),
            max_concurrent_batches: default_max_concurrent_batches(),
            line_threshold: default_line_threshold(),
            eviction_threshold: default_eviction_threshold(),
        }
    }
}

impl ProjectConfig {
    pub fn load(project_root: &Path) -> eyre::Result<Self> {
        let config_path = project_root.join(".claude/youwhatknow.toml");

        let config: ProjectConfig = Figment::new()
            .merge(Serialized::defaults(ProjectConfig::default()))
            .merge(Toml::file(config_path))
            .extract()?;

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

/// User-level config directory: ~/.config/youwhatknow/
pub fn config_dir() -> PathBuf {
    dirs_or_default("config")
}

/// User-level data directory for PID file etc: ~/.local/share/youwhatknow/
pub fn data_dir() -> PathBuf {
    dirs_or_default("data")
}

fn dirs_or_default(kind: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    match kind {
        "config" => PathBuf::from(&home).join(".config/youwhatknow"),
        "data" => PathBuf::from(&home).join(".local/share/youwhatknow"),
        _ => PathBuf::from(&home).join(".youwhatknow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = Config::default();
        assert_eq!(config.port, 7849);
        assert_eq!(config.session_timeout_minutes, 60);
        assert_eq!(config.idle_shutdown_minutes, 30);
    }

    #[test]
    fn default_project_config_values() {
        let config = ProjectConfig::default();
        assert_eq!(config.summary_path, ".claude/summaries");
        assert_eq!(config.max_file_size_kb, 100);
        assert!(config.ignored_patterns.is_empty());
    }

    #[test]
    fn default_project_config_has_concurrent_batches() {
        let config = ProjectConfig::default();
        assert_eq!(config.max_concurrent_batches, 4);
    }

    #[test]
    fn project_config_from_missing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = ProjectConfig::load(tmp.path()).expect("load");
        assert_eq!(config, ProjectConfig::default());
    }

    #[test]
    fn project_config_from_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            r#"
summary_path = "custom/summaries"
ignored_patterns = ["*.bak"]
max_file_size_kb = 50
"#,
        )
        .expect("write");

        let config = ProjectConfig::load(tmp.path()).expect("load");
        assert_eq!(config.summary_path, "custom/summaries");
        assert_eq!(config.ignored_patterns, vec!["*.bak"]);
        assert_eq!(config.max_file_size_kb, 50);
    }

    #[test]
    fn default_project_config_has_line_threshold() {
        let config = ProjectConfig::default();
        assert_eq!(config.line_threshold, 30);
    }

    #[test]
    fn project_config_line_threshold_from_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            "line_threshold = 50\n",
        )
        .expect("write");

        let config = ProjectConfig::load(tmp.path()).expect("load");
        assert_eq!(config.line_threshold, 50);
    }

    #[test]
    fn default_project_config_has_eviction_threshold() {
        let config = ProjectConfig::default();
        assert_eq!(config.eviction_threshold, 40);
    }

    #[test]
    fn eviction_threshold_from_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            "eviction_threshold = 20\n",
        )
        .expect("write");

        let config = ProjectConfig::load(tmp.path()).expect("load");
        assert_eq!(config.eviction_threshold, 20);
    }

    #[test]
    fn eviction_threshold_zero_from_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).expect("mkdir");
        std::fs::write(
            config_dir.join("youwhatknow.toml"),
            "eviction_threshold = 0\n",
        )
        .expect("write");

        let config = ProjectConfig::load(tmp.path()).expect("load");
        assert_eq!(config.eviction_threshold, 0);
    }

    #[test]
    fn all_ignore_patterns_includes_defaults_and_user() {
        let config = ProjectConfig {
            ignored_patterns: vec!["*.bak".to_owned()],
            ..ProjectConfig::default()
        };
        let patterns = config.all_ignore_patterns();
        assert!(patterns.contains(&"*.min.js"));
        assert!(patterns.contains(&"*.bak"));
    }
}
