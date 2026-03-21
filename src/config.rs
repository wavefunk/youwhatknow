use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_summary_path")]
    pub summary_path: String,
    #[serde(default)]
    pub ignored_patterns: Vec<String>,
    #[serde(default = "default_session_timeout")]
    pub session_timeout_minutes: u64,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_kb: u64,
}

fn default_port() -> u16 {
    7849
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

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            summary_path: default_summary_path(),
            ignored_patterns: Vec::new(),
            session_timeout_minutes: default_session_timeout(),
            max_file_size_kb: default_max_file_size(),
        }
    }
}

impl Config {
    pub fn load(project_root: &std::path::Path) -> eyre::Result<Self> {
        let config_path = project_root.join(".claude/youwhatknow.toml");

        let config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(config_path))
            .merge(Env::prefixed("YOUWHATKNOW_"))
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
        let mut patterns: Vec<&str> =
            Self::default_ignore_patterns().to_vec();
        for p in &self.ignored_patterns {
            patterns.push(p.as_str());
        }
        patterns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = Config::default();
        assert_eq!(config.port, 7849);
        assert_eq!(config.summary_path, ".claude/summaries");
        assert_eq!(config.session_timeout_minutes, 60);
        assert_eq!(config.max_file_size_kb, 100);
        assert!(config.ignored_patterns.is_empty());
    }

    #[test]
    fn load_from_missing_config_gives_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = Config::load(tmp.path()).expect("load");
        assert_eq!(config, Config::default());
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
"#,
        )
        .expect("write");

        let config = Config::load(tmp.path()).expect("load");
        assert_eq!(config.port, 9999);
        assert_eq!(config.summary_path, "custom/summaries");
        assert_eq!(config.ignored_patterns, vec!["*.bak"]);
        assert_eq!(config.session_timeout_minutes, 30);
        assert_eq!(config.max_file_size_kb, 50);
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
