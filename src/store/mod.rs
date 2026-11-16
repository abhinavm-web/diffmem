pub mod rule;

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use crate::store::rule::EpisodicRule;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Rule not found with id: {0}")]
    RuleNotFound(String),
}

pub struct RuleStore {
    file_path: PathBuf,
    rules: Vec<EpisodicRule>,
}

impl RuleStore {
    pub const DEFAULT_FILE_NAME: &'static str = ".diffmem/rules.json";

    pub fn load_or_init(repo_root: &Path) -> Result<Self, StoreError> {
        Self::load_or_create(repo_root)
    }

    pub fn load_or_create(repo_root: &Path) -> Result<Self, StoreError> {
        let file_path = repo_root.join(Self::DEFAULT_FILE_NAME);
        if !file_path.exists() {
            return Ok(Self {
                file_path,
                rules: Vec::new(),
            });
        }

        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let rules: Vec<EpisodicRule> = match serde_json::from_reader(reader) {
            Ok(r) => r,
            Err(e) => {
                // If empty file, initialize with empty vec
                if fs::metadata(&file_path)?.len() == 0 {
                    Vec::new()
                } else {
                    return Err(StoreError::Json(e));
                }
            }
        };

        Ok(Self { file_path, rules })
    }

    pub fn save(&self) -> Result<(), StoreError> {
        let parent = self.file_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let temp_path = self.file_path.with_extension(format!("tmp.{}", std::process::id()));
        {
            let file = File::create(&temp_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, &self.rules)?;
            writer.flush()?;
        }

        fs::rename(&temp_path, &self.file_path)?;
        Ok(())
    }

    pub fn add_rule(&mut self, rule: EpisodicRule) -> Result<bool, StoreError> {
        if self.rules.iter().any(|r| r.id == rule.id) {
            return Ok(false);
        }

        if let Some(existing) = self.rules.iter_mut().find(|r| {
            r.path_glob == rule.path_glob && r.hunk_rejected.trim() == rule.hunk_rejected.trim()
        }) {
            existing.timestamp = rule.timestamp;
            existing.fix = rule.fix;
            existing.hunk_accepted = rule.hunk_accepted;
            return Ok(false);
        }

        self.rules.push(rule);
        Ok(true)
    }

    pub fn delete_rule(&mut self, id: &str) -> Result<bool, StoreError> {
        let initial_len = self.rules.len();
        self.rules.retain(|r| r.id != id);
        if self.rules.len() < initial_len {
            Ok(true)
        } else {
            Err(StoreError::RuleNotFound(id.to_string()))
        }
    }

    pub fn deactivate_rule(&mut self, id: &str) -> Result<bool, StoreError> {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.active = false;
            Ok(true)
        } else {
            Err(StoreError::RuleNotFound(id.to_string()))
        }
    }

    pub fn activate_rule(&mut self, id: &str) -> Result<bool, StoreError> {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.active = true;
            Ok(true)
        } else {
            Err(StoreError::RuleNotFound(id.to_string()))
        }
    }

    pub fn get_rule(&self, id: &str) -> Option<&EpisodicRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn all_rules(&self) -> &[EpisodicRule] {
        &self.rules
    }

    pub fn active_rules(&self) -> Vec<&EpisodicRule> {
        self.rules.iter().filter(|r| r.active).collect()
    }

    pub fn list_rules(&self, include_inactive: bool, glob_filter: Option<&str>) -> Vec<&EpisodicRule> {
        self.rules
            .iter()
            .filter(|r| {
                if !include_inactive && !r.active {
                    return false;
                }
                if let Some(gf) = glob_filter {
                    let gf_norm = gf.replace('\\', "/");
                    if !r.path_glob.contains(&gf_norm) && !r.matches_path(&gf_norm) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn find_rules_for_path(&self, file_path: &Path) -> Vec<&EpisodicRule> {
        let path_str = file_path.to_string_lossy().replace('\\', "/");
        let mut matched: Vec<&EpisodicRule> = self
            .rules
            .iter()
            .filter(|r| r.active && r.matches_path(&path_str))
            .collect();

        // Sort by specificity (longer glob pattern first)
        matched.sort_by_key(|b| std::cmp::Reverse(b.path_glob.len()));
        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_crud_and_query() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::load_or_create(dir.path()).unwrap();
        assert_eq!(store.all_rules().len(), 0);

        let rule1 = EpisodicRule::new(
            "src/auth/**",
            "Plaintext password storage",
            "Hash with argon2",
            "- store_plain(pw);",
            "+ store_hash(argon2(pw));",
        );
        let id1 = rule1.id.clone();
        assert!(store.add_rule(rule1).unwrap());
        store.save().unwrap();

        // Reload
        let mut loaded = RuleStore::load_or_create(dir.path()).unwrap();
        assert_eq!(loaded.all_rules().len(), 1);
        assert_eq!(loaded.find_rules_for_path(Path::new("src/auth/login.rs")).len(), 1);
        assert_eq!(loaded.find_rules_for_path(Path::new("src/other/test.rs")).len(), 0);

        // Deactivate
        loaded.deactivate_rule(&id1).unwrap();
        assert_eq!(loaded.active_rules().len(), 0);
        assert_eq!(loaded.all_rules().len(), 1);

        // Reactivate
        loaded.activate_rule(&id1).unwrap();
        assert_eq!(loaded.active_rules().len(), 1);

        // Delete
        loaded.delete_rule(&id1).unwrap();
        assert_eq!(loaded.all_rules().len(), 0);
    }
}
