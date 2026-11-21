# Executive summary — Top 10 switches ranked by (impact × feasibility)

**Date:** 2026-11-21. Method: four parallel research agents (primary sources), body-dev page reads, HN/GitHub APIs. Reddit/websearch unavailable — stated. Scores are analyst judgments 1–10, NOT benchmarks.

| # | What would make users switch | Impact | Feasibility | What Do Code ALREADY has | Verdict |
|---|---|---:|---:|---|---|
| 1 | **Receipts on every claim — no false "done"** (E2E/proper verifier, rules the agent can't edit, ≤3 repairs then human) | 10 | 9 | Evidence gate + fail-closed complete() (workflow.rs:1282-1391); Claude's own blog validates "change mustn't modify its judge" | **BUILD — Module 1 (repair loop + receipts)** |
| 2 | **Bounded repair loop** (fail→fix→rerun, 2 attempts, honest blocked) | 10 | 8 | Loop budgets (loopctl.rs), patches+undo; no exec yet | **BUILD — Module 1** |
| 3 | **Portable memory / config across harnesses** (CLAUDE.md/AGENTS.md, session import) | 9 | 8 | Local markdown memory + scope; no import layer | P2, ranked high by competitor report |
| 4 | **Zero-telemetry + SBOM + auditable binary** (rank #1 in competitor report, 27/30) | 9 | 8 | Local audit, MIT, SBOM exists; no signed release/telemetry proof | P2, validate with 10 enterprise interviews |
| 5 | **Trust/least-privilege at execution boundary** (secrets redacted before model, deny-by-default, glob-aware Windows) | 9 | 9 | AppContainer+JobObject+DACL, env allowlist, redaction; unqualified | P0 boundary hardening inside Module 1 |
| 6 | **Context rot solution** (file-backed state outside window, clear re-fetchable tool results) | 9 | 8 | Bounded context (context.rs), advisory memory; no compaction | P1/P2 — after execution |
| 7 | **Model-agnostic routing + failover** (configure, not build) | 8 | 9 | router.rs role routing + fallback; tiers missing | **BUILD — Module 2 (config tiers, ~40 lines)** |
| 8 | **Repo map / context precision** (cheap outline; NOT heavy AST/LSP — Aider owns it and Serena is 10× slower) | 8 | 9 | heading/symbol index, regex outline | **BUILD — Module 3 (brace outline)** |
| 9 | **Swarm with conflict-notify** (jcode model, no locks) | 9 | 7 (needs qualifying sandbox) | taskboard locks/DAG, no conflict-notify | P2, only after Module 1 passes |
| 10 | **One-command signed install + <60s setup** | 8 | 7 | none | P3 — publishing gate |

## Explicitly DELETED under Rule 1 (would waste time or lose to competitors)
1000-micro-decision coherence (METR: impossible) · crypto audit trail (no demand) · self-pruning memory branding (Mem0 is ADD-only) · heavy AST/LSP (Aider owns; speed counter-evidence) · supervisor/worker swarm (15× tokens) · diff storytelling build (CodeRabbit owns) · cost-tiered router build (commodity — config only) · spec framework (GitHub Spec Kit 150.9k★ owns it) · "build PUBG/ChatGPT" (impossible for any agent).

## The honest gap
Do Code has **1 of 8** dream capabilities fully (state machine + audit), 7 foundations. The single missing primitive — **executed, receipt-bearing evidence** — blocks TDD, repair, mutation gates and completion. Windows sandbox remains unqualified (is_qualified()=false); process.run denied. Until the 26-case matrix passes on a disposable runner, no "best/equal" claim is provable.

## What I'd do in your place (final answer)
Commit this research, then Module 1 (qualify sandbox → execution receipts → repair loop), publish the eval number, ship a Windows beta — then add the rest one at a time under Rule 1. No more research rounds; the evidence is sufficient.
