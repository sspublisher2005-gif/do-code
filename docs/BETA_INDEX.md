# DoCode Beta — Document Index

**As-of date:** 2026-11-24

## Planning documents (Phase A-J)

| Document | Path | Purpose |
|----------|------|---------|
| Research report | docs/research/docode-beta-research.md | Repo audit, competitor matrix, user pain points, evidence table, uncertainties |
| Product brief | docs/product/docode-beta-product-brief.md | Target segment, positioning, beta scope, exclusions, success metrics, risks |
| Technical + TUI design | docs/architecture/docode-beta-design.md | Architecture assessment, tool loop, permissions, TUI, failure/recovery, security, context, cross-platform, model strategy, testing, unresolved decisions |
| Implementation plan | docs/plans/docode-beta-implementation-plan.md | 15 ordered tasks, dependency graph, effort, release gates, dangerous decisions |
| Evaluation + launch | docs/release/docode-beta-evaluation-and-launch.md | 20-task benchmark, release gates, platform evidence, launch strategy, feedback collection |

## Existing documents (pre-beta)

| Document | Path | Purpose |
|----------|------|---------|
| PDR (frozen) | docs/PDR.md | Vision + Rule 1 (supreme) |
| Technology decisions | docs/technology-decisions.md | 10 ADRs |
| Evals design | docs/evals/README.md | 30-task benchmark, false-success=0 gate |
| Feature ledger | docs/backlog/FEATURE_LEDGER.md | 90 features A-H + V1.0 release gate |
| Decision log | docs/backlog/DECISION_LOG.md | Append-only decision history |
| Future parking | docs/backlog/FUTURE_PARKING.md | Parked/deleted ideas with unpark conditions |
| Status | docs/STATUS.md | Current Rust status (updated Nov 23) |
| Sandbox design | docs/SANDBOX_DESIGN_AND_TEST_SPEC.md | 26-case adversarial matrix spec |
| Sandbox matrix | docs/SANDBOX_ACCEPTANCE_MATRIX.json | Machine-readable matrix |

## Research documents (10 files in docs/research/)

See docs/research/ for: EXECUTIVE_SUMMARY.md, SOURCES.md, wishlist-and-painpoints.md, dream-features-feasibility.md, competitor-strengths-ranking.md, big-company-gap-analysis.md, named-engineers-dreams.md, multi-platform-dreams.md, reddit-user-provided.md, bundle-factcheck-2026-11-22.md, docode-implementation-checklist.md

## External briefs (produced in this session)

- karpathy-coding-agent-requirements.md (17 requirements, scorecard, build order)
- do-code-success-guide.md (strategy, free stack, 30/60/90 plan)

## Key decisions recorded

1. Beta target: Windows developers who use AI coding tools daily and are frustrated by agents that claim success without running tests, no audit trail, and macOS/Linux-first tools.
2. Primary differentiator: Tamper-evident evidence chain (hash-chained, offline-verifiable proof of what the agent did).
3. Supporting strengths: Windows-native sandbox, single Rust binary.
4. Beta scope: 11 P0 items, 8 P1 items, 22 P2 items, 8 P3 items (explicitly out of scope).
5. Rejected: "best general-purpose agent," "fastest agent," "cheapest agent," "non-coder agent" (deferred), "verification layer only" (rejected as primary).
6. Build order: tool loop -> streaming -> incremental edits -> completion unblock -> docs -> release.
7. Release gates: 9 criteria, 4 numerical thresholds (to be set after baseline).
8. Launch: GitHub + Reddit + HN + X + Discord. Demo GIF + benchmark report + honest limitations.

## What remains unproven

- Whether the tool-call loop can be built in 2-4 weeks by a solo non-coder with AI assistance
- Whether the "verifiable agent" niche has paying customers
- Whether Windows-only sandbox is sufficient for launch
- Whether the evidence ledger's value proposition resonates with developers
- Whether the friend can sustain daily shipping (velocity gap: 50 commits vs jcode's 12,454)
- Whether "for all" non-coders is achievable or a distraction
- Whether NLnet funding application will succeed (deadline Jan 3 2027)
- Whether the repo will get any external users without distribution effort

## Next action

Build the tool-call loop in provider.rs (OpenAI-compatible first): define 4 tool schemas, parse tool_calls, execute via tools::execute(), feed results back, loop until done or budget exhausted. Write one test: loop_runs_three_turns. Wire into agent_loop.rs. This is the one change that converts "proposal generator" into "coding agent." Everything else depends on it.