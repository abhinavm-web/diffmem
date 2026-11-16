use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitHash(pub String);

impl fmt::Display for CommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for CommitHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileDeltaKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDelta {
    /// Canonical relative path with forward slashes (e.g. "src/auth/token.rs")
    pub path: String,
    pub kind: FileDeltaKind,
    pub is_staged: bool,
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub head_commit: Option<CommitHash>,
    pub deltas: Vec<FileDelta>,
}

impl GitStatus {
    pub fn is_clean(&self) -> bool {
        self.deltas.is_empty()
    }

    pub fn modified_paths(&self) -> Vec<String> {
        self.deltas.iter().map(|d| d.path.clone()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: CommitHash,
    pub parent_hash: Option<CommitHash>,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub message: String,
}
