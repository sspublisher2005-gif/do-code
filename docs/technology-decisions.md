# Do Code — Technology Decisions (ADRs summary)

**Status:** proposed; implements Rule 1 of `/docs/PDR.md`. **Date:** 2026-11-21.
**Method:** compare at least 3 realistic options per subsystem using evidence already gathered (source-level competitor RE + community research + local repo). Winner = best fit for "lightweight, free, powerful, equal-or-better", subject to Rule 1. UNVERIFIED where no fresh source was read.

---

## ADR-001 — Agent loop shape
- Context: our `agent_loop.rs` does one tool call per turn, sequential.
- Options: (a) keep sequential; (b) parallel independent tool calls + sequential edit/verify (Bullet, HN 49283063); (c) full DAG scheduler (jcode todo/swarm).
- Decision: **(b)** bounded in-flight futures with `needs_follow_up` (Codex spec, `turn.rs:2145/3541/4003`, Apache-2.0), plus `after` prerequisites from jcode todo (MIT) for big tasks. (c) deferred to P2; (a) rejected — measured 16% fewer round trips / 27% lower cost claimed by Bullet (vendor, UNVERIFIED independently).
- Validation: P1 benchmark round-trips, tokens, false-success=0.

## ADR-002 — Patch format
- Options: (a) full-file replacement (current); (b) structured JSON edits; (c) freeform grammar (Codex `apply_patch.lark`, parser `apply-patch/src/parser.rs:145`).
- Decision: **(c)** spec-reimplemented: `*** Begin/Update/Add/Delete File`, `@@`, `+/-/ `, `*** End of File`; keep our SHA-256 preconditions + checkpoints. License: Apache-2.0 spec reimplementation; keep NOTICE if code reused (we won't copy code).
- Validation: corruption, rename, delete, conflict, undo tests.

## ADR-003 — Tool policy
- Options: (a) command-name allowlist; (b) `(command,args,cwd)` prefix rules (Codex execpolicy, fetched 2026-11-21: `allow|prompt|forbidden`, match/not_match, strictest-wins); (c) model-called LLM permission check (Claude, Kir 2025 — 2 LLM calls per Bash, slow/costly).
- Decision: **(b)** deterministic prefix parser, invocation-level, no interpreter fallback (HN 44214926 showed Python used to route around Bash limits). Rule from Rule 1: delete (c) — wastes time.
- Validation: boundary-hopping probes in eval suite.

## ADR-004 — Context / compaction
- Options: (a) re-inject whole context (current); (b) structure-aware adaptive trimming (jcode agentgrep: transcript recency × file mtime confidence, 200-region cap); (c) embeddings everywhere.
- Decision: **(b)** + Codex compaction checkpoint separate from history (`compact.rs:75-102`) + `effective_budget = min(model window, session cap)` (jcode `compaction.rs:270`). (c) rejected: cost/latency, no evidence of benefit for our size — Rule 1.
- Validation: context precision@10 on labelled tasks; compaction invariant-retention test (Claude #111493 showed requirement vanished).

## ADR-005 — Sandbox
- Options: (a) copy Codex sandbox; (b) keep our Windows AppContainer/JobObject and qualify it; (c) VM/container backend.
- Decision: **(b) now**, (c) as optional second backend later behind one driver trait. (a) rejected: Apache-attribution + architecture coupling would cost more than spec reimplementation; Codex has WindowsRestrictedToken (source), so copying gains no Windows edge. Explicit: no host-shell fallback; `is_qualified` from retained matrix evidence only.
- Validation: 26-case matrix on disposable runner, hash + runner fingerprint; all BLOCKED = fail.

## ADR-006 — Runtime / language / distribution
- Context: 14.9k lines of Rust already working; founder says technology is a means.
- Decision: **keep Rust.** Not because "single binary" is sacred, but because rewrite cost > benefit and the Windows native sandbox is already in Rust. Revisit only if Rule 1 evidence says otherwise.
- Validation: startup/RSS benchmark vs jcode (unproven — measure before claiming speed).

## ADR-007 — Providers / local models
- Rule 1 applies: keep only if matching competitors and users use it. OpenCode/Codex/Crush all have first-class Ollama/LM Studio (fetched 2026-11-21). Our `provider.rs` has adapters; keep parity, no headline. Add LM Studio + Ollama auto-discovery only if eval shows user demand.
- Validation: provider conformance tests; no "every model" claims without test matrix.

## ADR-008 — Extensions (MCP/skills/hooks/plugins)
- Decision: spec reimplementation only (SKILL.md standard, MCP stdio → HTTP, typed hooks), all through the same policy/audit path as built-ins. No plugin marketplace before signing/rollback exists. jcode incident (silent hook injection, issue #811) is the explicit "never".
- Validation: permission-denial, timeout, no-secret-leak tests per extension.

## ADR-009 — Multi-agent
- Decision: P2. Copy spec: jcode conflict-notify (bidirectional file-touch index + typed soft interrupts, no locks), Codex limits (max 4 children, 30s wait). Sequential roles today are NOT agents — stop claiming otherwise.
- Validation: 3 independent tasks, conflict detected, full regression after merge.

## ADR-010 — Testing at scale
- Decision: cannot match internal eval labs; beat on transparency: publish evals with method/date/limitations (see `/docs/evals/README.md`). No vendor speed numbers quoted as fact (jcode/Bullet figures are vendor claims, UNVERIFIED).

---

## Ranking (best fit for Do Code, evidence-based judgments, not benchmarks)

| Category | Best source to learn from | Why | Our action |
|---|---|---|---|
| Loop | Codex + Bullet | parallel independent calls, receipts | reimplement spec |
| Patch | Codex | formal grammar + delta rollback | reimplement spec |
| Tool policy | Codex execpolicy + Pillar research | invocation-level, deny-by-default | reimplement spec |
| Context | jcode agentgrep + Codex compaction | measurable, bounded | reimplement spec |
| Sandbox primitive | Microsoft docs + our code | we already have Windows native | qualify, don't copy |
| Swarm | jcode | conflict-notify, no locks | P2 spec |
| Honesty/evidence | **Do Code** | nobody else hard-gates completion | protect + prove |

**Risks of this plan:** all third-party line numbers are from unpinned `main` (2026-11-21) and drift; vendor speed claims unverified; jcode semantic-todo spec disagrees with its own code; Reddit/X inaccessible. Each decision must be re-checked against Rule 1 at implementation time.
