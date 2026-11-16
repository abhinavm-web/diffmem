//! diffmem: Zero-touch episodic failure memory for coding agents.
//!
//! Observes git activity to correlate rejected or reverted agent diffs against final commit changes,
//! synthesizing path-scoped episodic memory rules across Claude Code (`CLAUDE.md`), Cursor (`.cursor/rules/*.mdc`),
//! and Antigravity/Codex (`AGENTS.md`) without requiring explicit agent tool calls.

#![deny(clippy::all)]

pub mod adapters;
pub mod config;
pub mod correlator;
pub mod error;
pub mod git;
pub mod store;
pub mod watcher;

pub use adapters::{AdapterRegistry, SyncAdapter, SyncReport, SyncStatus};
pub use config::{Config, SyncTarget};
pub use correlator::{correlate_diffs, Correlator};
pub use error::DiffmemError;
pub use git::{install_post_commit_hook, uninstall_post_commit_hook, GitCli, GitOps};
pub use store::{rule::EpisodicRule, RuleStore};
pub use watcher::{run_watcher, RepoWatcher, Snapshot, SnapshotState, SnapshotStore};
