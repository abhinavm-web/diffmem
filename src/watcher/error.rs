use crate::git::GitError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("Filesystem notification error: {0}")]
    Notify(#[from] notify::Error),

    #[error("Git error encountered in watcher: {0}")]
    Git(#[from] GitError),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Event channel disconnected")]
    ChannelDisconnected,

    #[error("General watcher error: {0}")]
    General(String),
}
