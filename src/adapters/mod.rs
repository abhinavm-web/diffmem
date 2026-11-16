pub mod agents_md;
pub mod claude;
pub mod cursor;

pub use agents_md::AgentsMdAdapter;
pub use claude::ClaudeAdapter;
pub use cursor::CursorAdapter;

use std::path::{Path, PathBuf};
pub use crate::config::SyncTarget;
use crate::error::DiffmemError;
use crate::store::rule::EpisodicRule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Created,
    Updated,
    Unchanged,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub adapter_name: &'static str,
    pub target_path: PathBuf,
    pub rules_written: usize,
    pub status: SyncStatus,
}

pub trait SyncAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn sync(&self, workspace: &Path, rules: &[EpisodicRule], dry_run: bool) -> Result<SyncReport, DiffmemError>;
    fn clean(&self, workspace: &Path) -> Result<(), DiffmemError>;
}

pub struct AdapterRegistry {
    adapters: Vec<Box<dyn SyncAdapter>>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(claude::ClaudeAdapter::default()),
                Box::new(cursor::CursorAdapter::default()),
                Box::new(agents_md::AgentsMdAdapter::default()),
            ],
        }
    }

    pub fn sync(
        &self,
        target: SyncTarget,
        workspace: &Path,
        rules: &[EpisodicRule],
        dry_run: bool,
    ) -> Result<Vec<SyncReport>, DiffmemError> {
        let active_rules: Vec<EpisodicRule> = rules.iter().filter(|r| r.active).cloned().collect();
        let mut reports = Vec::new();

        for adapter in &self.adapters {
            let matches = match target {
                SyncTarget::All => true,
                SyncTarget::Claude => adapter.name() == "claude",
                SyncTarget::Cursor => adapter.name() == "cursor",
                SyncTarget::Antigravity => adapter.name() == "antigravity",
            };

            if matches {
                let report = adapter.sync(workspace, &active_rules, dry_run)?;
                reports.push(report);
            }
        }

        Ok(reports)
    }

    pub fn clean(&self, workspace: &Path) -> Result<(), DiffmemError> {
        for adapter in &self.adapters {
            adapter.clean(workspace)?;
        }
        Ok(())
    }
}
