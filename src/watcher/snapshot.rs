use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use super::error::WatcherError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotState {
    /// In-progress uncommitted working tree edit
    Draft,
    /// Working tree was reverted/discarded without a commit
    Discarded {
        discarded_at: DateTime<Utc>,
        reason: String,
    },
    /// Correlated against a commit diff and promoted to a rule
    Correlated {
        rule_id: String,
        committed_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub head_commit: String,
    pub dirty_files: Vec<String>,
    pub unified_diff: String,
    pub diff_hash: String,
    pub state: SnapshotState,
}

pub struct SnapshotStore {
    snapshots_dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(repo_root: &Path) -> Result<Self, WatcherError> {
        let dot_diffmem = repo_root.join(".diffmem");
        let snapshots_dir = dot_diffmem.join("snapshots");
        fs::create_dir_all(&snapshots_dir)?;

        let gitignore = dot_diffmem.join(".gitignore");
        if !gitignore.exists() {
            let _ = fs::write(gitignore, "snapshots/\n*.pid\n*.log\n");
        }

        Ok(Self { snapshots_dir })
    }

    pub fn save(&self, snapshot: &Snapshot) -> Result<(), WatcherError> {
        let target_file = self.snapshots_dir.join(format!("{}.json", snapshot.id));
        let serialized = serde_json::to_string_pretty(snapshot)?;

        let temp_file = tempfile::NamedTempFile::new_in(&self.snapshots_dir)?;
        fs::write(temp_file.path(), serialized.as_bytes())?;
        temp_file.persist(&target_file).map_err(|e| e.error)?;

        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Option<Snapshot>, WatcherError> {
        let path = self.snapshots_dir.join(format!("{}.json", id));
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(path)?;
        let snapshot: Snapshot = serde_json::from_str(&data)?;
        Ok(Some(snapshot))
    }

    pub fn list_all(&self) -> Result<Vec<Snapshot>, WatcherError> {
        let mut list = Vec::new();
        if !self.snapshots_dir.exists() {
            return Ok(list);
        }

        for entry in fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(snap) = serde_json::from_str::<Snapshot>(&data) {
                        list.push(snap);
                    }
                }
            }
        }

        // Descending order: newest first
        list.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        Ok(list)
    }

    pub fn find_candidate_rejected(&self, head_commit: &str, file_paths: &[String]) -> Result<Vec<Snapshot>, WatcherError> {
        let all = self.list_all()?;
        let matched = all
            .into_iter()
            .filter(|s| {
                (s.head_commit == head_commit || matches!(s.state, SnapshotState::Discarded { .. }))
                    && s.dirty_files.iter().any(|f| file_paths.contains(f))
            })
            .collect();
        Ok(matched)
    }

    pub fn prune(&self, keep_latest: usize) -> Result<usize, WatcherError> {
        let mut all = self.list_all()?;
        if all.len() <= keep_latest {
            return Ok(0);
        }

        let mut deleted = 0;
        for snap in all.drain(keep_latest..) {
            let p = self.snapshots_dir.join(format!("{}.json", snap.id));
            if p.exists() {
                let _ = fs::remove_file(p);
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_snapshot_store_lifecycle() {
        let dir = tempdir().unwrap();
        let store = SnapshotStore::new(dir.path()).unwrap();

        let snap = Snapshot {
            id: "snap-1".to_string(),
            timestamp: Utc::now(),
            head_commit: "commit-1".to_string(),
            dirty_files: vec!["src/lib.rs".to_string()],
            unified_diff: "+ test line\n".to_string(),
            diff_hash: "hash-1".to_string(),
            state: SnapshotState::Draft,
        };

        store.save(&snap).unwrap();

        let loaded = store.load("snap-1").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().head_commit, "commit-1");

        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 1);

        let candidates = store.find_candidate_rejected("commit-1", &["src/lib.rs".to_string()]).unwrap();
        assert_eq!(candidates.len(), 1);

        // Pruning
        let pruned = store.prune(0).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(store.list_all().unwrap().len(), 0);
    }
}
