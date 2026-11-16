pub mod error;
pub mod snapshot;

pub use error::WatcherError;
pub use snapshot::{Snapshot, SnapshotState, SnapshotStore};

use crate::adapters::{AdapterRegistry, SyncTarget};
use crate::correlator::Correlator;
use crate::error::DiffmemError;
use crate::git::{CommitHash, GitCli, GitOps};
use crate::store::RuleStore;
use chrono::Utc;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct RepoWatcher {
    git: GitCli,
    store: SnapshotStore,
    debounce_duration: Duration,
    stop_signal: Arc<AtomicBool>,
}

impl RepoWatcher {
    pub fn new(repo_root: impl Into<PathBuf>) -> Result<Self, WatcherError> {
        let root = repo_root.into();
        let git = GitCli::new(&root);
        let store = SnapshotStore::new(&root)?;
        Ok(Self {
            git,
            store,
            debounce_duration: Duration::from_millis(500),
            stop_signal: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce_duration = debounce;
        self
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_signal)
    }

    pub fn run_loop<F>(&self, mut on_commit_detected: F) -> Result<(), WatcherError>
    where
        F: FnMut(&str) -> Result<(), WatcherError>,
    {
        let repo_root = self.git.repo_root().to_path_buf();
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;

        watcher.watch(&repo_root, RecursiveMode::Recursive)?;

        let mut pending_paths: HashSet<PathBuf> = HashSet::new();
        let mut last_event_time: Option<Instant> = None;
        let mut last_head_commit = self.git.get_head_commit().ok().map(|h| h.0);
        let mut last_snapshot_hash: Option<String> = None;

        while !self.stop_signal.load(Ordering::Relaxed) {
            let timeout = match last_event_time {
                Some(t) => {
                    let elapsed = t.elapsed();
                    if elapsed >= self.debounce_duration {
                        self.process_dirty_state(
                            &mut pending_paths,
                            &mut last_head_commit,
                            &mut last_snapshot_hash,
                            &mut on_commit_detected,
                        )?;
                        last_event_time = None;
                        continue;
                    } else {
                        self.debounce_duration - elapsed
                    }
                }
                None => Duration::from_millis(250),
            };

            match rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    if self.should_ignore_event(&event) {
                        continue;
                    }
                    for path in event.paths {
                        pending_paths.insert(path);
                    }
                    last_event_time = Some(Instant::now());
                }
                Ok(Err(_)) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if !pending_paths.is_empty() {
                        self.process_dirty_state(
                            &mut pending_paths,
                            &mut last_head_commit,
                            &mut last_snapshot_hash,
                            &mut on_commit_detected,
                        )?;
                        last_event_time = None;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        Ok(())
    }

    fn should_ignore_event(&self, event: &Event) -> bool {
        let repo_root = self.git.repo_root();
        for path in &event.paths {
            if let Ok(rel) = path.strip_prefix(repo_root) {
                let s = rel.to_string_lossy().replace('\\', "/");
                if s.starts_with(".git")
                    || s.starts_with(".diffmem")
                    || s.starts_with("target")
                    || s.starts_with("node_modules")
                    || s.starts_with(".venv")
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn process_dirty_state<F>(
        &self,
        pending_paths: &mut HashSet<PathBuf>,
        last_head_commit: &mut Option<String>,
        last_snapshot_hash: &mut Option<String>,
        on_commit_detected: &mut F,
    ) -> Result<(), WatcherError>
    where
        F: FnMut(&str) -> Result<(), WatcherError>,
    {
        pending_paths.clear();

        let current_head = self.git.get_head_commit().ok().map(|h| h.0);

        // Check if HEAD advanced (a git commit occurred)
        if let (Some(cur), Some(prev)) = (&current_head, last_head_commit.as_ref()) {
            if cur != prev {
                *last_head_commit = current_head.clone();
                *last_snapshot_hash = None;
                on_commit_detected(cur)?;
            }
        } else if current_head.is_some() && last_head_commit.is_none() {
            *last_head_commit = current_head.clone();
        }

        let status = self.git.get_status()?;
        if status.is_clean() {
            // Working tree is clean: if we had a prior active draft, mark it Discarded!
            if let Some(prev_hash) = last_snapshot_hash.take() {
                let snapshots = self.store.list_all()?;
                if let Some(mut draft) = snapshots
                    .into_iter()
                    .find(|s| s.diff_hash == prev_hash && s.state == SnapshotState::Draft)
                {
                    draft.state = SnapshotState::Discarded {
                        discarded_at: Utc::now(),
                        reason: "working_tree_reverted".to_string(),
                    };
                    self.store.save(&draft)?;
                }
            }
            return Ok(());
        }

        // Working tree is dirty: capture uncommitted diff
        let diff = self.git.get_working_diff(None)?;
        if diff.trim().is_empty() {
            return Ok(());
        }

        let diff_hash = blake3::hash(diff.as_bytes()).to_hex().to_string();
        if Some(&diff_hash) == last_snapshot_hash.as_ref() {
            return Ok(());
        }

        let head_str = current_head
            .unwrap_or_else(|| "0000000000000000000000000000000000000000".to_string());
        let snapshot_id = format!("{}-{}", Utc::now().format("%Y%m%d%H%M%S"), &diff_hash[..8]);

        let snapshot = Snapshot {
            id: snapshot_id,
            timestamp: Utc::now(),
            head_commit: head_str,
            dirty_files: status.modified_paths(),
            unified_diff: diff,
            diff_hash: diff_hash.clone(),
            state: SnapshotState::Draft,
        };

        self.store.save(&snapshot)?;
        *last_snapshot_hash = Some(diff_hash);

        // Keep disk bounded: prune old snapshots beyond 100 entries
        let _ = self.store.prune(100);

        Ok(())
    }
}

pub fn run_watcher(repo_root: &Path, interval_secs: f64, daemon: bool) -> Result<(), DiffmemError> {
    let watcher = RepoWatcher::new(repo_root)?
        .with_debounce(Duration::from_millis((interval_secs * 1000.0) as u64));

    let repo_path = repo_root.to_path_buf();
    let git = GitCli::new(&repo_path);
    let snapshot_store = SnapshotStore::new(&repo_path)?;

    println!(
        "[diffmem] Watcher started in repo: {} (daemon={})",
        repo_path.display(),
        daemon
    );

    let on_commit = move |head_commit: &str| -> Result<(), WatcherError> {
        let commit_hash = CommitHash(head_commit.to_string());
        let commit_diff = match git.get_commit_diff(&commit_hash) {
            Ok(d) => d,
            Err(_) => return Ok(()),
        };

        if commit_diff.trim().is_empty() {
            return Ok(());
        }

        let mut rule_store = match RuleStore::load_or_create(&repo_path) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };

        let candidate_snapshots = snapshot_store.list_all()?;
        let correlator = Correlator::new(2);

        for mut snap in candidate_snapshots {
            if matches!(snap.state, SnapshotState::Correlated { .. }) {
                continue;
            }

            let rules = correlator.correlate(&snap.unified_diff, &commit_diff);
            for rule in rules {
                let rule_id = rule.id.clone();
                let _ = rule_store.add_rule(rule);

                snap.state = SnapshotState::Correlated {
                    rule_id: rule_id.clone(),
                    committed_at: Utc::now(),
                };
                let _ = snapshot_store.save(&snap);
            }
        }

        let _ = rule_store.save();
        let registry = AdapterRegistry::new();
        let _ = registry.sync(SyncTarget::All, &repo_path, rule_store.all_rules(), false);

        Ok(())
    };

    if !daemon {
        // Run until interrupted
        watcher.run_loop(on_commit)?;
    } else {
        // Daemon loop
        watcher.run_loop(on_commit)?;
    }

    Ok(())
}
