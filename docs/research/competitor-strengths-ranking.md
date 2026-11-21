# Competitor strengths — best-of-each-brand & ranking

**Date:** 2026-11-21 · **Sources:** GitHub API (stars/pushed), official docs, HN Algolia. Reddit 403 (inaccessible) — not cited. Vendor benchmarks UNVERIFIED.

## Repo facts (GitHub API, 2026-11-21)
OpenCode 228.4k★ TS MIT · Claude Code 156.5k★ TS **license: null**, 15,972 open issues · Codex 136.1k★ **Rust Apache-2.0** · Pi 123.2k★ TS MIT · Gemini CLI 108.9k★ · Cline 71.8k★ · Goose 56.9k★ **Rust Apache-2.0** (AAIF) · Aider 50.2k★ last push **2026-05-22 (stalled)** · jcode 22.0k★ **Rust MIT** (created 2026-01-05, YC S26).

## Best-of-each-brand

| Brand | ONE strongest capability | Small Rust team feasible? |
|---|---|---|
| Claude Code | Cross-surface persistent context (CLAUDE.md/skills/hooks/MCP in terminal/IDE/desktop/web, --teleport) | harness yes, model no |
| Codex | Cloud sandboxed parallel tasks → PRs + local CLI | local yes, cloud needs capital |
| OpenCode | Provider-agnostic multi-surface harness, zero lock-in | yes (porting job) |
| Cursor | IDE-native diffs + background/cloud agents | no whole (IDE UX not Rust) — take worktrees only |
| Aider | Graph-ranked token-budget repo map | **yes — best quick win** (stalled upstream = absorption chance) |
| Cline | Approval-gated checkpointed Plan/Act across surfaces | CLI yes, IDE no |
| Gemini CLI | Subsidized frontier access + 1M context | no (distribution, not code); paid tier replaced by Antigravity 2026-06-18 — churn warning |
| Devin | Ticket→PR autonomy in cloud workspace + API | orchestration yes, trust/cost no |
| Pi | Minimal composable toolkit + serious supply-chain hardening | **yes — best design template** (no built-in permission system → sandbox is buyer's job) |
| Goose | oi-kit reusable Rust agent runtime (state machine, tools, providers, ACP) | **yes — reference architecture, already Rust** (alpha quality) |
| jcode | Swarm with file-conflict notification, DM/broadcast, self-dev | **yes — already Rust, 22k★ in <11 months** |

**3 of 11 already Rust** (Codex CLI, Goose, jcode). Every capability except subsidized inference and full IDE UX is small-team buildable.

## Ranking — top 10 capabilities (analyst scoring, not vendor data)

| # | Capability | User value | Feasibility | Differentiation | Total /30 |
|---|---|---:|---:|---:|---:|
| 1 | Zero-telemetry auditable binary + egress DLP + SBOM | 9 | 8 | 10 | **27** |
| 2 | Low-overhead parallel swarm + file-conflict notification (jcode) | 9 | 9 | 9 | **27** |
| 3 | Model-agnostic routing + provider failover | 10 | 9 | 7 | **26** |
| 4 | Reusable Rust agent runtime + SDK/ACP (oi-kit/Pi) | 8 | 10 | 8 | **26** |
| 5 | Cross-harness session/memory portability (Beacon pattern) | 9 | 8 | 9 | **26** |
| 6 | Approval gates + checkpoints + tamper-evident audit (Cline/Devin) | 9 | 8 | 7 | **24** |
| 7 | Graph-ranked repo map / context engineering (Aider) | 8 | 9 | 7 | **24** |
| 8 | Config/rules compatibility across harnesses | 8 | 9 | 6 | **23** |
| 9 | Git-worktree isolation + sub-second start (Emdash) | 8 | 9 | 6 | **23** |
| 10 | Hooks/skills/MCP extensibility | 8 | 9 | 5 | **22** |

**Excluded:** subsidized inference 20 (impossible for small team); enterprise compliance stack 18 (12–18 months); cloud ticket→PR 19; local inference 19; full IDE UX 17.
**Tie-break:** higher differentiation first — the gap is trust + portability, not feature breadth.

## Big-company gap — are the three reasons real?
1. **Lock-in — VERIFIED but eroding.** Anthropic Commercial Terms §D.4 and OpenAI §3.3(e) forbid competing models; but Claude Code supports Bedrock/Vertex/Foundry/gateways and Cursor offers ~40 models. Lock-in now sits at the model layer, not harness.
2. **Demo-velocity — PARTIAL.** 100-line mini-swe-agent ≈ near SOTA (65% vs 70%) — models absorb scaffolding. But labs ship durable substrate (MCP, SDKs, hooks, skills).
3. **Liability — VERIFIED as brake.** "AS IS" + 12-month fee caps in both terms; trust incidents (prompt-steganography researcher claim, issue #107757 805👍). Labs price compliance around it (SSO, CMEK, SOC2).
**Verdict:** real as portability + trust + cost-routing gap; wishful as "better model" play. Models absorb features over time — build the **neutral layer**, not a me-too agent. HN pushback: "Won't that be addressed by the CLIs themselves?" and "went back to Claude Code".

## Would users switch?
- **Power users: yes conditionally — already churn** (Cursor→CC→emdash; Codex for backend, CC for rest). Need: config parity (CLAUDE.md/AGENTS.md/.clinerules/GEMINI.md), MCP compat, memory import, signed one-binary. Setup <60s.
- **Non-coders: plausible, unproven** (Reddit inaccessible; HN says demand for UIs > CLI). Need: 1-click signed install, visual diffs, plain-language tasks. Setup fragility kills switching.
- **Enterprises: long gate.** Need SOC2 Type II, SSO/SCIM, DPA/MSA, audit logs, CMEK, zero-telemetry proof. SBOM + verifiable zero telemetry are gaps no incumbent headlines (validate with 10 interviews before investment).

Risk: the trust/zero-telemetry differentiation rests on one issue + one HN thread — validate before committing engineering time.
