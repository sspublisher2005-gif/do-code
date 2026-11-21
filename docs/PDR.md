# Do Code — Product Discovery Report (PDR)

**Status:** FROZEN baseline. Changes require founder approval.
**Date frozen:** 2026-11-21.
**Repo:** `sspublisher2005-gif/do-code` (local path: `01_CURRENT_RUST_CODEBASE`).
**Evidence basis:** local source inspection (14,880 lines / 32 modules / 138 tests), three reverse-engineering agents (Codex, jcode, community) on 2026-11-21, body-dev read-only research (HN thread 44214926, Pillar Security "The Week of Sandbox Escapes", 2026-07-20), local Claude Code 2.1.323 CLI inspection. All external claims carry source + date. UNVERIFIED means not established.

---

## Rule 1 — the supreme rule (overrides everything below)

> A step, feature, or component stays only if it makes users succeed **better than the alternatives** — competitor tools, or no tool. If a competitor does it better, if it wastes time, or if users don't want it: **delete or replace it.**

Every keep/delete decision must record: (a) evidence, (b) the tradeoff, (c) a checkable reason. If evidence is missing, mark UNVERIFIED — do not decide from habit. This rule outranks previous ideas from either founder or agent. Local-model support is NOT exempt: it stays only if it matches competitors and users use it.

## 1. Vision

Do Code is one lightweight coding agent for everyone — beginner to expert — that helps users work fast and correctly, with clear communication, so they can create what they want. It must be equal to or better than Claude Code, Codex, OpenCode, Cursor, Aider, Cline, Gemini CLI and others in every category that matters. Users should love it, stay satisfied, and not need to switch. Free, powerful, fit for present and future. Technology is a means, not the goal: single binary or not, this crate or that — whatever is the best fit and works.

Explicitly NOT the goal: one-prompt AAA games (PUBG/CoD), Blender, ChatGPT, AGI claims, four separate products, or feature count before proof.

## 2. Current state (verified 2026-11-21)

- Rust 2024, `docode-cli` 0.7.0-rs.1, single binary `docode.exe`; `cargo test --all-targets --locked` = 125 unit + 10 integration green (per docs/STATUS.md 2026-11-08 entry; re-verify before claiming current).
- Real modules: `agent_loop.rs` (bounded loop, typed tools), `tools.rs` (validate → policy → audit), `contract.rs`, `patches.rs` (hash preconditions, checkpoints, undo), `evidence.rs` (SHA-256 manifests), `sandbox.rs` (Windows AppContainer + Job Object + DACL + env allowlist), `router.rs`, `provider.rs`, `memory.rs`, `loopctl.rs`, `mission_store.rs`, `audit.rs`.
- Sandbox executes: Windows subset matrix 10/10 PASS on windows-2022 and windows-2025 (GH runs 42485511999 etc.); but full 26-case matrix never ran, `--native` executor missing, `is_qualified()` stays false, `complete()` fail-closed.
- Harness gaps: one tool call per turn, full context re-injected each turn (`agent_loop.rs:171`), `process.run` policy-denied, no repair loop, no compaction, no MCP/skills/subagents/TUI.
- CI: 8 legs green (Rust 1.88/1.99 × ubuntu/macos/windows/windows-2025) per 2026-11-21 status; not independently re-run in this session.

## 3. Product-direction diagnosis

1. **What Do Code is now:** a safety-conscious proposal engine with a real Windows sandbox primitive and an unreachable completion gate.
2. **What it should be:** a coding agent that plans → patches → runs real tests → repairs → shows proof → completes only on evidence.
3. **Single most important workflow:** engineer task: "fix this bug / refactor safely, prove tests pass."
4. **Parts supporting it:** contract, patches+undo, evidence gate, sandbox driver, mission/audit, typed tools.
5. **Parts conflicting:** proposal-only loop with no execution, fake role claims in teams, full-file-only patches, hard-false qualification.
6. **Missing:** qualified sandbox execution, receipts, repair loop, bounded/compacted context, guided UX for non-coders, installer.
7. **Keep / refactor / replace / remove:** keep evidence+contract+patches+audit+sandbox primitive; refactor agent loop + context; replace role theater with real capabilities; remove dead code and false claims.
8. **First usable integrated version:** Windows beta — `docode agent` with qualified sandbox, real test execution, receipts, ≤2 repair attempts, diff + undo.
9. **Explicitly not yet:** TUI, desktop, marketplace, subagent swarm, Linux/macOS, enterprise tier, game-engine work.

## 4. Core workflow (conditional — Rule 1 applies to each step)

Ideal path: understand → (ask only if needed) → context → feasibility → plan → approved milestones → implement → run checks → repair → diff + plain-language summary → evidence → deliver.
Every step is conditional: if evidence shows competitors do it better, or users reject it, delete or replace it. The evidence gate is kept because hard data supports it (Claude issue #104504: 80/357 success claims unbacked, 5 contradicted by JSONL).

## 5. Differentiators (candidate moats — all require proof)

- [moat candidate] Evidence gate: `complete` refuses without executed proof. Nobody else hard-requires this.
- [moat candidate] Windows-native OS sandbox + local audit in one MIT Rust binary. Codex has WindowsRestrictedToken (source, 2026-11-21) — parity on primitive, our evidence binding is the difference.
- [table stakes] Local models, MCP, skills, subagents, diffs, repair loop — must match, not brag.
- [unproven] Speed/footprint — old CLI sample ~1–4 ms; agent-loop metrics not measured.

## 6. P0/P1/P2 backlog

**P0 (release integrity + trust, Windows):**
1. Write `--native` probe executor; run full 26-case matrix on disposable Windows runner; make `is_qualified()` evidence-driven (hash + runner fingerprint + all PASS). Keep fail-closed until pass.
2. Enable `process.run` inside qualified sandbox; capture exit code, output hash, timestamp, workspace/patch hash as receipt; forbid free-text success without receipt.
3. Bounded repair loop ≤2 attempts with fail→fix→rerun evidence; stall → honest blocked.
4. Invariant ledger outside compaction (safety/user requirements never only in summary); glob-expanded deny-by-default boundary (Claude #111167 Windows data loss); invocation-level `(command,args,cwd)` policy (HN 44214926 boundary-hopping).
5. Anti-slop gate on patch write (reject `todo!`, `unimplemented!`, placeholder comments) — spec observed in Gemini zip, reimplemented independently.

**P1 (real loop):** tool-call lifecycle with parallel independent calls + bounded context + frozen prompt prefix; freeform patch grammar (Codex spec, Apache notice); compaction checkpoint separate from history; tool output limits; resume at transaction boundary with no duplicate side effects; 30-task benchmark.

**P2 (breadth, conditional on Rule 1):** guided non-coder mode; skills (`SKILL.md`) + hooks + MCP stdio under one policy path; conflict-notify subagents (jcode spec, MIT notice); Linux then macOS sandboxes; TUI; installer + signed release. Each with its own acceptance test and evidence.

## 7. Command map (current / planned)

Current: `init`, `doctor`, `config`, `files`, `audit`, `index`, `context`, `agents`, `plan`, `run`, `agent`, `mission`, `apply`, `verify`, `complete`, `undo`, `freeze`, `sandbox`, `memory`, `decisions`, `evidence`, `model`, `tasks`, `contract`, `lessons`, `approvals`, `risk`.
Planned (Rule 1 conditional): `idea`, `research`, `build`, `fix`, `test`, `review`, `ask`, `exec`, `serve` (local-only), `--json`.

## 8. Architecture (target, incremental — not a rewrite)

Keep the single crate for now; extract modules only when behavior is stable. Layering: mission controller (state/cancel/recovery) → agent loop (model/tool/observation) → typed tools → policy/approvals → OS sandbox drivers → evidence/verification → CLI/API surface. Rule: model proposes; typed code enforces; sandbox executes; evidence decides completion.

## 9. Security / threat model (sources: Pillar 2026-07-20; jcode issue #811; Claude #111167)

Three boundary layers: direct execution, workspace writes, **host trust** (what unsandboxed components later do with agent-written files). Four failure modes to prevent: denylist sandboxes, workspace config as executable code, name-based command allowlists, privileged local daemons. Concrete rules: never let agent-written hooks/settings run host-side; glob-expand before path checks (Windows); deny by default; no planner/interpreter fallback to route around denial; installer must never silently write agent-executed config (jcode injected into `~/.claude`/`~/.codex` without consent — do not copy).

## 10. Testing and evaluation

See `/docs/evals/README.md`. Gate: 30 fixed tasks, mock + local provider, false-success rate 0, no unauthorized writes, honest blocked states. Each release reports measured success, regressions, cost/latency, and limitations with date.

## 11. Roadmap

M0: this PDR + evals + technology decisions + commit (now).
M1: sandbox qualification → execution → receipts → repair → anti-slop → boundary hardening. Publish Windows beta after gate.
M2: harness mechanics from competitor specs (one at a time, each tested).
M3: breadth. Windows first, then Linux, then macOS. Never parallel OS drivers.

## 12. Decision log / open questions

- 2026-11-21: Rule 1 adopted as supreme; founder confirmed direction ("best for everyone, equal or better, delete what isn't useful").
- 2026-11-21: privacy/local-model demoted from "must-have" to "Rule-1 conditional" (founder correction).
- 2026-11-21: sequencing chosen by agent (Windows → engineer → breadth) — founder to confirm at approval.
- Open: final command set, whether to extract crates, TUI timing, installer format. All resolved by Rule 1 + evidence when reached.

---
*Sources (fetched 2026-11-21 unless noted): local repo files; Codex raw source (Apache-2.0, LICENSE Copyright 2025 OpenAI); jcode raw source + GitHub API (MIT, 21,965 stars, pushed 2026-11-21); HN 44214926 via body-dev; Pillar Security "The Week of Sandbox Escapes" (2026-07-20) via body-dev; Claude Code 2.1.323 local CLI; Claude issues #104504/#111493/#111167; community research agent (HN/GitHub/DEV). Video transcripts: UNVERIFIED (no readable captions). Reddit: CAPTCHA-blocked. X/Twitter: not accessible.*
