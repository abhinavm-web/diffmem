# diffmem

**Zero-touch episodic failure memory for coding agents — correlates rejected agent diffs against final git commits.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)

---

## Overview

Developers constantly switch between Claude Code, Cursor, Antigravity, and Codex. Existing agent memory systems fail in practice for two reasons:

1. **Static instruction files (`CLAUDE.md`, `.cursorrules`, `AGENTS.md`)** require manual curation, grow stale, and bloat every prompt if not scoped.
2. **MCP memory servers (`memory_write` / `memory_read`)** rely on the agent *remembering* to invoke a tool call to record a lesson. In real debugging sessions, agents rarely log their own mistakes, and when they do, they write vague prose summaries.

Meanwhile, the highest-signal learning signal on your machine is completely ignored: **what the agent tried first that you reverted or overwrote versus what actually landed in `git commit`**.

`diffmem` captures this **"Diff-of-Diffs"** automatically without requiring a single tool call from the agent:

- **Zero-Touch Observation**: Watches working-tree draft diffs (before reverts or overwrites) and correlates them against final committed hunks via a Git `post-commit` hook.
- **Diff-of-Diffs Distillation**: Pairs rejected hunks with accepted hunks over overlapping file/line ranges to extract structured, path-scoped **Mistake -> Fix** rules (`src/payments/**`, `tests/e2e/**`).
- **Surgical Multi-Editor Synchronization**: Automatically syncs active rules into `CLAUDE.md`, `.cursor/rules/*.mdc`, and `AGENTS.md` using bounded marker blocks—strictly preserving existing human instructions and behavioral guidelines.

```text
  Agent Draft Diff (Reverted / Overwritten) --+
                                              +--> Diff-of-Diffs Correlator
  Final Verified `git commit` ----------------+               |
                                                              v
                                               +-----------------------------+
                                               | .diffmem/rules.json         |
                                               | [src/payments/**]           |
                                               | Mistake: f64 currency math  |
                                               | Fix: i64 integer cents      |
                                               +--------------+--------------+
                                                              |
                                          +-------------------+-------------------+
                                          v                   v                   v
                                      CLAUDE.md     .cursor/rules/*.mdc       AGENTS.md
```

## Installation

```bash
git clone https://github.com/abhinavm-web/diffmem.git
cd diffmem
cargo install --path .
```

## Usage

### 1. Initialize in any Git repository

```bash
# Creates .diffmem/ store and installs .git/hooks/post-commit
diffmem init
```

### 2. Watch working-tree drafts or record diff pairs

```bash
# Run the filesystem watcher to capture transient agent drafts before reverts
diffmem watch --interval 1.0

# Or explicitly correlate a rejected draft patch against an accepted patch
diffmem record --rejected broken_attempt.diff --accepted working_fix.diff
```

### 3. Inspect, blame, and revert learned rules

```bash
# List all active episodic rules
diffmem log
diffmem log --glob "src/payments/**" --json

# Inspect all rules matching a specific file path (with rejected vs. accepted hunks)
diffmem blame src/payments/ledger.rs

# Deactivate or delete a rule and immediately re-sync editor files
diffmem revert <rule-id>
diffmem revert <rule-id> --delete
```

### 4. Synchronize across agent environments

```bash
# Sync active rules into CLAUDE.md, .cursor/rules/, and AGENTS.md
diffmem sync --target all

# Preview changes without modifying files
diffmem sync --target claude --dry-run
```

## Architecture

```text
src/
├── main.rs             # CLI entry point (init, watch, log, blame, revert, sync, record)
├── config.rs           # Repository configuration (.diffmem/config.toml)
├── error.rs            # Typed error definitions (DiffmemError)
├── git/
│   ├── mod.rs          # Git CLI operations & diff extraction
│   └── hook.rs         # Non-destructive .git/hooks/post-commit installer
├── watcher/
│   ├── mod.rs          # Filesystem polling/notify loop for transient working-tree diffs
│   └── snapshot.rs     # Uncommitted draft snapshot persistence (.diffmem/snapshots)
├── correlator/
│   ├── hunk.rs         # Unified diff parser & line-range overlap matcher
│   ├── distill.rs      # Semantic Mistake -> Fix extractor & path-glob derivation
│   └── mod.rs          # Diff-of-Diffs correlation pipeline
├── store/
│   ├── rule.rs         # EpisodicRule model & deterministic ID hashing
│   └── mod.rs          # JSON rule store CRUD & glob matching
└── adapters/
    ├── claude.rs       # Surgical marker-bounded sync for CLAUDE.md
    ├── cursor.rs       # Frontmatter .mdc rule generator for .cursor/rules/
    └── antigravity.rs  # Marker-bounded sync for AGENTS.md (preserves behavior-guidelines)
```

## Development & Verification

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

[MIT](LICENSE)
