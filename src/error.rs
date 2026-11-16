use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffmemError {
    #[error("Git error: {0}")]
    Git(#[from] crate::git::error::GitError),

    #[error("Store error: {0}")]
    Store(#[from] crate::store::StoreError),

    #[error("Watcher error: {0}")]
    Watcher(#[from] crate::watcher::error::WatcherError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Corrupted marker block: {0}")]
    MarkerCorrupted(String),

    #[error("Diff parsing or correlation error: {0}")]
    Correlation(String),

    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    #[error("General error: {0}")]
    General(String),
}

impl From<serde_json::Error> for DiffmemError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

impl From<toml::de::Error> for DiffmemError {
    fn from(err: toml::de::Error) -> Self {
        Self::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for DiffmemError {
    fn from(err: toml::ser::Error) -> Self {
        Self::Config(err.to_string())
    }
}
