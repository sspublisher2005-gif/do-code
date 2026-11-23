# DoCode Beta — Implementation Plan

**As-of date:** 2026-11-24
**Status:** Planning document. No implementation started.

## 1. Build Order (Critical Path)

The single critical path: **tool loop → streaming → incremental edits → completion unblock → docs → release.**

Everything else (MCP, TUI, OAuth, hooks, skills) builds on top of these. Do not start P1 work until P0 tasks are done and the thin slice works.

## 2. Phase 1: Close Critical Risks (P0)

### T-001: Fix lying docs
- **Problem:** README says "sandbox matrix has NOT run" (now 20/20 PASS). STATUS.md was updated but README and parity ledger still contradict code.
- **Current state:** docs/STATUS.md updated. docs/README.md and docs/FEATURE_PARITY_LEDGER.md still stale.
- **Proposed change:** Update README.md sandbox section, update parity ledger statuses (A-007, A-008, B-004, C-005, D-003, E-002 → DONE), remove contradictory claims.
- **Dependencies:** None.
- **Acceptance:** No contradictions between any doc and any code. `grep -r "has NOT run" docs/` returns 0.
- **Effort:** Small.
- **DoD:** All docs consistent with code. Clippy/test still green.
- **Blockers:** None.

### T-002: Create AGENTS.md
- **Problem:** No AGENTS.md in repo root. Agent starts every session from zero.
- **Current state:** No AGENTS.md exists.
- **Proposed change:** Create AGENTS.md with build/test/lint commands, repo map, never-do list, definition of done.
- **Dependencies:** None.
- **Acceptance:** File exists at repo root. Contains build/test/lint commands, repo map, never-do list, definition of done.
- **Effort:** Small.
- **DoD:** AGENTS.md committed.
- **Blockers:** None.

### T-003: Create ./check script
- **Problem:** No single command to verify everything.
- **Current state:** Individual commands exist (cargo test, clippy, fmt) but no unified script.
- **Proposed change:** Create `check.ps1` (Windows) and `check.sh` (Unix) that runs: cargo fmt --check, cargo clippy -D warnings, cargo test --bins, cargo test --test cli_smoke.
- **Dependencies:** None.
- **Acceptance:** `./check` runs all gates and exits 0 on clean repo.
- **Effort:** Small.
- **DoD:** Script committed and tested.
- **Blockers:** None.

### T-004: Create SPEC.md
- **Problem:** No single source of truth for what to build. Ideas scattered across docs.
- **Current state:** FEATURE_LEDGER.md has 90 features but no primary user or definition of done.
- **Proposed change:** Create SPEC.md with: one primary user, one definition of done (testable sentence), one primary job-to-be-done, explicit scope boundary (P0/P1/P2/P3).
- **Dependencies:** None.
- **Acceptance:** SPEC.md exists with primary user, definition of done, job-to-be-done, scope boundary.
- **Effort:** Small.
- **DoD:** SPEC.md committed.
- **Blockers:** None.

## 3. Phase 2: Essential End-to-End Workflow (P0)

### T-005: Tool-call loop in provider.rs (OpenAI-compatible first)
- **Problem:** Provider is single-shot text. No tool_calls parsing. No observation loop.
- **Current state:** `provider.rs:249 generate_text()` sends "stream": false, no tools. `agent_loop.rs:164` runs fixed 6-role pipeline.
- **Proposed change:** Add `tools` parameter to generate_text. Add tool-call schema (read_file, edit_file, run_command, search). Parse tool_calls from response. Execute via tools::execute(). Feed results back. Loop until no tool_calls or budget exhausted. Wire into agent_loop.rs as alternative to fixed pipeline.
- **Dependencies:** T-004 (SPEC.md defines the tool set).
- **Acceptance:** `cargo test loop_runs_three_turns` passes. Agent reads file, edits file, runs test, observes result.
- **Effort:** Large (touches provider.rs, agent_loop.rs, tools.rs, message_store.rs).
- **DoD:** Test passes, clippy clean, agent completes a bug fix end-to-end.
- **Blockers:** None (OpenAI format is well-documented).

### T-006: Streaming output
- **Problem:** "stream": false. User sees nothing until mission completes.
- **Current state:** All 4 providers use stream: false.
- **Proposed change:** Add stream: true to request body. Parse SSE (Server-Sent Events) in response. Print tokens as they arrive. Buffer for tool_calls.
- **Dependencies:** T-005 (tool loop needs streaming for UX).
- **Acceptance:** Visible output during a 30-second run. Tokens appear incrementally.
- **Effort:** Medium.
- **DoD:** Streaming works on OpenAI-compatible provider. Test passes.
- **Blockers:** SSE parsing complexity.

### T-007: Incremental edits (unified diff)
- **Problem:** Full-file JSON replacement. Truncation → refuse. Unauditable.
- **Current state:** patches.rs does edit/create/delete/rename with checkpoints. context.rs has "truncated → refuse" cliff.
- **Proposed change:** Add unified diff apply to patches.rs. Add search/replace with fuzz tolerance. Handle truncation gracefully (apply partial, report what was skipped).
- **Dependencies:** T-005 (tool loop needs edit_file tool).
- **Acceptance:** `doc run` applies a 5-line diff to a 500-line file without rewriting it. cargo test passes.
- **Effort:** Medium.
- **DoD:** Unified diff apply works. Test passes.
- **Blockers:** Diff parsing complexity.

### T-008: Repo traversal + file reading
- **Problem:** 96KB pre-pack with substring ranking. Agent can't read files on demand.
- **Current state:** context.rs builds a 96KB context pack with substring scoring.
- **Proposed change:** Add repo traversal (respect .gitignore, skip binary/large files). Add read_file tool that reads on demand (bounded, redacted). Keep outline as navigation aid.
- **Dependencies:** T-005 (tool loop needs read_file tool).
- **Acceptance:** Agent reads files on demand. No "truncated → refuse" cliff.
- **Effort:** Medium.
- **DoD:** read_file tool works. Test passes.
- **Blockers:** None.

### T-009: Permission policy per tool
- **Problem:** tools.rs has only Deny/Execute. No allow/ask/deny per tool.
- **Current state:** `tool_policy()` returns Execute for allowed tools, Deny for everything else.
- **Proposed change:** Add PermissionPolicy enum (Allow, Ask, Deny). Map each tool to a policy. Wire risk.rs scoring into policy (risk >= 3 → Ask, risk >= 5 → Deny). Add approval memory.
- **Dependencies:** T-005 (tool loop needs permission checks).
- **Acceptance:** read_file auto-allowed, edit_file asks, run_command asks with risk score.
- **Effort:** Medium.
- **DoD:** Permission policy works. Test passes.
- **Blockers:** None.

### T-010: Session persistence + resume
- **Problem:** Mission state exists but resume is inspection-only.
- **Current state:** MissionStore persists to disk. Resume loads but doesn't continue.
- **Proposed change:** Save context (messages, tool results, plan) to disk. Resume from last checkpoint. Re-run failed step with same or different model.
- **Dependencies:** T-005 (tool loop needs session state).
- **Acceptance:** `doc resume <mission_id>` continues from where it left off.
- **Effort:** Medium.
- **DoD:** Resume works. Test passes.
- **Blockers:** Context serialization complexity.

### T-011: Real eval suite
- **Problem:** No tests on real tasks. 141 unit tests but none end-to-end.
- **Current state:** tests/cli_smoke.rs has 10 CLI smoke tests. No real coding tasks.
- **Proposed change:** Create 20 eval tasks (bug fix, feature add, test write, refactor, debug). Each with expected outcome + verification procedure. Run nightly. Track pass rate.
- **Dependencies:** T-005 (tool loop needed for real task execution).
- **Acceptance:** 20 tasks defined, runnable, pass rate recorded.
- **Effort:** Medium.
- **DoD:** Eval suite runs. Pass rate tracked. Results published.
- **Blockers:** Need real repos with known bugs.

## 4. Phase 3: Primary Differentiator

### T-012: Verification bundle
- **Problem:** No portable proof of what the agent did.
- **Current state:** Evidence ledger exists but no export format.
- **Proposed change:** `doc verify <repo> --diff <patch>` → compile, test, lint, fuzz → produce signed evidence bundle (.docode-proof). Offline-verifiable.
- **Dependencies:** T-005 (tool loop needed for execution), T-004 (SPEC.md).
- **Acceptance:** Bundle produced, verifiable offline with `doc verify --check <bundle>`.
- **Effort:** Large.
- **DoD:** Bundle format defined, validator works, test passes.
- **Blockers:** Schema design.

### T-013: Plain-language explain
- **Problem:** Non-coders can't read fix explanations.
- **Current state:** Error messages are engineer-facing.
- **Proposed change:** `doc explain <bundle> --level human|nontechnical` → compress to minimum review surface. Risk-ranked hunks, one proof command per hunk, plain-language summary.
- **Dependencies:** T-012 (needs evidence bundle).
- **Acceptance:** Non-coder can understand what changed and what's risky.
- **Effort:** Medium.
- **DoD:** Explain works. Test passes.
- **Blockers:** None.

## 5. Phase 4: Release

### T-014: Ship binary + installer
- **Problem:** No public binary, no installer, no release.
- **Current state:** cargo build --release produces docode.exe locally.
- **Proposed change:** GitHub Release with binaries (Windows, Linux, macOS). One-line installer (curl | bash, irm | iex). Checksums + SBOM.
- **Dependencies:** T-001 (docs fixed), T-005 (tool loop working).
- **Acceptance:** Fresh machine can install and run docode in 30 seconds.
- **Effort:** Medium.
- **DoD:** Release published, installer works, README has quickstart.
- **Blockers:** None (CI already builds release).

### T-015: Demo + launch
- **Problem:** No demo, no launch, no users.
- **Current state:** No demo GIF, no launch post, no community.
- **Proposed change:** 30-second demo GIF. README rewrite. Launch on GitHub, Reddit (r/rust, r/programming), Hacker News, X. Discord/Discussions.
- **Dependencies:** T-014 (binary exists).
- **Acceptance:** Demo shows real task completion. Launch post published.
- **Effort:** Medium.
- **DoD:** Demo exists, launch posted, feedback collected.
- **Blockers:** None.

## 6. Task Dependency Graph

```
T-001 (docs) ──┐
T-002 (AGENTS.md) ──┤
T-003 (./check) ──┤──→ T-005 (tool loop) ──→ T-006 (streaming) ──→ T-008 (repo traversal)
T-004 (SPEC.md) ──┘         │                      │                      │
                             ├──→ T-007 (diffs)     │                      │
                             ├──→ T-009 (permissions)│                      │
                             └──→ T-010 (resume)     │                      │
                                                      └──→ T-011 (evals) ──→ T-012 (verify bundle)
T-013 (explain) ←── T-012
T-014 (binary) ←── T-001 + T-005
T-015 (launch) ←── T-014
```

## 7. Effort Summary

| Phase | Tasks | Effort | Duration |
|-------|-------|--------|----------|
| Phase 1: Critical risks | T-001 to T-004 | Small ×4 | 1-2 weeks |
| Phase 2: End-to-end workflow | T-005 to T-011 | Large + Medium ×6 | 4-8 weeks |
| Phase 3: Differentiator | T-012 to T-013 | Large + Medium | 2-4 weeks |
| Phase 4: Release | T-014 to T-015 | Medium ×2 | 1-2 weeks |
| **Total** | **15 tasks** | | **8-16 weeks** |

## 8. What to Delay (if behind schedule)

If behind schedule, delay in this order:
1. T-013 (explain) — nice to have, not blocking
2. T-010 (resume) — inspection-only is acceptable for beta
3. T-012 (verification bundle) — the differentiator, but the agent works without it
4. T-009 (permissions) — Deny/Execute is sufficient for beta
5. T-006 (streaming) — ugly but functional without it

Never delay: T-005 (tool loop), T-008 (repo traversal), T-011 (evals), T-014 (binary release).

## 9. Dangerous Decisions to Avoid

- **Don't rewrite the architecture.** Incremental changes only. The evidence/audit/sandbox core is the moat — don't touch it.
- **Don't add MCP before the tool loop.** MCP is tool calls. It cannot exist without the loop.
- **Don't build TUI before the CLI is loved.** jcode and OpenCode earned their surfaces.
- **Don't add more providers before the loop works.** One provider (OpenAI-compatible) is enough for v1.
- **Don't build enterprise features before 100 users.** SOC2/SSO/DPA is 12-18 months of work.
- **Don't claim "best" without a published benchmark number.** jcode publishes failures. That's why people trust it.

## 10. Definition of Done (Beta)

DoCode Beta is done when:
1. `doc run "add a test that proves 2+2=4"` applies a diff, runs the test, prints PASS/FAIL, exits 0/1, and is repeatable.
2. A fresh Windows machine can install and run it in 30 seconds.
3. README has a 5-minute quickstart + demo GIF.
4. 20 eval tasks defined and pass rate recorded.
5. No contradictions between any doc and any code.
6. AGENTS.md exists with build/test/lint commands.
7. `./check` runs all gates and exits 0.
8. At least one external user has used it on a real repo.