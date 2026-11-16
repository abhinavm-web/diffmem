#![deny(clippy::all)]

use diffmem::{AdapterRegistry, Correlator, RuleStore, SyncTarget};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== diffmem: Zero-Touch Diff-of-Diffs Episodic Memory Showcase ===");

    let rejected_agent_diff = "\
--- a/src/payments/invoice.rs
+++ b/src/payments/invoice.rs
@@ -10,3 +10,4 @@
 pub fn compute_total(subtotal: f64, tax_rate: f64) -> f64 {
-    subtotal
+    let total: f64 = subtotal * (1.0 + tax_rate);
+    total
 }
";

    let accepted_commit_diff = "\
--- a/src/payments/invoice.rs
+++ b/src/payments/invoice.rs
@@ -10,3 +10,4 @@
 pub fn compute_total_cents(subtotal_cents: i64, tax_bps: i64) -> i64 {
-    subtotal_cents
+    subtotal_cents + (subtotal_cents * tax_bps) / 10_000
 }
";

    let correlator = Correlator::new(2);
    let rules = correlator.correlate(rejected_agent_diff, accepted_commit_diff);

    let tmp = tempfile::tempdir()?;
    let mut store = RuleStore::load_or_create(tmp.path())?;
    for rule in &rules {
        println!("Synthesized Rule ID: {}", rule.id);
        println!("  Path Glob: {}", rule.path_glob);
        println!("  Mistake:   {}", rule.mistake);
        println!("  Fix:       {}", rule.fix);
        store.add_rule(rule.clone())?;
    }
    store.save()?;

    let registry = AdapterRegistry::new();
    let reports = registry.sync(SyncTarget::All, tmp.path(), store.all_rules(), false)?;
    for rep in reports {
        println!(
            "Synced {} rule(s) -> {} ({:?})",
            rep.rules_written,
            rep.target_path.display(),
            rep.status
        );
    }

    Ok(())
}
