#![deny(clippy::all)]

use diffmem::{Correlator, RuleStore};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== diffmem 20-Commit Signal Quality & Recurrence Benchmark ===");

    let tmp = tempfile::tempdir()?;
    let mut store = RuleStore::load_or_create(tmp.path())?;
    let correlator = Correlator::new(2);

    // 12 non-overlapping exploratory commits (base changed cleanly without reverting overlapping draft hunks)
    let mut exploratory_rejected_noise = 0usize;
    let start = Instant::now();

    for i in 0..12 {
        let draft = format!(
            "--- a/src/feature_{i}.rs\n+++ b/src/feature_{i}.rs\n@@ -1,2 +1,2 @@\n-fn step_a() {{}}\n+fn step_a() {{ /* draft */ }}\n"
        );
        let final_commit = format!(
            "--- a/src/feature_{i}.rs\n+++ b/src/feature_{i}.rs\n@@ -25,2 +25,2 @@\n-fn helper() {{}}\n+fn helper() {{ /* unrelated addition */ }}\n"
        );
        let rules = correlator.correlate(&draft, &final_commit);
        if rules.is_empty() {
            exploratory_rejected_noise += 1;
        }
    }

    // 8 recurring mistake -> fix episodes (4 distinct bug classes repeated 2x each across sessions)
    let mistake_pairs = [
        (
            "--- a/src/payments/calc.rs\n+++ b/src/payments/calc.rs\n@@ -10,2 +10,2 @@\n-let fee = 0;\n+let fee = (amount as f64 * 0.05) as u64;\n",
            "--- a/src/payments/calc.rs\n+++ b/src/payments/calc.rs\n@@ -10,2 +10,2 @@\n-let fee = 0;\n+let fee = amount.checked_mul(500).unwrap_or(0) / 10_000;\n",
        ),
        (
            "--- a/src/db/users.rs\n+++ b/src/db/users.rs\n@@ -4,2 +4,2 @@\n-let q = \"\";\n+let q = format!(\"SELECT * FROM users WHERE id = {}\", uid);\n",
            "--- a/src/db/users.rs\n+++ b/src/db/users.rs\n@@ -4,2 +4,2 @@\n-let q = \"\";\n+let q = sqlx::query_as(\"SELECT * FROM users WHERE id = $1\").bind(uid);\n",
        ),
        (
            "--- a/src/api/handler.rs\n+++ b/src/api/handler.rs\n@@ -8,2 +8,2 @@\n-let v = None;\n+let v = req.header.unwrap();\n",
            "--- a/src/api/handler.rs\n+++ b/src/api/handler.rs\n@@ -8,2 +8,2 @@\n-let v = None;\n+let v = req.header.map_err(ApiErr::Missing)?;\n",
        ),
        (
            "--- a/src/jobs/worker.rs\n+++ b/src/jobs/worker.rs\n@@ -12,2 +12,2 @@\n-wait();\n+std::thread::sleep(dur);\n",
            "--- a/src/jobs/worker.rs\n+++ b/src/jobs/worker.rs\n@@ -12,2 +12,2 @@\n-wait();\n+tokio::time::sleep(dur).await;\n",
        ),
    ];

    for _round in 0..2 {
        for (rej, acc) in &mistake_pairs {
            for r in correlator.correlate(rej, acc) {
                store.add_rule(r)?;
            }
        }
    }
    store.save()?;

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let rules = store.all_rules();
    let recurring_rules = rules.iter().filter(|r| r.occurrences >= 2).count();

    println!("Non-overlapping exploratory diffs ignored: {}/12 (100% noise filtered)", exploratory_rejected_noise);
    println!("Unique rules synthesized across 8 mistakes: {}", rules.len());
    println!("Rules promoted with >=2x recurrence:       {}/{}", recurring_rules, rules.len());
    println!("Total correlation + persistence latency:   {:.2} ms", elapsed_ms);
    for r in rules {
        println!(
            "  [{} | {}x | {}] {} -> {}",
            r.id, r.occurrences, r.path_glob, r.mistake, r.fix
        );
    }

    assert_eq!(exploratory_rejected_noise, 12);
    assert_eq!(recurring_rules, 4);
    Ok(())
}
