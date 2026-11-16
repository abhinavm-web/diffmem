use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Git executable not found in system PATH: {0}")]
    GitNotFound(#[source] std::io::Error),

    #[error("Directory is not inside a git repository: {0}")]
    NotAGitRepo(PathBuf),

    #[error("Git command '{cmd}' failed (exit code {exit_code:?}): {stderr}")]
    CommandFailed {
        cmd: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("Failed to parse git output: {0}")]
    ParseError(String),

    #[error("Path '{0}' is outside repository root '{1}'")]
    PathOutsideRepo(PathBuf, PathBuf),

    #[error("Index lock contended on {0}")]
    IndexLockContended(PathBuf),

    #[error("IO error during git operation: {0}")]
    Io(#[from] std::io::Error),
}
