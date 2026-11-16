use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::DiffmemError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncTarget {
    #[default]
    All,
    Claude,
    Cursor,
    Antigravity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub sync: SyncConfig,
    pub watcher: WatcherConfig,
    pub correlator: CorrelatorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub version: u32,
    pub rules_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub default_target: SyncTarget,
    pub claude_file: String,
    pub cursor_dir: String,
    pub agents_file: String,
    pub auto_sync_on_commit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
    pub snapshots_dir: String,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatorConfig {
    pub line_tolerance: usize,
    pub min_hunk_similarity: f32,
    pub max_active_rules: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                version: 1,
                rules_file: ".diffmem/rules.json".into(),
            },
            sync: SyncConfig {
                default_target: SyncTarget::All,
                claude_file: "CLAUDE.md".into(),
                cursor_dir: ".cursor/rules".into(),
                agents_file: "AGENTS.md".into(),
                auto_sync_on_commit: true,
            },
            watcher: WatcherConfig {
                debounce_ms: 500,
                snapshots_dir: ".diffmem/snapshots".into(),
                ignore_patterns: vec![
                    ".git/**".into(),
                    "target/**".into(),
                    "node_modules/**".into(),
                    ".diffmem/**".into(),
                ],
            },
            correlator: CorrelatorConfig {
                line_tolerance: 2,
                min_hunk_similarity: 0.3,
                max_active_rules: 100,
            },
        }
    }
}

impl Config {
    pub fn config_path(workspace: &Path) -> PathBuf {
        workspace.join(".diffmem").join("config.toml")
    }

    pub fn load_or_default(workspace: &Path) -> Self {
        let path = Self::config_path(workspace);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, workspace: &Path) -> Result<(), DiffmemError> {
        let dir = workspace.join(".diffmem");
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| DiffmemError::Config(e.to_string()))?;
        fs::write(Self::config_path(workspace), content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_save_and_load() {
        let dir = tempdir().unwrap();
        let config = Config::default();
        config.save(dir.path()).unwrap();

        let loaded = Config::load_or_default(dir.path());
        assert_eq!(loaded.general.version, 1);
        assert_eq!(loaded.correlator.line_tolerance, 2);
        assert_eq!(loaded.sync.default_target, SyncTarget::All);
    }
}
