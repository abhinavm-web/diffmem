mod common;

use chrono::Utc;
use common::TestWorkspace;
use diffmem::adapters::{AdapterRegistry, SyncTarget};
use diffmem::store::rule::EpisodicRule;

fn sample_rule(id: &str, glob: &str, mistake: &str, fix: &str) -> EpisodicRule {
    EpisodicRule {
        id: id.to_string(),
        timestamp: Utc::now(),
        path_glob: glob.to_string(),
        mistake: mistake.to_string(),
        fix: fix.to_string(),
        hunk_rejected: "- rejected_code();\n".to_string(),
        hunk_accepted: "+ fixed_code();\n".to_string(),
        active: true,
        occurrences: 1,
    }
}

#[test]
fn test_claude_adapter_surgical_preservation_and_idempotency() {
    let ws = TestWorkspace::new();
    let user_custom_text = "# Custom Instructions\n- Be concise\n- Follow TDD\n";
    ws.write_file("CLAUDE.md", user_custom_text);

    let rules = vec![sample_rule("mem-1", "src/auth/**", "Plaintext tokens", "Use bcrypt")];
    let registry = AdapterRegistry::new();

    // First sync
    registry
        .sync(SyncTarget::Claude, ws.path(), &rules, false)
        .expect("sync 1");
    let content1 = ws.read_file("CLAUDE.md");

    assert!(
        content1.starts_with(user_custom_text),
        "Pre-existing content must be untouched"
    );
    assert!(content1.contains("<!-- BEGIN DIFFMEM RULES -->"));
    assert!(content1.contains("src/auth/**"));
    assert!(content1.contains("Plaintext tokens"));
    assert!(content1.contains("<!-- END DIFFMEM RULES -->"));

    // Second sync (idempotency check)
    registry
        .sync(SyncTarget::Claude, ws.path(), &rules, false)
        .expect("sync 2");
    let content2 = ws.read_file("CLAUDE.md");
    assert_eq!(content1, content2, "Repeated sync must be strictly idempotent");

    // Sync with empty active rules (cleans markers without deleting file)
    registry
        .sync(SyncTarget::Claude, ws.path(), &[], false)
        .expect("sync empty");
    let content3 = ws.read_file("CLAUDE.md");
    assert!(content3.starts_with(user_custom_text));
    assert!(content3.contains("<!-- diffmem: no active episodic rules -->"));
    assert!(!content3.contains("Plaintext tokens"));
}

#[test]
fn test_cursor_adapter_creation_and_pruning() {
    let ws = TestWorkspace::new();
    let rule1 = sample_rule("mem-101", "src/db/**", "N+1 query loop", "Use join fetch");
    let rule2 = sample_rule("mem-102", "src/api/**", "Missing rate limiter", "Add middleware");
    let registry = AdapterRegistry::new();

    // Sync 2 rules
    registry
        .sync(SyncTarget::Cursor, ws.path(), &[rule1, rule2.clone()], false)
        .expect("sync 2 rules");
    assert!(ws.file_exists(".cursor/rules/diffmem_mem-101.mdc"));
    assert!(ws.file_exists(".cursor/rules/diffmem_mem-102.mdc"));

    let mdc1 = ws.read_file(".cursor/rules/diffmem_mem-101.mdc");
    assert!(mdc1.contains("globs: \"src/db/**\""));
    assert!(mdc1.contains("description: \"[diffmem] N+1 query loop\""));

    // Sync with only rule2 (rule1 is deactivated/pruned)
    registry
        .sync(SyncTarget::Cursor, ws.path(), &[rule2], false)
        .expect("sync prune");
    assert!(
        !ws.file_exists(".cursor/rules/diffmem_mem-101.mdc"),
        "Deactivated rule mdc must be pruned"
    );
    assert!(
        ws.file_exists(".cursor/rules/diffmem_mem-102.mdc"),
        "Active rule mdc must remain"
    );
}

#[test]
fn test_antigravity_codex_adapter_preserves_behavior_guidelines() {
    let ws = TestWorkspace::new();
    let existing_agents_md = r#"<!-- BEGIN behavior-guidelines -->
# AGENTS.md
## 1. Think Before Coding
Don't assume.
## 2. Simplicity First
Minimum code.
<!-- END behavior-guidelines -->
"#;
    ws.write_file("AGENTS.md", existing_agents_md);

    let rules = vec![sample_rule("mem-201", "infra/**", "Hardcoded port", "Use env var")];
    let registry = AdapterRegistry::new();

    // Sync to antigravity
    registry
        .sync(SyncTarget::Antigravity, ws.path(), &rules, false)
        .expect("sync antigravity");
    let updated_agents = ws.read_file("AGENTS.md");

    assert!(updated_agents.contains("<!-- BEGIN behavior-guidelines -->"));
    assert!(updated_agents.contains("<!-- END behavior-guidelines -->"));
    assert!(updated_agents.contains("## 1. Think Before Coding"));
    assert!(updated_agents.contains("<!-- BEGIN DIFFMEM EPISODIC RULES -->"));
    assert!(updated_agents.contains("infra/**"));
    assert!(updated_agents.contains("Hardcoded port"));
    assert!(updated_agents.contains("<!-- END DIFFMEM EPISODIC RULES -->"));
}
