use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

fn default_occurrences() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodicRule {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub path_glob: String,
    pub mistake: String,
    pub fix: String,
    pub hunk_rejected: String,
    pub hunk_accepted: String,
    pub active: bool,
    #[serde(default = "default_occurrences")]
    pub occurrences: u32,
}

impl EpisodicRule {
    pub fn new(
        path_glob: impl Into<String>,
        mistake: impl Into<String>,
        fix: impl Into<String>,
        hunk_rejected: impl Into<String>,
        hunk_accepted: impl Into<String>,
    ) -> Self {
        let path_glob = path_glob.into().replace('\\', "/");
        let mistake = mistake.into();
        let fix = fix.into();
        let hunk_rejected = hunk_rejected.into();
        let hunk_accepted = hunk_accepted.into();
        let id = Self::generate_id(&path_glob, &mistake, &fix, &hunk_rejected);

        Self {
            id,
            timestamp: Utc::now(),
            path_glob,
            mistake,
            fix,
            hunk_rejected,
            hunk_accepted,
            active: true,
            occurrences: 1,
        }
    }

    pub fn generate_id(path_glob: &str, mistake: &str, fix: &str, hunk_rejected: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path_glob.as_bytes());
        hasher.update(b":");
        hasher.update(mistake.as_bytes());
        hasher.update(b":");
        hasher.update(fix.as_bytes());
        hasher.update(b":");
        hasher.update(hunk_rejected.trim().as_bytes());
        let hash = hasher.finalize();
        let hex_str = hash.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>();
        format!("mem-{}", hex_str)
    }

    pub fn matches_path(&self, file_path: &str) -> bool {
        let normalized = file_path.replace('\\', "/");
        if let Ok(pattern) = glob::Pattern::new(&self.path_glob) {
            pattern.matches(&normalized)
        } else {
            normalized.starts_with(self.path_glob.trim_end_matches('*'))
        }
    }

    pub fn matches_path_buf(&self, path: &Path) -> bool {
        let s = path.to_string_lossy();
        self.matches_path(&s)
    }
}
