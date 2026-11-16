pub mod distill;
pub mod hunk;

use crate::correlator::distill::{distill_rule, format_snippet};
use crate::correlator::hunk::{FilePatch, Hunk, LineRange, UnifiedDiffParser};
use crate::error::DiffmemError;
use crate::store::rule::EpisodicRule;

#[derive(Debug, Clone)]
pub struct CorrelatedHunkPair {
    pub file_path: String,
    pub rejected_hunk: Hunk,
    pub accepted_hunk: Hunk,
    pub overlap_range: LineRange,
}

pub struct Correlator {
    pub tolerance_lines: usize,
}

impl Default for Correlator {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Correlator {
    pub fn new(tolerance_lines: usize) -> Self {
        Self { tolerance_lines }
    }

    pub fn correlate(&self, rejected_diff: &str, accepted_diff: &str) -> Vec<EpisodicRule> {
        let rej_patches = UnifiedDiffParser::parse(rejected_diff);
        let acc_patches = UnifiedDiffParser::parse(accepted_diff);
        let pairs = self.match_hunks(&rej_patches, &acc_patches);

        pairs.into_iter().filter_map(|p| distill_rule(&p)).collect()
    }

    pub fn match_hunks(&self, rejected: &[FilePatch], accepted: &[FilePatch]) -> Vec<CorrelatedHunkPair> {
        let mut pairs = Vec::new();

        for rej_patch in rejected {
            let acc_patch = accepted.iter().find(|p| {
                p.new_path == rej_patch.new_path
                    || (rej_patch.old_path.is_some() && rej_patch.old_path == p.old_path)
            });
            let Some(acc_patch) = acc_patch else {
                continue;
            };

            for rej_hunk in &rej_patch.hunks {
                for acc_hunk in &acc_patch.hunks {
                    if rej_hunk.old_range.overlaps(&acc_hunk.old_range, self.tolerance_lines)
                        && self.is_divergent(rej_hunk, acc_hunk)
                    {
                        let start = rej_hunk.old_range.start.max(acc_hunk.old_range.start);
                        let end = rej_hunk.old_range.end().min(acc_hunk.old_range.end());
                        let count = end.saturating_sub(start);

                        pairs.push(CorrelatedHunkPair {
                            file_path: rej_patch.new_path.clone(),
                            rejected_hunk: rej_hunk.clone(),
                            accepted_hunk: acc_hunk.clone(),
                            overlap_range: LineRange { start, count },
                        });
                    }
                }
            }
        }

        pairs
    }

    pub fn is_divergent(&self, rej: &Hunk, acc: &Hunk) -> bool {
        let rej_adds = rej.added_content();
        let acc_adds = acc.added_content();

        if rej_adds == acc_adds && rej.deleted_content() == acc.deleted_content() {
            return false;
        }

        rej_adds != acc_adds || (!rej_adds.is_empty() && acc_adds.is_empty())
    }
}

pub fn correlate_diffs(
    rejected_diff: &str,
    accepted_diff: &str,
    glob_override: Option<String>,
    mistake_override: Option<String>,
    fix_override: Option<String>,
) -> Result<EpisodicRule, DiffmemError> {
    let correlator = Correlator::new(2);
    let correlated = correlator.correlate(rejected_diff, accepted_diff);

    if let Some(mut rule) = correlated.into_iter().next() {
        if let Some(glob) = glob_override {
            rule.path_glob = glob.replace('\\', "/");
        }
        if let Some(m) = mistake_override {
            rule.mistake = m;
        }
        if let Some(f) = fix_override {
            rule.fix = f;
        }
        rule.id = EpisodicRule::generate_id(
            &rule.path_glob,
            &rule.mistake,
            &rule.fix,
            &rule.hunk_rejected,
        );
        return Ok(rule);
    }

    // Fallback: extract patches directly from both diffs
    let rej_patches = UnifiedDiffParser::parse(rejected_diff);
    let acc_patches = UnifiedDiffParser::parse(accepted_diff);

    let file_path = rej_patches
        .first()
        .map(|p| p.new_path.as_str())
        .or_else(|| acc_patches.first().map(|p| p.new_path.as_str()))
        .unwrap_or("unknown");

    let path_glob = glob_override.unwrap_or_else(|| distill::derive_path_glob(file_path));

    let rej_adds: Vec<&str> = rej_patches
        .iter()
        .flat_map(|p| p.hunks.iter().flat_map(|h| h.added_content()))
        .collect();
    let acc_adds: Vec<&str> = acc_patches
        .iter()
        .flat_map(|p| p.hunks.iter().flat_map(|h| h.added_content()))
        .collect();

    let heading = rej_patches
        .first()
        .and_then(|p| p.hunks.first().map(|h| h.heading.as_str()))
        .unwrap_or("");

    let rej_text = rej_adds.join("\n");
    let acc_text = acc_adds.join("\n");

    let (auto_mistake, auto_fix) = distill::extract_semantics(heading, &rej_text, &acc_text);
    let mistake = mistake_override.unwrap_or(auto_mistake);
    let fix = fix_override.unwrap_or(auto_fix);

    let hunk_rejected = if !rej_adds.is_empty() {
        format_snippet(&rej_adds)
    } else {
        rejected_diff
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n")
    };

    let hunk_accepted = if !acc_adds.is_empty() {
        format_snippet(&acc_adds)
    } else {
        accepted_diff
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok(EpisodicRule::new(
        path_glob,
        mistake,
        fix,
        hunk_rejected,
        hunk_accepted,
    ))
}
