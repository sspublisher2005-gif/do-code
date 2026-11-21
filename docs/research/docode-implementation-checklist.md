# Do Code implementation checklist — map research to code

**Date:** 2026-11-21 · **Basis:** read-only code inspection + four research agents. **Rule 1** (PDR.md:10-14) governs. Sandbox Windows-only, is_qualified()=false (sandbox.rs:613-615), process.run denied (tools.rs:63-68).

## Capability audit — what we ALREADY have (8 dream capabilities)

| Capability | Verdict | Evidence |
|---|---|---|
| Supervisor+Worker context separation | FOUNDATION | 8 AgentRole variants + per-role prompts (workflow.rs:37-42), typed AgentMessage (workflow.rs:1149-1168), per-role routing (workflow.rs:953-1007). **But** all roles get same context_text — no isolation. |
| Spec-first TDD + mutation gates | FOUNDATION (spec only) | Contract gate (contract.rs:28-80) stored as evidence (workflow.rs:560-586); test step hardcoded Blocked(TEST_PLAN_ONLY_NOT_EXECUTED) (workflow.rs:540-546). **No execution, no mutation.** "mutation journal" in workspace.rs is a file ledger, NOT mutation testing. |
| Cost-tiered routing | FOUNDATION (routing yes, tiers no) | Role→provider/model + fallback (router.rs:16-23,63-82), complexity classification (workflow.rs:400,418,616). Cost display-only "unknown" (workflow.rs:1229-1280). |
| AST/LSP repo maps | FOUNDATION (compact map yes, AST/LSP no) | Index headings/symbol_hints (context.rs:21-29), bounded ranking (context.rs:237-308); symbols are regex line-prefix (context.rs:380-407). relevance written never read (context.rs:51-53). |
| State-machine guardrails + audit | **HAVE (strong)** | Mission transitions (workflow.rs:98-99,318-319), illegal-transition rejection (workflow.rs:1033-1077), audit::append on every event, content-addressed evidence (evidence.rs:39-112), fail-closed complete() (workflow.rs:1282-1391), loop budgets (loopctl.rs:16-33,66-105). **PROTECT — this is the moat.** |
| Comprehension-first diff storytelling | FOUNDATION | per-file reason (patches.rs:24-32), patch summary (patches.rs:92-100), redacted preview (patches.rs:346-372). No structured narrative layer. |
| Self-pruning memory | FOUNDATION (memory yes, pruning no) | confidence/last_verified_at/status fields (memory.rs:20-40,602), verify/remove/reset (memory.rs:188-201,258-266,336-352), advisory brief (workflow.rs:44-79). No pruning logic. |
| Bounded repair + false-success=0 | FOUNDATION (both partial) | MAX_TURNS=24, LoopPolicy 40/3/30 (loopctl.rs); complete() refuses without verified test_log (workflow.rs:1387-1391). **No fail→fix→rerun; no execution → completion unreachable.** |

**Bottom line:** 1 fully present (audit/state machine), 7 foundations, 0 absent. The missing primitive is *executed, receipt-bearing evidence* — it blocks TDD, repair, mutation, even completion. Consistent with PDR.md:26-28.

## Next 3 modules — ranked by (impact × feasibility)

### Module 1 — Bounded repair loop + execution receipts
- **Acceptance:** (1) fixture with failing test → loop applies patch, runs declared test command, stores test_log receipt {argv, cwd, exit_code, output_sha256, patch_hash, timestamp}; (2) at most 2 attempts, each re-proposal carries prior failing receipt; stall/budget → Blocked with honest reason, no success claim; (3) complete() still refuses without verified test_log; process.run denied when is_qualified()==false; (4) eval uses injected local executor only — NEVER flip is_qualified without 26-case matrix evidence.
- **Test:** `repair_loop_repairs_then_completes_and_never_claims_unverified_success` — bad patch→good patch, assert attempt 1 receipt non-zero + complete() errors, attempt 2 receipt zero + completion after qualified-runner stub verifies, permanently-failing variant → Blocked after 2.
- **Path:** new `repair.rs`, small tools.rs/evidence.rs edits. Gate process.run on `is_qualified() && invocation_policy_ok`; make mark_verified accept a receipt from qualified runner.

### Module 2 — Cost-tiered routing (configure, don't build a new router)
- **Acceptance:** (1) RoutingConfig.tiers Cheap|Standard|Strong → provider/model, routes_for orders by complexity-inferred tier; (2) existing role overrides/fallback intact, default config = identical behavior (opt-in); (3) cost display-only "unknown", no cost stopping.
- **Test:** `simple_task_prefers_cheap_tier_complex_prefers_strong`.
- **Path:** router.rs + config, ~40 lines. Note research says "delete cost-tiered routing as a build" — this is CONFIGURATION of existing code, not a new subsystem (Rule 1 compliant).

### Module 3 — Compact semantic repo-map outline (no tree-sitter/LSP)
- **Acceptance:** (1) brace-matched outline records top-level symbols + line ranges + referenced names; render_context emits compact outline; zero new deps, no schema migration; (2) labelled 10-task fixture: precision@10 ≥ current heuristic, pack bytes ≤ current; (3) no LSP/tree-sitter added (deleted under Rule 1).
- **Test:** `outline_captures_top_level_symbols_with_line_ranges`.

## Deleted under Rule 1 (recorded, not built)
Real LSP/AST backend (Aider owns it; counter-evidence: Serena 10× slower than grep) · cryptographic audit trail (no demand) · auto-pruning of user-confirmed memory (data loss) · supervisor/worker swarm (15× tokens, weak coding payoff) · diff storytelling build (CodeRabbit owns it) · cost-based mission stopping (violates honesty) · 1000-micro-decision coherence (METR: not possible) · cheap full-file narrative generator (reason+summary already suffice).

## Skipped / risk
Read-only inspection; no build/tests run — HAVE claims are source-level. Windows sandbox unqualified and process.run denied → Module 1's production path cannot be honestly exercised until the 26-case matrix passes on a disposable runner (external gate, not a code change). TDD/mutation may fail Rule 1 at eval time — delete rather than keep as decoration.
