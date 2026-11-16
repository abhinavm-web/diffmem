use std::collections::HashSet;
use std::fs;
use std::path::Path;
use crate::adapters::{SyncAdapter, SyncReport, SyncStatus};
use crate::error::DiffmemError;
use crate::store::rule::EpisodicRule;

#[derive(Debug, Clone)]
pub struct CursorAdapter {
    pub rules_dir: &'static str,
}

impl Default for CursorAdapter {
    fn default() -> Self {
        Self {
            rules_dir: ".cursor/rules",
        }
    }
}

impl CursorAdapter {
    pub fn sanitize_filename(id: &str) -> String {
        id.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect()
    }

    pub fn format_mdc_rule(rule: &EpisodicRule) -> String {
        let mut buf = String::new();
        // YAML Frontmatter
        buf.push_str("---\n");
        buf.push_str(&format!("description: \"[diffmem] {}\"\n", rule.mistake.replace('\"', "\\\"")));
        buf.push_str(&format!("globs: \"{}\"\n", rule.path_glob.replace('\\', "/")));
        buf.push_str("alwaysApply: false\n");
        buf.push_str("---\n\n");

        // Markdown Body
        buf.push_str(&format!("# Rule {}: {}\n\n", rule.id, rule.path_glob));
        buf.push_str(&format!("- **Synthesized**: {}\n", rule.timestamp.to_rfc3339()));
        buf.push_str(&format!("- **Path Scope**: `{}`\n\n", rule.path_glob));

        buf.push_str("## Mistake (Rejected Attempt)\n");
        buf.push_str(&rule.mistake);
        buf.push_str("\n\n");

        if !rule.hunk_rejected.trim().is_empty() {
            buf.push_str("```diff\n");
            buf.push_str(&rule.hunk_rejected);
            if !rule.hunk_rejected.ends_with('\n') {
                buf.push('\n');
            }
            buf.push_str("```\n\n");
        }

        buf.push_str("## Fix (Committed Resolution)\n");
        buf.push_str(&rule.fix);
        buf.push_str("\n\n");

        if !rule.hunk_accepted.trim().is_empty() {
            buf.push_str("```diff\n");
            buf.push_str(&rule.hunk_accepted);
            if !rule.hunk_accepted.ends_with('\n') {
                buf.push('\n');
            }
            buf.push_str("```\n");
        }

        buf
    }
}

impl SyncAdapter for CursorAdapter {
    fn name(&self) -> &'static str {
        "cursor"
    }

    fn sync(&self, workspace: &Path, rules: &[EpisodicRule], dry_run: bool) -> Result<SyncReport, DiffmemError> {
        let target_dir = workspace.join(self.rules_dir);
        if !dry_run && !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
        }

        let mut active_filenames = HashSet::new();
        let mut modified = false;

        for rule in rules {
            let filename = format!("diffmem_{}.mdc", Self::sanitize_filename(&rule.id));
            let file_path = target_dir.join(&filename);
            let content = Self::format_mdc_rule(rule);

            let changed = if file_path.exists() {
                let existing = fs::read_to_string(&file_path)?;
                existing != content
            } else {
                true
            };

            if changed {
                modified = true;
                if !dry_run {
                    fs::write(&file_path, content)?;
                }
            }
            active_filenames.insert(filename);
        }

        // Pruning: remove old diffmem_*.mdc files not in active_filenames
        if target_dir.exists() {
            for entry in fs::read_dir(&target_dir)? {
                let entry = entry?;
                let path = entry.path();
                if let Some(file_name_str) = path.file_name().and_then(|s| s.to_str()) {
                    if file_name_str.starts_with("diffmem_")
                        && file_name_str.ends_with(".mdc")
                        && !active_filenames.contains(file_name_str)
                    {
                        modified = true;
                        if !dry_run {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        let status = if modified {
            SyncStatus::Updated
        } else {
            SyncStatus::Unchanged
        };

        Ok(SyncReport {
            adapter_name: self.name(),
            target_path: target_dir,
            rules_written: rules.len(),
            status,
        })
    }

    fn clean(&self, workspace: &Path) -> Result<(), DiffmemError> {
        let target_dir = workspace.join(self.rules_dir);
        if target_dir.exists() {
            for entry in fs::read_dir(&target_dir)? {
                let entry = entry?;
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("diffmem_") && name.ends_with(".mdc") {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
        Ok(())
    }
}
