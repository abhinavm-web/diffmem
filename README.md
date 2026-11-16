# diffmem

**Zero-touch episodic failure memory for coding agents — correlates rejected agent diffs against final git commits.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](#roadmap--mvp-milestones)
[![Architecture: MVP](https://img.shields.io/badge/Architecture-MVP-orange.svg)](#roadmap--mvp-milestones)

---

## The Problem

AI enthusiasts switch constantly between Claude Code, Cursor, Antigravity, Codex, and local Ollama models. Existing memory solutions fall into two broken camps:

1. **Static instruction files (`CLAUDE.md`, `.cursorrules`)**: Require manual curation, bloat until they pollute every prompt, and go stale as the codebase evolves.
2. **MCP memory servers (`memory_write` / `memory_read`)**: Rely on the agent *remembering* to call `memory_write`. In real sessions, coding agents forget to log memories ~80% of the time—or write noisy, generic summaries that degrade future retrieval.

Worst of all, **episodic failure memory** (*"the agent tried X, you reverted it or edited it manually because Y broke, and the real fix was Z"*) disappears forever the moment a session ends.

## The Unclaimed Wedge: How It Works

`diffmem` requires **zero tool calls from the agent**. It runs as a passive background daemon + Git hook that computes the **"Diff-of-Diffs"**:

1. **Observe Rejected Attempts**: Watches agent transcripts/checkpoints and working-tree mutations before reverts, edits, or failed test runs.
2. **Correlate With Truth (`git commit`)**: When you actually commit working code, `diffmem` compares what the agent originally produced against what finally landed in Git.
3. **Compile Path-Scoped Rules**: Distills high-signal **"Mistake → Fix"** lessons bound to specific file globs (`src/auth/**`, `prisma/schema.prisma`) and stores them in a git-versioned memory log (`diffmem log`, `diffmem blame`, `diffmem revert`).
4. **Cross-Agent Delivery**: Exposes a lightweight MCP resource + automatic prompt-hook injector so *every* coding agent on your machine receives only the lessons relevant to the files it is touching right now.

```text
  Agent Draft Diff (Reverted / Edited) ──┐
                                         ├──► Diff-of-Diffs Correlator
  Final Human/Verified `git commit` ─────┘               │
                                                         ▼
                                          ┌─────────────────────────────┐
                                          │ Git-Backed Lesson Store     │
                                          │ [src/auth/**] "Never cache  │
                                          │  tenantId in module scope"  │
                                          └──────────────┬──────────────┘
                                                         │
                                     ┌───────────────────┼───────────────────┐
                                     ▼                   ▼                   ▼
                                Claude Code           Cursor            Codex / CLI
```

### Target CLI Experience (`< 60 second aha moment`)

> Note: Below is the target MVP CLI specification currently under construction.

```bash
# 1. Install the zero-touch git hook + transcript watcher in any repo
diffmem init

# 2. Code normally across Claude Code, Cursor, or Codex.
#    Whenever you revert an agent's broken attempt and commit the real fix,
#    diffmem extracts the delta automatically:
diffmem log
#  mem-a19f  [src/payments/**]  Agent used float math for cents; final commit uses BigInt.
#  mem-8c02  [tests/e2e/**]     Do not mock Redis in integration tests; use testcontainer port.

# 3. Inspect or roll back a learned rule just like Git
diffmem blame src/payments/invoice.ts
diffmem revert mem-8c02
```

## Roadmap & MVP Milestones

- [ ] **Milestone 1 — Diff-of-Diffs Extractor**: Git post-commit hook + working-tree snapshot watcher that pairs discarded/overwritten hunks with final committed hunks.
- [ ] **Milestone 2 — Multi-Tool Transcript Parser**: Adapters for Claude Code (`~/.claude/projects/`), Cursor, and Codex session logs to capture *why* an attempt failed (test error, user rejection).
- [ ] **Milestone 3 — Local Distillation & Path-Scoped Store**: Fast local/Flash distillation into compact, deduplicated, glob-scoped rules with `log`, `blame`, and `revert` CLI commands.
- [ ] **Milestone 4 — Universal Injection**: One-command MCP server + `UserPromptSubmit` / pre-edit hook adapters so memories surface automatically when touching matching paths.

## License

[MIT](LICENSE)
