mod common;

use common::TestWorkspace;
use diffmem::adapters::{AdapterRegistry, SyncTarget};
use diffmem::correlator::correlate_diffs;
use diffmem::store::RuleStore;
use std::path::Path;

#[test]
fn test_reverted_draft_to_resolving_commit_correlation() {
    let ws = TestWorkspace::new();

    // 1. Setup baseline source code
    let file_path = "src/payments/calc.rs";
    let baseline_content = r#"pub fn calculate_fee(amount: u64) -> u64 {
    amount / 10
}
"#;
    ws.write_file(file_path, baseline_content);
    ws.git(&["add", "."]);
    ws.git(&["commit", "-m", "chore: initial baseline payment fee"]);

    // 2. Simulate Agent introducing broken/rejected implementation (Mistake)
    let rejected_draft = r#"pub fn calculate_fee(amount: u64) -> u64 {
    // Mistake: floating-point math causes precision loss
    (amount as f64 * 0.05) as u64
}
"#;
    ws.write_file(file_path, rejected_draft);

    // Capture rejected diff against HEAD
    let rejected_diff_output = ws.git(&["diff", "HEAD"]);
    let rejected_diff = String::from_utf8_lossy(&rejected_diff_output.stdout).to_string();
    assert!(!rejected_diff.is_empty(), "rejected diff must not be empty");

    // 3. User or test reverts the broken draft back to HEAD
    ws.git(&["checkout", "--", "."]);
    assert_eq!(
        ws.read_file(file_path).replace("\r\n", "\n"),
        baseline_content.replace("\r\n", "\n")
    );

    // 4. Simulate Resolving Commit (Fix)
    let accepted_fix = r#"pub fn calculate_fee(amount: u64) -> u64 {
    // Fixed: integer basis point arithmetic (500 bps = 5.00%)
    amount.saturating_mul(500) / 10_000
}
"#;
    ws.write_file(file_path, accepted_fix);
    ws.git(&["add", "."]);
    ws.git(&["commit", "-m", "fix(payments): use integer bps instead of f64"]);

    // Capture accepted commit diff
    let accepted_diff_output = ws.git(&["diff", "HEAD~1", "HEAD"]);
    let accepted_diff = String::from_utf8_lossy(&accepted_diff_output.stdout).to_string();
    assert!(!accepted_diff.is_empty(), "accepted diff must not be empty");

    // 5. Correlate rejected vs accepted diffs
    let rule = correlate_diffs(
        &rejected_diff,
        &accepted_diff,
        Some("src/payments/**".to_string()),
        Some("Agent used f64 floating point math for fee calculation".to_string()),
        Some("Use integer basis points calculation".to_string()),
    )
    .expect("correlation must succeed");

    assert_eq!(rule.path_glob, "src/payments/**");
    assert!(rule.active);
    assert!(rule.hunk_rejected.contains("f64"));
    assert!(rule.hunk_accepted.contains("saturating_mul"));

    // 6. Store and verify persistence
    let mut store = RuleStore::load_or_create(ws.path()).expect("store load/create");
    store.add_rule(rule).expect("add rule");
    store.save().expect("store save");

    let loaded_store = RuleStore::load_or_create(ws.path()).expect("reload store");
    assert_eq!(loaded_store.all_rules().len(), 1);

    // 7. Test path lookup (diffmem blame equivalent)
    let matches = loaded_store.find_rules_for_path(Path::new("src/payments/calc.rs"));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].path_glob, "src/payments/**");

    // 8. Test editor sync
    let registry = AdapterRegistry::new();
    let reports = registry
        .sync(SyncTarget::All, ws.path(), loaded_store.all_rules(), false)
        .expect("sync all");
    assert_eq!(reports.len(), 3);

    // Verify CLAUDE.md got updated
    let claude_md = ws.read_file("CLAUDE.md");
    assert!(claude_md.contains("<!-- BEGIN DIFFMEM RULES -->"));
    assert!(claude_md.contains("src/payments/**"));
    assert!(claude_md.contains("Agent used f64 floating point math"));

    // Verify Cursor got updated
    let cursor_rule_file = format!(".cursor/rules/diffmem_{}.mdc", loaded_store.all_rules()[0].id);
    assert!(ws.file_exists(&cursor_rule_file));

    // Verify AGENTS.md got updated
    let agents_md = ws.read_file("AGENTS.md");
    assert!(agents_md.contains("<!-- BEGIN DIFFMEM EPISODIC RULES -->"));
    assert!(agents_md.contains("src/payments/**"));
}
