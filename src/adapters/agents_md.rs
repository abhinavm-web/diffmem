use std::fs;
use std::path::Path;
use crate::adapters::{SyncAdapter, SyncReport, SyncStatus};
use crate::error::DiffmemError;
use crate::store::rule::EpisodicRule;

pub const AGENTS_BEGIN_MARKER: &str = "<!-- BEGIN DIFFMEM EPISODIC RULES -->";
pub const AGENTS_END_MARKER: &str = "<!-- END DIFFMEM EPISODIC RULES -->";

#[derive(Debug, Clone)]
pub struct AgentsMdAdapter {
    pub file_name: &'static str,
}

impl Default for AgentsMdAdapter {
    fn default() -> Self {
        Self {
            file_name: "AGENTS.md",
        }
    }
}

impl AgentsMdAdapter {
    pub fn format_episodic_section(&self, rules: &[EpisodicRule]) -> String {
        let mut buf = String::new();
        buf.push_str(AGENTS_BEGIN_MARKER);
        buf.push('\n');

        if rules.is_empty() {
            buf.push_str("<!-- diffmem: no active episodic rules -->\n");
        } else {
            buf.push_str("## Episodic Failure Memory\n\n");
            buf.push_str("Lessons learned from previous reverted agent attempts and committed fixes:\n\n");

            for rule in rules {
                buf.push_str(&format!("### [{}] `{}`\n", rule.id, rule.path_glob));
                buf.push_str(&format!("- **Mistake**: {}\n", rule.mistake));
                buf.push_str(&format!("- **Fix**: {}\n", rule.fix));
                if !rule.hunk_rejected.trim().is_empty() {
                    buf.push_str("```diff\n");
                    buf.push_str(&rule.hunk_rejected);
                    if !rule.hunk_rejected.ends_with('\n') {
                        buf.push('\n');
                    }
                    buf.push_str("```\n");
                }
                buf.push('\n');
            }
        }

        buf.push_str(AGENTS_END_MARKER);
        buf
    }

    pub fn inject_into_content(&self, existing: &str, rules: &[EpisodicRule]) -> Result<(String, bool), DiffmemError> {
        let new_block = self.format_episodic_section(rules);

        let start_idx = existing.find(AGENTS_BEGIN_MARKER);
        let end_idx = existing.find(AGENTS_END_MARKER);

        match (start_idx, end_idx) {
            (Some(start), Some(end)) => {
                if end < start {
                    return Err(DiffmemError::MarkerCorrupted(
                        "END marker appears before BEGIN marker in AGENTS.md".into(),
                    ));
                }
                let end_after = end + AGENTS_END_MARKER.len();
                let mut updated = String::with_capacity(existing.len() + new_block.len());
                updated.push_str(&existing[..start]);
                updated.push_str(&new_block);
                updated.push_str(&existing[end_after..]);
                let changed = updated != existing;
                Ok((updated, changed))
            }
            (None, None) => {
                let mut updated = existing.to_string();
                if !updated.is_empty() && !updated.ends_with("\n\n") {
                    if updated.ends_with('\n') {
                        updated.push('\n');
                    } else {
                        updated.push_str("\n\n");
                    }
                }
                updated.push_str(&new_block);
                updated.push('\n');
                Ok((updated, true))
            }
            (Some(_), None) => Err(DiffmemError::MarkerCorrupted(
                "Found BEGIN marker without matching END marker in AGENTS.md".into(),
            )),
            (None, Some(_)) => Err(DiffmemError::MarkerCorrupted(
                "Found END marker without matching BEGIN marker in AGENTS.md".into(),
            )),
        }
    }
}

impl SyncAdapter for AgentsMdAdapter {
    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn sync(&self, workspace: &Path, rules: &[EpisodicRule], dry_run: bool) -> Result<SyncReport, DiffmemError> {
        let target_path = workspace.join(self.file_name);
        let exists = target_path.exists();

        let (content, status) = if exists {
            let existing_text = fs::read_to_string(&target_path)?;
            let (updated_text, changed) = self.inject_into_content(&existing_text, rules)?;
            let status = if changed {
                SyncStatus::Updated
            } else {
                SyncStatus::Unchanged
            };
            (updated_text, status)
        } else {
            let mut init_text = String::from("# AGENTS.md\n\n");
            init_text.push_str(&self.format_episodic_section(rules));
            init_text.push('\n');
            (init_text, SyncStatus::Created)
        };

        if !dry_run && status != SyncStatus::Unchanged {
            fs::write(&target_path, content)?;
        }

        Ok(SyncReport {
            adapter_name: self.name(),
            target_path,
            rules_written: rules.len(),
            status,
        })
    }

    fn clean(&self, workspace: &Path) -> Result<(), DiffmemError> {
        let target_path = workspace.join(self.file_name);
        if target_path.exists() {
            let existing_text = fs::read_to_string(&target_path)?;
            let (updated_text, changed) = self.inject_into_content(&existing_text, &[])?;
            if changed {
                fs::write(&target_path, updated_text)?;
            }
        }
        Ok(())
    }
}
