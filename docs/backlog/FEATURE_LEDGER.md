# Do Code — Feature Ledger (single source of truth)

**Status:** LIVING DOCUMENT. Every feature idea goes here first. Nothing is "forgotten" — if it is not in this ledger, it does not exist as a plan.
**Created:** 2026-11-21 · **Governed by:** Rule 1 in `/docs/PDR.md` — keep only what beats alternatives; delete the rest.
**Evidence base:** `/docs/research/*` (4 agents, committed c36b9d7), fact-check (5262006), PDR (6017bef).

## Status definitions
- **HAVE** — implemented in code (cite file:line).
- **FOUNDATION** — partial scaffolding; named capability absent.
- **V1.0** — required before first publication (the "full version" gate).
- **V1.1+** — planned after V1.0 passes.
- **FUTURE** — parked; see `/docs/backlog/FUTURE_PARKING.md` for unpark conditions.
- **WATCH** — external technology to monitor, not build.
- **DELETED** — rejected under Rule 1; reason recorded, do not resurrect without new evidence.

## V1.0 RELEASE GATE — "full" means ALL of these pass, not all features exist
1. 26-case Windows sandbox matrix PASS with retained hash + runner fingerprint; `is_qualified()` evidence-driven.
2. `process.run` executes only inside qualified sandbox, with receipts.
3. Bounded repair loop ≤2; false-success rate = 0 on 30-task eval.
4. Invariant ledger, hard-to-fool verifier, glob-aware deny boundary, invocation-level policy, anti-slop gate.
5. One-command signed install + uninstall/rollback.
6. Published eval table with method, date, and limitations.
If any fails → do not publish. This is the honest version of "full".

---

## A. Security & trust (moat candidates)

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| A-001 | Fail-closed completion gate (`complete()` refuses without verified test_log) | HAVE | — | `workflow.rs:1282-1391`; protected moat |
| A-002 | SHA-256 content-addressed, project-bound evidence manifests | HAVE | — | `evidence.rs:39-112,191-212` |
| A-003 | Mission state machine + illegal-transition rejection | HAVE | — | `workflow.rs:98-99,318-319,1033-1077` |
| A-004 | Append-only audit log on every material event | HAVE | — | `audit::append` call sites; `docs/PDR.md:56` |
| A-005 | Windows AppContainer + JobObject + DACL + env allowlist | HAVE (unqualified) | — | `sandbox.rs`; 10/10 subset green; `is_qualified()=false` |
| A-006 | 26-case matrix + evidence-driven `is_qualified()` | FOUNDATION | V1.0 | `sandbox.rs:613-615`; M1 gate |
| A-007 | `process.run` inside qualified sandbox | PLANNED | V1.0 | `tools.rs:63-68,99-132` currently denied |
| A-008 | Execution receipts {argv, cwd, exit, output hash, patch hash, timestamp} | PLANNED | V1.0 | `docs/evals/README.md`; Claude #104504 |
| A-009 | Invocation-level `(command,args,cwd)` policy | PLANNED | V1.0 | Codex execpolicy spec; HN 44214926 |
| A-010 | Glob-expanded deny-by-default path boundary (Windows) | PLANNED | V1.0 | Claude #111167 data loss; Pillar escapes |
| A-011 | Secret redaction **before** model sees tool output | FOUNDATION | V1.0 | `redaction.rs` exists; verify coverage |
| A-012 | Invariant ledger outside compaction | PLANNED | V1.0 | Claude #111493 requirement vanished |
| A-013 | Anti-slop gate on patch write (no `todo!`/placeholder) | PLANNED | V1.0 | reimplement Gemini-zip idea independently |
| A-014 | Hard-to-fool verifier; rules agent cannot edit | PLANNED | V1.0 | Claude official verification blog 2026-11-16 |
| A-015 | Zero-telemetry proof + SBOM + signed binary | PARTIAL | V1.0 | SBOM exists; signed release missing; 27/30 ranked |
| A-016 | Approval gates + checkpoints + tamper-evident audit | HAVE | — | `patches.rs` checkpoints; `evidence.rs` |
| A-017 | Cryptographic/SCITT action receipts | DELETED | — | no demand signal; our SHA-256 manifests suffice |
| A-018 | "Crypto audit trail" as a marketed feature | DELETED | — | no primary source users want it |

## B. Core loop & execution

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| B-001 | Bounded agent loop (24 turns, 40 steps / 3 stalls / 30 min) | HAVE | — | `agent_loop.rs:20-21,166-170`; `loopctl.rs:16-33,66-105` |
| B-002 | Typed tools: validate → policy → audit | HAVE | — | `tools.rs`; integration test in `tests/agent_loop.rs` |
| B-003 | Parallel independent tool calls; sequential edit/verify | PLANNED | V1.0 | Bullet spec (16% fewer round trips — vendor, UNVERIFIED but cheap to test) |
| B-004 | Bounded repair loop ≤2 with fail→fix→rerun | PLANNED | V1.0 | Module 1; acceptance test in `docode-implementation-checklist.md` |
| B-005 | Freeform patch grammar (Codex `apply_patch.lark` spec) | PLANNED | V1.1 | Apache-2.0 spec; keep our preimage hashes |
| B-006 | Compaction checkpoint separate from replacement history | PLANNED | V1.1 | Codex `compact.rs:75-102`; Claude #111493 |
| B-007 | Frozen prompt prefix + tool output token limits | PLANNED | V1.1 | Claude `--system-prompt-snapshot`; jcode locked snapshot |
| B-008 | Resume at transaction boundary, no duplicate side effects | FOUNDATION | V1.1 | mission store + journal; needs test |
| B-009 | Immutable keep/revert evaluator | FOUNDATION | V1.0 | autoresearch pattern; eval needs ground truth agent can't edit |
| B-010 | Spec-first TDD (test before implementation) | FOUNDATION | V1.0/V1.1 | `contract.rs:28-80`; test step `Blocked(TEST_PLAN_ONLY_NOT_EXECUTED)` |
| B-011 | Mutation gate (cargo-mutants style) | FUTURE | — | Rule 1: eval first, delete if it doesn't catch fake tests |
| B-012 | Self-host: Do Code builds another coding agent | FUTURE | — | keep only as proof of engine strength |
| B-013 | 1000-micro-decision coherence | DELETED | — | METR: not possible as framed |

## C. Context, memory, planning

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| C-001 | Bounded repo index + deterministic ranking | HAVE | — | `context.rs:21-29,199-320,237-308` |
| C-002 | Spec-first contract gate | HAVE | — | `contract.rs:28-80`; stored as evidence `workflow.rs:560-586` |
| C-003 | Task board per-file locks + dependency DAG | HAVE | — | `taskboard.rs` (per PDR) |
| C-004 | Complexity classification + per-role routing | HAVE | — | `workflow.rs:400,418,616`; `router.rs:16-23,63-82` |
| C-005 | Compact brace-matched repo outline (top-level symbols + ranges) | DONE | V1.0 (Module 3) | src/outline.rs; docode outline CLI |
| C-006 | Context hygiene: clear re-fetchable tool results, file-backed state | PLANNED | V1.1 | Anthropic "context rot"; issues #17428/#27242 |
| C-007 | Narrow in-repo memory + staleness lint (no auto-delete) | FOUNDATION | V1.1 | `memory.rs:20-40,188-201,258-266,336-352,602` |
| C-008 | Portable session/memory + config import (CLAUDE.md/AGENTS.md) | PLANNED | V1.1 | power-user churn; portability gap 26/30 |
| C-009 | `program.md`-style project contract | PLANNED | V1.1 | Karpathy autoresearch; 21/30, 10/10 feasibility |
| C-010 | Real LSP / tree-sitter repo map | DELETED | — | Aider owns; Serena MCP 10× slower than grep |
| C-011 | Self-pruning memory (branding) | DELETED | — | Mem0 is ADD-only; use C-007 instead |
| C-012 | Supervisor + worker context separation | FUTURE | — | 15× tokens; weak coding payoff (Anthropic) |
| C-013 | 1M-token context window | FUTURE | — | model-dependent; we don't control models |

## D. Providers & models

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| D-001 | Multi-provider router + ordered fallback | HAVE | — | `router.rs`, `provider.rs` |
| D-002 | Cost honesty: "unknown" unless provider-reported, never zero | HAVE | — | `workflow.rs:1229-1280`; our rule |
| D-003 | Cost-tier config Cheap/Standard/Strong (configure, not build) | DONE | V1.0 (Module 2) | src/router.rs (tiers, role_tiers) |
| D-004 | Local models (Ollama / LM Studio) | FOUNDATION | V1.1 | table stakes: OpenCode/Codex/Crush all have it; Rule 1 conditional |
| D-005 | Native streaming responses | FUTURE | — | provider work; no differentiator |
| D-006 | Structured JSON output | FOUNDATION | — | agent loop parses single JSON object |
| D-007 | Cost-based mission stopping | DELETED | — | violates honesty rule; no hard stops |

## E. Tools & extensions

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| E-001 | workspace.list / read / search | HAVE | — | `tools.rs`; bounded, redacted |
| E-002 | process.run (qualified sandbox only) | DONE | V1.0 | see A-007 |
| E-003 | Git status/diff/log tools (read-only) | FUTURE | — | avoid arbitrary shell |
| E-004 | HTTP/fetch tool | FUTURE | — | needs network policy |
| E-005 | Browser tool | FUTURE | — | isolated runtime only |
| E-006 | MCP stdio client under one policy path | PLANNED | V1.1 | per-server allowlist, timeout, no secrets |
| E-007 | `SKILL.md` loader (on-demand) | PLANNED | V1.1 | open standard; skills are instructions, not permissions |
| E-008 | Typed lifecycle hooks (pre/post tool, apply, verify) | PLANNED | V1.1 | same sandbox as user tools; never elevate |
| E-009 | Versioned plugin manifest | FUTURE | — | only after 3 internal consumers |
| E-010 | Plugin marketplace | DELETED (early) | — | signing/rollback absent |
| E-011 | ACP adapter (Agent Client Protocol) | FUTURE | — | only if second surface exists |
| E-012 | WASM tool modules | WATCH | — | sandboxed analysis, monitor only |

## F. Multi-agent & worktrees

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| F-001 | Sequential specialist roles (honestly named, NOT agents) | HAVE | — | `workflow.rs`; `teams.rs` says sequential perspectives |
| F-002 | Git worktree isolation per task | FUTURE | — | Emdash sub-second start; not needed for V1.0 |
| F-003 | Conflict-notify (file-touch index → typed notices, no locks) | PLANNED | V1.1+ | jcode MIT spec; 27/30 ranked — after sandbox qualifies |
| F-004 | DM/broadcast agent channels | FUTURE | — | jcode spec; cross-swarm DM only |
| F-005 | Self-dev loop (build/test/reload own binary) | FUTURE | — | jcode spec; proof-of-engine only |
| F-006 | Sub-second task start | FUTURE | — | requires daemon; trade-off vs lightweight |

## G. UX & surfaces

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| G-001 | CLI command surface (~29 commands) | HAVE | — | `src/main.rs` |
| G-002 | JSON / non-interactive CI mode | PARTIAL | V1.0 | `--json` needed for eval + CI |
| G-003 | Guided mode for non-coders (3–5 clarify questions, plain English) | PLANNED | V1.1 | HN 47650080; setup simplicity is the switch condition |
| G-004 | Expert power mode | FOUNDATION | V1.0 | CLI flags; progressive disclosure |
| G-005 | Team/enterprise policy mode | FUTURE | — | SOC2/SSO 12–18 months |
| G-006 | TUI | DEFERRED | — | P3; CLI proven first |
| G-007 | Desktop (Tauri / Iced) | FUTURE | — | after stable API |
| G-008 | Autonomy slider (design rule + config) | PLANNED | V1.1 | Karpathy: HIGH confidence; not marketed as separate feature |
| G-009 | Visual/GUI diff verification | PRINCIPLE ONLY | — | Karpathy needs it; CodeRabbit owns implementation — no custom build |
| G-010 | Adaptive communication layers 1/2/3 (plain → plan → full diff/logs) | PLANNED | V1.1 | `docs/PDR.md` user requirement |

## H. Product, release, platform

| ID | Feature | Status | Target | Evidence / decision |
|---|---|---|---|---|
| H-001 | One-command signed install + uninstall/rollback | PLANNED | V1.0 gate | switch analysis: setup <60s |
| H-002 | 30-task eval + published number (false-success=0) | PLANNED | V1.0 gate | `docs/evals/README.md` |
| H-003 | Terminal-Bench as optional eval harness | FUTURE | — | real repo; vendor scores UNVERIFIED |
| H-004 | Reproducible build + SBOM | PARTIAL | V1.0 | SBOM exists; reproduction not proven |
| H-005 | Docs: 60-second quick start + honest limits | PARTIAL | V1.0 | README exists; needs rewrite |
| H-006 | Published comparison table with method/date/limitations | PLANNED | V1.0 gate | truth rule |
| H-007 | First-user feedback loop | PLANNED | after V1.0 | feedback before features |
| H-008 | Cross-platform CI (8 legs) | HAVE | — | Rust 1.88/1.99 × 4 OS legs |
| H-009 | Linux sandbox (bwrap + Landlock + seccomp) | FUTURE | — | only after Windows qualifies |
| H-010 | macOS sandbox | FUTURE | — | only after Linux qualifies |
| H-011 | Enterprise SOC2/SSO/audit compliance | FUTURE | — | 12–18 months; validate 10 interviews first |

## Rules for maintaining this ledger
1. New idea → add a row with ID, status, target, evidence. Never start coding from chat alone.
2. Status changes → append to `/docs/backlog/DECISION_LOG.md` with date and reason.
3. Anything rejected → move to `FUTURE_PARKING.md` with the unpark condition; never silently drop.
4. Before each milestone, re-read this file and `/docs/PDR.md` Rule 1.
5. No feature enters V1.0 unless it is in the V1.0 release gate list above.
