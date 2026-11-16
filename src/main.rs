#![deny(clippy::all)]

use std::path::{Path, PathBuf};
use clap::{Parser, Subcommand, ValueEnum};
use diffmem::adapters::{AdapterRegistry, SyncTarget};
use diffmem::config::Config;
use diffmem::error::DiffmemError;
use diffmem::git::{GitCli, GitOps};
use diffmem::store::RuleStore;
use diffmem::watcher::{SnapshotState, SnapshotStore};

#[derive(Parser, Debug)]
#[command(
    name = "diffmem",
    about = "Zero-touch episodic failure memory for coding agents",
    version,
    long_about = "Observes git activity to correlate rejected or reverted agent diffs against final commit changes, synthesizing path-scoped episodic memory rules across Claude Code, Cursor, and Antigravity/Codex."
)]
pub struct Cli {
    #[arg(short, long, global = true, help = "Path to repository root (defaults to current dir)")]
    pub repo: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize diffmem and install git post-commit hooks
    Init {
        #[arg(long, default_value_t = true, help = "Install git post-commit hook")]
        hook: bool,
        #[arg(long, help = "Overwrite existing configuration")]
        force: bool,
    },

    /// Run the zero-touch git and filesystem watcher daemon
    Watch {
        #[arg(short, long, default_value_t = 1.0, help = "Polling interval in seconds")]
        interval: f64,
        #[arg(long, help = "Run in background daemon mode")]
        daemon: bool,
    },

    /// List recorded episodic failure rules
    Log {
        #[arg(short, long, help = "Include reverted/inactive rules")]
        all: bool,
        #[arg(long, help = "Output as raw JSON")]
        json: bool,
        #[arg(short, long, help = "Filter by path glob pattern")]
        glob: Option<String>,
    },

    /// Show rules applying to a specific file or path
    Blame {
        #[arg(help = "Path to inspect")]
        path: PathBuf,
        #[arg(long, help = "Output full details as JSON")]
        json: bool,
    },

    /// Revert/deactivate an episodic rule by ID
    Revert {
        #[arg(help = "Rule ID to revert (e.g. mem-a19f)")]
        id: String,
        #[arg(long, help = "Permanently delete rule instead of deactivating")]
        delete: bool,
        #[arg(long, help = "Skip editor synchronization")]
        no_sync: bool,
    },

    /// Synchronize rules to editor files (CLAUDE.md, Cursor rules, AGENTS.md)
    Sync {
        #[arg(long, value_enum, default_value_t = SyncTargetCli::All, help = "Target editor adapter")]
        target: SyncTargetCli,
        #[arg(long, help = "Preview changes without modifying files")]
        dry_run: bool,
    },

    /// Manually or hook-record a pair of rejected and accepted diffs
    Record {
        #[arg(long, help = "File containing the rejected diff")]
        rejected: Option<PathBuf>,
        #[arg(long, help = "File containing the accepted commit diff")]
        accepted: Option<PathBuf>,
        #[arg(long, help = "Explicit path glob override")]
        glob: Option<String>,
        #[arg(long, help = "Explicit mistake summary")]
        mistake: Option<String>,
        #[arg(long, help = "Explicit fix summary")]
        fix: Option<String>,
        #[arg(long, help = "Skip editor synchronization")]
        no_sync: bool,
        #[arg(long, help = "Triggered by post-commit git hook")]
        post_commit: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SyncTargetCli {
    All,
    Claude,
    Cursor,
    Antigravity,
}

impl From<SyncTargetCli> for SyncTarget {
    fn from(val: SyncTargetCli) -> Self {
        match val {
            SyncTargetCli::All => SyncTarget::All,
            SyncTargetCli::Claude => SyncTarget::Claude,
            SyncTargetCli::Cursor => SyncTarget::Cursor,
            SyncTargetCli::Antigravity => SyncTarget::Antigravity,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let repo_root = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match cli.command {
        Commands::Init { hook, force } => {
            cmd_init(&repo_root, hook, force)?;
        }
        Commands::Watch { interval, daemon } => {
            cmd_watch(&repo_root, interval, daemon)?;
        }
        Commands::Log { all, json, glob } => {
            cmd_log(&repo_root, all, json, glob)?;
        }
        Commands::Blame { path, json } => {
            cmd_blame(&repo_root, &path, json)?;
        }
        Commands::Revert { id, delete, no_sync } => {
            cmd_revert(&repo_root, &id, delete, no_sync)?;
        }
        Commands::Sync { target, dry_run } => {
            cmd_sync(&repo_root, target.into(), dry_run)?;
        }
        Commands::Record {
            rejected,
            accepted,
            glob,
            mistake,
            fix,
            no_sync,
            post_commit,
        } => {
            cmd_record(RecordOpts {
                repo: &repo_root,
                rejected: rejected.as_deref(),
                accepted: accepted.as_deref(),
                glob,
                mistake,
                fix,
                no_sync,
                post_commit,
            })?;
        }
    }

    Ok(())
}

fn cmd_init(repo: &Path, hook: bool, force: bool) -> Result<(), DiffmemError> {
    let diffmem_dir = repo.join(".diffmem");
    if diffmem_dir.exists() && !force {
        println!("diffmem is already initialized in {}", diffmem_dir.display());
        return Ok(());
    }

    std::fs::create_dir_all(diffmem_dir.join("snapshots"))?;
    let config = Config::default();
    config.save(repo)?;

    let store = RuleStore::load_or_create(repo)?;
    store.save()?;

    if hook {
        diffmem::git::install_post_commit_hook(repo)?;
        println!("Installed git post-commit hook: .git/hooks/post-commit");
    }

    println!("Initialized diffmem in {}", diffmem_dir.display());
    Ok(())
}

fn cmd_log(repo: &Path, all: bool, json: bool, glob_filter: Option<String>) -> Result<(), DiffmemError> {
    let store = RuleStore::load_or_create(repo)?;
    let rules = store.list_rules(all, glob_filter.as_deref());

    if json {
        let serialized = serde_json::to_string_pretty(&rules)?;
        println!("{}", serialized);
        return Ok(());
    }

    if rules.is_empty() {
        println!("No episodic rules found.");
        return Ok(());
    }

    println!("{:<10} {:<8} {:<24} {:<32} FIX", "ID", "STATUS", "PATH GLOB", "MISTAKE");
    println!("{}", "-".repeat(100));

    for rule in rules {
        let status = if rule.active { "ACTIVE" } else { "REVERTED" };
        let mistake_trunc = truncate(&rule.mistake, 30);
        let fix_trunc = truncate(&rule.fix, 30);
        println!("{:<10} {:<8} {:<24} {:<32} {}", rule.id, status, rule.path_glob, mistake_trunc, fix_trunc);
    }

    Ok(())
}

fn cmd_blame(repo: &Path, path: &Path, json: bool) -> Result<(), DiffmemError> {
    let store = RuleStore::load_or_create(repo)?;
    let rules = store.find_rules_for_path(path);

    if json {
        let serialized = serde_json::to_string_pretty(&rules)?;
        println!("{}", serialized);
        return Ok(());
    }

    if rules.is_empty() {
        println!("No episodic failure rules matching '{}'.", path.display());
        return Ok(());
    }

    println!("Rules matching '{}':\n", path.display());
    for rule in rules {
        println!("[{}] {} (Created: {})", rule.id, if rule.active { "Active" } else { "Reverted" }, rule.timestamp.to_rfc3339());
        println!("  Glob:    {}", rule.path_glob);
        println!("  Mistake: {}", rule.mistake);
        println!("  Fix:     {}", rule.fix);
        if !rule.hunk_rejected.trim().is_empty() {
            println!("  Rejected Hunk:\n{}", indent(&rule.hunk_rejected, 4));
        }
        if !rule.hunk_accepted.trim().is_empty() {
            println!("  Accepted Hunk:\n{}", indent(&rule.hunk_accepted, 4));
        }
        println!();
    }

    Ok(())
}

fn cmd_revert(repo: &Path, id: &str, delete: bool, no_sync: bool) -> Result<(), DiffmemError> {
    let mut store = RuleStore::load_or_create(repo)?;
    if delete {
        store.delete_rule(id)?;
        println!("Deleted rule {}", id);
    } else {
        store.deactivate_rule(id)?;
        println!("Reverted rule {} (marked inactive)", id);
    }
    store.save()?;

    if !no_sync {
        let registry = AdapterRegistry::new();
        let reports = registry.sync(SyncTarget::All, repo, store.all_rules(), false)?;
        println!("Synchronized {} editor target(s).", reports.len());
    }

    Ok(())
}

fn cmd_sync(repo: &Path, target: SyncTarget, dry_run: bool) -> Result<(), DiffmemError> {
    let store = RuleStore::load_or_create(repo)?;
    let registry = AdapterRegistry::new();
    let reports = registry.sync(target, repo, store.all_rules(), dry_run)?;

    if dry_run {
        println!("Dry run sync preview:");
    } else {
        println!("Synchronized diffmem rules:");
    }

    for report in reports {
        println!("  - {}: {:?} ({} rules written to {})",
            report.adapter_name,
            report.status,
            report.rules_written,
            report.target_path.display()
        );
    }

    Ok(())
}

struct RecordOpts<'a> {
    repo: &'a Path,
    rejected: Option<&'a Path>,
    accepted: Option<&'a Path>,
    glob: Option<String>,
    mistake: Option<String>,
    fix: Option<String>,
    no_sync: bool,
    post_commit: bool,
}

fn cmd_record(opts: RecordOpts<'_>) -> Result<(), DiffmemError> {
    let repo = opts.repo;
    if opts.post_commit {
        let git = GitCli::new(repo);
        let head = git.get_head_commit()?;
        let commit_diff = git.get_commit_diff(&head)?;

        if commit_diff.trim().is_empty() {
            return Ok(());
        }

        let snapshot_store = SnapshotStore::new(repo)?;
        let candidate_snapshots = snapshot_store.list_all()?;
        let correlator = diffmem::correlator::Correlator::new(2);
        let mut store = RuleStore::load_or_create(repo)?;
        let mut created_count = 0;

        for mut snap in candidate_snapshots {
            if matches!(snap.state, SnapshotState::Correlated { .. }) {
                continue;
            }

            let rules = correlator.correlate(&snap.unified_diff, &commit_diff);
            for r in rules {
                let rule_id = r.id.clone();
                if store.add_rule(r)? {
                    created_count += 1;
                }
                snap.state = SnapshotState::Correlated {
                    rule_id,
                    committed_at: chrono::Utc::now(),
                };
                let _ = snapshot_store.save(&snap);
            }
        }

        if created_count > 0 {
            store.save()?;
            if !opts.no_sync {
                let registry = AdapterRegistry::new();
                let _ = registry.sync(SyncTarget::All, repo, store.all_rules(), false);
            }
            println!("Post-commit recorded {} new episodic rule(s).", created_count);
        }

        return Ok(());
    }

    let Some(rej_path) = opts.rejected else {
        return Err(DiffmemError::General("Missing --rejected diff path".to_string()));
    };
    let Some(acc_path) = opts.accepted else {
        return Err(DiffmemError::General("Missing --accepted diff path".to_string()));
    };

    let rejected_diff = std::fs::read_to_string(rej_path)?;
    let accepted_diff = std::fs::read_to_string(acc_path)?;

    let rule = diffmem::correlator::correlate_diffs(
        &rejected_diff,
        &accepted_diff,
        opts.glob,
        opts.mistake,
        opts.fix,
    )?;

    let mut store = RuleStore::load_or_create(repo)?;
    let rule_id = rule.id.clone();
    let rule_glob = rule.path_glob.clone();
    store.add_rule(rule)?;
    store.save()?;

    println!("Recorded episodic rule {} for glob '{}'", rule_id, rule_glob);

    if !opts.no_sync {
        let registry = AdapterRegistry::new();
        registry.sync(SyncTarget::All, repo, store.all_rules(), false)?;
        println!("Synchronized editor targets.");
    }

    Ok(())
}

fn cmd_watch(repo: &Path, interval: f64, daemon: bool) -> Result<(), DiffmemError> {
    diffmem::watcher::run_watcher(repo, interval, daemon)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let prefix: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", prefix)
    } else {
        s.to_string()
    }
}

fn indent(s: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    s.lines().map(|line| format!("{}{}\n", pad, line)).collect()
}
