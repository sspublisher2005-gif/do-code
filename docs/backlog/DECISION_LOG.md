# Decision log — append only

**Rule:** every change to `/docs/backlog/FEATURE_LEDGER.md`, `/docs/PDR.md`, or a release gate gets an entry here. Format: date | decision | reason | evidence | approver.

## 2026-11-21
- **Froze the PDR** (`6017bef`). Reason: stop re-deciding the vision every conversation. Evidence: founder confirmed the goal repeatedly; PDR.md locked. Approver: founder.
- **Adopted Rule 1 as supreme** (`PDR.md:10-14`): keep only what beats alternatives; delete the rest. Reason: prevents feature bloat and unproven claims. Approver: founder.
- **Committed research deliverables** (`c36b9d7`): wishlist/pain points, dream-feature feasibility, competitor ranking, big-company gap, Do Code gap map, sources. Reason: 4 parallel agents + body-dev, primary sources.
- **Fact-checked external bundle** (`5262006`): rejected Bastion, Doc2Harness, IETF EQR, fabricated benchmarks/model names, jcode/picocli misattribution. Reason: those claims would break the no-lies rule. Evidence: GitHub API searches.
- **Founder accepted the publish rule**: V1.0 publishes when the core loop is proven by a benchmark (false-success = 0, real tests run) — not when every competitor feature is matched. Reason: "full-only" before proof is unachievable solo. Approver: founder ("yes").
- **Defined V1.0 release gate** (6 pass/fail items in FEATURE_LEDGER.md). Reason: founder wants a full first release; "full" is now a testable gate, not an unbounded feature list. Approver: founder.
- **Ranked next 3 modules**: (1) repair loop + receipts, (2) cost-tier config, (3) repo outline. Reason: impact × feasibility; missing primitive is executed evidence. Evidence: `docs/research/docode-implementation-checklist.md`.
- **Deleted under Rule 1**: crypto audit trail, 1000-micro-decision coherence, self-pruning branding, heavy AST/LSP, supervisor swarm now, diff-storytelling build, spec framework, cost-router build, cost-based stopping, plugin marketplace early, Bastion/Doc2Harness/EQR dependencies. Reason: no demand, competitor owns it, impossible, or violates honesty.
- **Created this feature ledger** (`docs/backlog/`). Reason: founder asked that ALL features/updates be listed permanently so nothing is forgotten.
- **Sequencing locked**: Windows first → engineer → breadth; Linux only after Windows qualifies; macOS after Linux. Reason: only real technical edge is Windows native; no parallel OS work.

## 2026-11-22
- **Implemented the 9 missing Windows-applicable sandbox cases** (FS-03, FS-04, FS-05, PROC-02, LOG-01, LIFE-01, WIN-02, WIN-03, PKG-01; EXEC-01 moved into the executed matrix). Reason: the spec has 26 cases, 20 apply to Windows; the runner only had 10, so qualification was impossible. Evidence: `scripts/sandbox_adversarial_windows.py`, `src/probe.rs`; local run 20/20 PASS → `WINDOWS_QUALIFIED`.
- **Qualification is now evidence-driven, not a hard-coded `false`** (`src/qualification.rs`). It flips only for a complete record bound to the exact binary hash with status `QUALIFIED` and all 20 applicable cases passed; `WINDOWS_SUBSET_PASS` never qualifies. Reason: the previous constant could never be earned and proved nothing. Evidence: `src/qualification.rs` unit tests; record present → "available (qualified)", removed → "NOT qualified".
- **Capture keeps the newest 1 MiB, not the oldest** (`src/sandbox.rs:append_bounded`). Reason: a truncated build log must show the failure tail; the old code showed the first megabyte and discarded the actual end. Found by the new LOG-01 case.
- **FS-05 measures data reads, not handle opens.** Reason: Windows' own DACL lets an AppContainer open `KsecDD`; failing the matrix on a handle it cannot read from would be measuring Windows, not our sandbox.
- **CI gate now requires `WINDOWS_QUALIFIED`** and retains the qualification record as an artifact. Reason: `WINDOWS_SUBSET_PASS` was explicitly not a qualification. Evidence: `.github/workflows/sandbox-matrix-windows.yml`.

## 2026-11-22 (process execution)
- **Enabled `process.run` behind the qualified sandbox** with receipts. Reason: the evidence gate needs real test runs; execution was `Deny`. Evidence: `src/tools.rs` (policy gated on `is_qualified()`), `src/sandbox.rs::run_in_project`, receipt JSON with argv/cwd/exit_code/timed_out/output_sha256/timestamp.
- **Sandbox now grants the project read-only and `target/`+caches writable** (Codex/Claude Code run in place; we are stricter). Reason: tests need the source tree; copying would kill incremental builds. Evidence: `src/sandbox.rs::SandboxAccess`, `grant_container_access`; matrix 20/20 still PASS.
- **Grant is idempotent** (no duplicate ACEs). Reason: repeated runs were bloating the project root DACL and slowing `SetNamedSecurityInfoW` to 30s+. Evidence: `src/sandbox.rs::grant_container_access` ACE walk + `EqualSid`.
- **`sandbox exec` now uses `run_in_project`**, so the matrix exercises the real execution path.
- **Capture keeps the newest 1 MiB** (not the oldest) so truncated logs show the failure tail.

## Update procedure
Append a line here with: `YYYY-MM-DD | decision | reason | evidence (commit/file:line) | approver`. Never edit history. If a decision is reversed, append a new line that supersedes it by ID/date.
