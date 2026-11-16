pub mod error;
pub mod hook;
pub mod types;

pub use error::GitError;
pub use hook::{install_post_commit_hook, uninstall_post_commit_hook, HookManager};
pub use types::*;

use std::path::{Path, PathBuf};
use std::process::Command;

pub trait GitOps {
    fn repo_root(&self) -> &Path;
    fn get_head_commit(&self) -> Result<CommitHash, GitError>;
    fn get_status(&self) -> Result<GitStatus, GitError>;
    fn get_working_diff(&self, paths: Option<&[&str]>) -> Result<String, GitError>;
    fn get_commit_diff(&self, commit: &CommitHash) -> Result<String, GitError>;
    fn get_file_at_commit(&self, commit: &CommitHash, relative_path: &str) -> Result<Option<String>, GitError>;
    fn get_commit_info(&self, commit: &CommitHash) -> Result<CommitInfo, GitError>;
    fn normalize_path(&self, path: &Path) -> Result<String, GitError>;
}

#[derive(Debug, Clone)]
pub struct GitCli {
    repo_root: PathBuf,
}

impl GitCli {
    pub fn discover(start_path: impl AsRef<Path>) -> Result<Self, GitError> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .current_dir(start_path.as_ref())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GitError::GitNotFound(e)
                } else {
                    GitError::Io(e)
                }
            })?;

        if !output.status.success() {
            return Err(GitError::NotAGitRepo(start_path.as_ref().to_path_buf()));
        }

        let root_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Self {
            repo_root: PathBuf::from(root_str),
        })
    }

    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn run_git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GitError::GitNotFound(e)
                } else {
                    GitError::Io(e)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitError::CommandFailed {
                cmd: format!("git {}", args.join(" ")),
                exit_code: output.status.code(),
                stderr,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl GitOps for GitCli {
    fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    fn get_head_commit(&self) -> Result<CommitHash, GitError> {
        let out = self.run_git(&["rev-parse", "HEAD"])?;
        Ok(CommitHash(out.trim().to_string()))
    }

    fn get_status(&self) -> Result<GitStatus, GitError> {
        let out = self.run_git(&["status", "--porcelain=v1", "-uall", "-z"])?;
        let bytes = out.as_bytes();
        let mut deltas = Vec::new();
        let mut idx = 0;

        while idx < bytes.len() {
            if idx + 3 > bytes.len() {
                break;
            }
            let x = bytes[idx] as char;
            let y = bytes[idx + 1] as char;
            idx += 3; // skip XY and space

            // Find terminating NUL
            let path_end = bytes[idx..].iter().position(|&b| b == 0)
                .map(|p| idx + p)
                .unwrap_or(bytes.len());
            let path_str = String::from_utf8_lossy(&bytes[idx..path_end]).replace('\\', "/");
            idx = path_end + 1;

            if x == 'R' || y == 'R' {
                // Next string is the new path in a rename
                let new_end = bytes[idx..].iter().position(|&b| b == 0)
                    .map(|p| idx + p)
                    .unwrap_or(bytes.len());
                let new_path = String::from_utf8_lossy(&bytes[idx..new_end]).replace('\\', "/");
                idx = new_end + 1;
                deltas.push(FileDelta {
                    path: new_path,
                    kind: FileDeltaKind::Renamed,
                    is_staged: x != ' ' && x != '?',
                    old_path: Some(path_str),
                });
            } else {
                let kind = if x == '?' && y == '?' {
                    FileDeltaKind::Untracked
                } else if x == 'D' || y == 'D' {
                    FileDeltaKind::Deleted
                } else if x == 'A' || y == 'A' {
                    FileDeltaKind::Added
                } else {
                    FileDeltaKind::Modified
                };

                deltas.push(FileDelta {
                    path: path_str,
                    kind,
                    is_staged: x != ' ' && x != '?',
                    old_path: None,
                });
            }
        }

        let head = self.get_head_commit().ok();
        Ok(GitStatus {
            branch: None,
            head_commit: head,
            deltas,
        })
    }

    fn get_working_diff(&self, paths: Option<&[&str]>) -> Result<String, GitError> {
        let mut args = vec!["diff", "HEAD", "--unified=3"];
        if let Some(p) = paths {
            args.push("--");
            args.extend_from_slice(p);
        }
        self.run_git(&args)
    }

    fn get_commit_diff(&self, commit: &CommitHash) -> Result<String, GitError> {
        self.run_git(&["show", "--format=", "--unified=3", commit.as_ref()])
    }

    fn get_file_at_commit(&self, commit: &CommitHash, relative_path: &str) -> Result<Option<String>, GitError> {
        let spec = format!("{}:{}", commit.as_ref(), relative_path.replace('\\', "/"));
        match self.run_git(&["show", &spec]) {
            Ok(content) => Ok(Some(content)),
            Err(GitError::CommandFailed { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn get_commit_info(&self, commit: &CommitHash) -> Result<CommitInfo, GitError> {
        let out = self.run_git(&["log", "-1", "--format=%H%x00%P%x00%an%x00%ae%x00%aI%x00%B", commit.as_ref()])?;
        let parts: Vec<&str> = out.split('\0').collect();
        if parts.len() < 6 {
            return Err(GitError::ParseError(format!("Malformed commit info: {}", out)));
        }

        let hash = CommitHash(parts[0].trim().to_string());
        let parent = if parts[1].trim().is_empty() { None } else { Some(CommitHash(parts[1].trim().to_string())) };
        let author_name = parts[2].trim().to_string();
        let author_email = parts[3].trim().to_string();
        let timestamp = chrono::DateTime::parse_from_rfc3339(parts[4].trim())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| GitError::ParseError(e.to_string()))?;
        let message = parts[5].trim().to_string();

        Ok(CommitInfo {
            hash,
            parent_hash: parent,
            author_name,
            author_email,
            timestamp,
            message,
        })
    }

    fn normalize_path(&self, path: &Path) -> Result<String, GitError> {
        let rel = if path.is_absolute() {
            path.strip_prefix(&self.repo_root)
                .map_err(|_| GitError::PathOutsideRepo(path.to_path_buf(), self.repo_root.clone()))?
        } else {
            path
        };
        let normalized = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/");
        Ok(normalized)
    }
}
