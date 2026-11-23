# DoCode Beta — Research Report

**As-of date:** 2026-11-24
**Repository:** sspublisher2005-gif/do-code, branch fix/sandbox-cross-verification, HEAD 5ff129a
**Sources:** Full repo audit (27 modules, ~13,761 lines Rust, 141 tests), 10 research documents in docs/research/, docs/backlog/FEATURE_LEDGER.md (90 features), docs/PDR.md, docs/STATUS.md, docs/technology-decisions.md, docs/evals/README.md, and external research from Nov 22-23 2026.

## 1. Repository Audit Summary

### What exists and works (verified by build/test/clippy)
- Rust codebase: ~13,761 lines, 27 modules, 141 tests passing, clippy -D warnings clean, fmt clean
- Windows sandbox: AppContainer (zero capabilities = OS network deny) + Job Object (kill-on-close) + private disposable workspace DACL + sanitized env. 20/20 adversarial matrix cases PASS locally. is_qualified() is evidence-driven (reads binary-bound qualification record).
- process.run: enabled behind qualified sandbox. Grants project root read+execute, target/ + cargo cache + temp read+write. Network denied. Every run produces a receipt: {argv, cwd, exit_code, timed_out, output_sha256, output_bytes, patch_hash, writable_roots, sandbox, timestamp}. Receipts appended to audit log.
- Bounded repair loop: agent_loop.rs tracks consecutive tool failures; stops after 2. Final rejected unless qualified test passed (up to 2 retries).
- evidence::mark_verified: implemented, succeeds when sandbox qualified. process.run success creates verified test_log evidence.
- Cost-tiered routing: RoutingConfig has tiers + role_tiers. routes_for resolves tier assignments, dedupes by (provider, model).
- Compact repo outline: src/outline.rs extracts top-level symbols per language. CLI: docode outline [--max-lines N].
- Evidence ledger: hash-chained manifests (evidence.rs), audit log (audit.rs), message store (message_store.rs). Fail-closed risk engine 0-5 with approval memory.
- Patches: edit/create/delete/rename with checkpoints and undo (patches.rs).
- Context: substring ranking, 96KB cap, max 20 files (context.rs).
- 4 providers: OpenAI-compatible, Ollama, Anthropic, Google. Single-shot text only (no tool calls, no streaming).
- CI: 6 jobs (3 OS x 2 toolchains), fmt/check/clippy/test/build/SBOM/gate.

### What is broken, stubbed, or impossible
- No tool-call loop: provider.rs sends "stream": false, no tool_calls parsing. Agent cannot read files, run commands, observe results, or iterate.
- No streaming output.
- Full-file JSON replacement in patches.rs (no incremental diffs, no fuzz/offset tolerance). Truncation in context.rs causes "refuse" cliff.
- No OAuth/subscription auth (env var API keys only).
- No MCP client, no hooks, no skills, no TUI.
- No session resume (inspection-only).
- No eval suite on real tasks.
- Context ranking is primitive (substring, max 20 files, 96KB cap).
- No cost tracking or spend ceiling.
- No non-coder UX.

### Critical contradictions (docs vs code)
- README says "sandbox matrix has NOT run" — now 20/20 PASS.
- STATUS.md said "partially proven" — now fully qualified (fixed this session).
- Parity ledger says "create/delete/rename missing" — patches.rs has them.
- SANDBOX_DESIGN says "not implemented" — driver exists and runs.
- Other agent said "no commits made" — 6 commits exist.
- Other agent said "missions can never complete" — fixed: mark_verified implemented, gate can open.

## 2. Competitor Landscape (verified Nov 23 2026)

| Agent | Language | Stars | Commits | Backing | Model access |
|-------|----------|-------|---------|---------|--------------|
| OpenCode | TS/Bun | 228.8k | 23,209 | Anomaly (YC) | BYOK + Zen/Go $10/mo |
| pi | TS | 123.5k | 8,173 | earendil-works | BYOK + subscriptions |
| Gemini CLI | TS | 109k | 6,640 | Google (Antigravity) | Google OAuth/API |
| Goose | Rust | 56.9k | 6,097 | Linux Foundation (AAIF) | 85+ providers, local models |
| Crush | Go | 29.2k | 4,627 | Charm | BYOK + Hyper (own provider) |
| jcode | Rust | 22k | 12,454 | YC S26 | Claude + OpenAI + Google OAuth |
| Claude Code | closed | — | — | Anthropic | Subscription-first |
| Codex CLI | Rust (open) | — | — | OpenAI | ChatGPT sign-in |
| Cline | TS | — | — | Cline | ClinePass $9.99 + BYOK |
| Cursor CLI | closed | — | — | Cursor | Cursor subscription |
| Kilo Code | TS | — | — | Anaconda (acquired) | Kilo Gateway + BYOK |
| Amp | closed | — | — | Sourcegraph | Amp + ChatGPT |
| Aider | Python | — | — | community | BYOK |
| Do Code | Rust | 0 | ~50 | solo | BYOK only |

### What each competitor does that Do Code doesn't
- Tool-call agent loop (all competitors have this; Do Code doesn't)
- Streaming output (all competitors; Do Code sends "stream": false)
- TUI/interactive UI (all competitors; Do Code is CLI-only)
- Incremental edits (all competitors; Do Code replaces whole files via JSON)
- Runs tests and iterates on failure (all competitors; Do Code blocks completion)
- Context compaction + caching (all competitors; Do Code has fixed 96KB pack)
- On-demand repo retrieval (all competitors; Do Code pre-packs 20 files)
- Subscription OAuth (most competitors; Do Code uses env var API keys only)
- MCP client (most competitors; Do Code absent)
- Hooks (Claude Code, Codex; Do Code absent)
- Skills (Claude Code, Codex, Crush; Do Code has DOCODE.md only)
- Subagents/parallel (Claude Code, Codex, OpenCode, jcode; Do Code sequential prompts)
- Headless/CI mode (Codex, Gemini, pi; Do Code partial)
- Resume sessions (all competitors; Do Code inspection-only)

### What Do Code has that nobody else has
- Hash-chained evidence ledger (evidence.rs) — tamper-evident, auditable
- Fail-closed risk engine (0-5) with approval memory
- Windows AppContainer + Job Object sandbox driver
- process.run with receipts (argv, cwd, exit_code, output_sha256, timestamp)
- False-success gate (won't claim done without verified evidence)
- Single Rust binary, no runtime dependency
- CI gate-integrity evaluator (catches fake-green CI)

## 3. User Pain Points (researched, evidence-based)

### Hard data
- Stack Overflow 2025 (33,662 respondents): 84% use/plan AI; 66% cite "almost right but not quite" as top frustration; 45.2% say debugging AI code is more time-consuming; 20% less confident in problem-solving.
- Stack Overflow 2026 (13-14k): 73% use AI daily; 48% trust AI when they can validate answers; only 16% trust for most tasks; 66% coding assistants/agents top use case.
- METR RCT: 16 experienced devs, 246 issues, AI made them 19% slower. Devs forecast +24% and still believed +20% after.
- Linux Foundation (Oct 2026): 25% of PRs are AI slop (31% large projects); 59% more volume; 53% disagree GenAI improved trust; only 35% have formal GenAI policy.
- DORA 2025 (~5,000): 90% use AI; 30% little/no trust; AI is an "amplifier" of existing org strengths/weaknesses.
- CodeRabbit (470 PRs): AI PRs have 10.83 issues vs 6.45 human (1.7x); logic +75%, readability 3x, security up to 2.74x.

### Repeated complaints (ranked by frequency across sources)
1. Review volume exceeds human capacity (25% slop PRs, 1.7x issues/PR)
2. "Almost right but not quite" worse than wrong (66% SO 2025)
3. Agent explanations too long, don't say what matters (Stenberg, Varda, Willison)
4. Confident claims not separated from verified facts (Karpathy, Beck, Stenberg)
5. Cost unpredictable, limits interrupt flow (~30% hit limits, PE 2026)
6. Sessions die, work lost (Yegge, Karpathy)
7. Debugging AI code takes longer than writing it (45.2% SO 2025, METR 19% slower)
8. Trust didn't improve (53% LF maintainers)
9. Skill atrophy / loss of understanding (17.1% SO 2026)
10. Non-coders hit a wall they can't diagnose (SO blog, Platformer, DAPLab)

### Repeated wants (ranked)
1. Cheap, fast check — verify by looking, not reading
2. Review surface sized to human attention — ranked diff, not wall of text
3. Explicit claim status: verified / assumed / not-checked
4. Durable work that survives context loss
5. Tenacity — don't give up at first "impossible"
6. Cost predictability + hard caps
7. Tests as contract (tests-first)
8. Sandboxing + local/offline
9. Plain-language explanation at user's level
10. Agent-native surfaces (CLI, API, MCP, structured logs)

### Gaps nobody is building
1. Agent-agnostic evidence compiler (portable proof of what happened)
2. Review-budget-aware output shaping (compression as discipline)
3. Claim/evidence separation as first-class artifact
4. Severity/risk adjudication locally
5. Windows-native, offline-first, single binary
6. Durable spec that outlives the session

## 4. Competitive Landscape Summary

### jcode (most important competitor to study)
- Same language (Rust), YC S26, 22k stars, 12,454 commits
- Wedge: performance (3.3MB/session vs 169MB Claude Code, 19ms TTI vs 3513ms)
- Swarm: multi-agent with file-conflict notifications, agents spawn sub-swarms
- OAuth account router with quota failover
- Self-dev mode (agent edits own source, hot-reloads)
- Token engineering: agentgrep, deferred tool loading, KV-cache monitor
- Todo science: phases/lanes/items, confidence stepping (92% vs 88% on TB 2.1), hill-climbability
- Own benchmark (jcode bench v1): 398x vs Claude Code 144x on float-print, same model
- System prompt: 236-671 tokens (Codex: 4,365; Claude: ~2,300)
- Ships daily, 213+ releases since February

### Claude Code (feature ceiling)
- One engine across terminal/IDE/desktop/web/mobile
- CLAUDE.md + AGENTS.md + auto-memory; Skills; Hooks; subagents + background agents + dynamic workflows; Agent SDK; MCP; Channels (push events); scheduled tasks; remote control/teleport; GitHub Actions; Chrome debugging; dedicated code-review product
- Native auto-updating installer, brew stable/latest, winget, apt/dnf/apk

### Codex CLI (OpenAI, open-source Rust)
- ChatGPT sign-in; reasoning-effort levels; /permissions granular sandbox; Skills & Plugins; subagents; web search; image input; codex resume; codex exec headless; cloud handoff; MCP; /review; "Import from another agent"

### OpenCode (228.8k stars)
- TUI + desktop + web. build/plan agents (Tab), @general subagent. Plugins, MCP, commands. /connect provider flow. Go $10/mo. Install everywhere. README in 20+ languages.

### Crush (Charm, Go, 29.2k stars)
- LSP-enhanced context. MCP stdio/http/sse + OAuth + Channels. Skills via agentskills.io standard (reads .claude/.cursor/.agents skills). crushrc = Bash-based config. Catwalk community model catalog. Hyper = Charm's own open-weight provider. Mid-session model switch. Provably-inert bash auto-approve. FreeBSD/OpenBSD/NetBSD/Android.

### Goose (Linux Foundation, Rust, 56.9k stars)
- oi-kit = reusable agent components. ACP (Agent Client Protocol) → Zed/Neovim. 85+ providers, in-process local models. Desktop + CLI. Evals/harbor (Terminal-Bench integration).

### Cline (TS)
- Agent core + SDK. Desktop (open-weight, parallel sessions, scheduled tasks), CLI (headless CI), VS Code, JetBrains, ACP. Kanban (parallel agents, worktrees, auto-commit, dependency chains). Enterprise: SSO/RBAC/OTel. ClinePass $9.99.

### Cursor CLI
- Agent/Plan/Ask modes. Print mode for CI. Cloud handoff with &. Sessions resume. /sandbox toggle with network control. Secure sudo prompt.

### Kilo Code (TS, acquired by Anaconda)
- Modes (Architect/Code/Ask/Debug). Custom rules. MCP. Kilo Gateway. Desktop. Teams/enterprise.

### Gemini CLI → Antigravity
- 109k stars. Google moved free tier to Antigravity CLI (June 18 2026). gemini-cli now enterprise-only. "Not tuned for Gemini 4+." Lesson: harness must be tuned per model generation.

### Amp (Sourcegraph)
- Opinionated multi-model. Orbs = per-thread cloud machines. Apps on every platform. Shareable threads. Puck coordinator. Plugins/skills/MCP. "If we don't use and love a feature, we kill it. No backward compatibility."

### Aider
- Reference for edit-format engineering (diff/whole/architect formats, well-formed %). Repo map via tree-sitter. Lint/test loops. Git auto-commits. Polyglot benchmark (225 Exercism tasks, 6 languages, top model 88%).

### Terminal-Bench (Stanford/Harbor/Laude, ICLR 2026)
- v4.1, run via harbor framework. Public arena for CLI agents ranked by resolution rate + cost + tokens.

## 5. Do Code Current State (verified)

### What works (build/test/clippy verified)
- Rust, ~13,761 lines, 27 modules, 141 tests passing, clippy clean, fmt clean
- Windows sandbox: 20/20 adversarial matrix PASS. is_qualified() evidence-driven.
- process.run: enabled behind qualified sandbox with receipts (argv, cwd, exit_code, timed_out, output_sha256, patch_hash, timestamp)
- Bounded repair loop (<=2) in agent_loop.rs
- False-success gate (Final rejected unless qualified test passed)
- evidence::mark_verified implemented
- Cost-tiered routing (tiers + role_tiers in RoutingConfig)
- Compact repo outline (docode outline CLI)
- Evidence ledger, audit log, message store (hash-chained)
- Risk engine 0-5 with approval memory
- 4 providers (OpenAI, Ollama, Anthropic, Google) — single-shot text only
- Patches (edit/create/delete/rename) with checkpoints/undo
- Context pack (substring ranking, 96KB cap)
- CI: 6 jobs, 3 OS x 2 toolchains

### What is broken/missing
- No tool-call loop (provider sends "stream": false, no tool_calls parsing)
- No streaming output
- Full-file JSON replacement (no incremental diffs)
- No OAuth/subscription auth
- No MCP client, no hooks, no skills, no TUI
- No session resume (inspection-only)
- No eval suite on real tasks
- Context ranking primitive (substring, max 20 files, 96KB cap)
- No cost tracking/spend ceiling
- No non-coder UX
- Docs contradict code (STATUS.md vs README vs parity ledger)

### Unique assets (nobody else has these)
- Hash-chained evidence ledger (evidence.rs)
- Fail-closed risk engine with approval memory
- Windows AppContainer + Job Object sandbox
- process.run with receipts
- False-success gate
- Single Rust binary, no runtime
- CI gate-integrity evaluator

## 6. Recommended Niche: "The Auditable Coding Agent"

**Positioning:** The only coding agent where every action is OS-sandboxed, hash-chained, and receipted. You don't trust the agent — you verify it.

**Who pays:** Enterprise teams, OSS maintainers, security teams, Windows shops, air-gapped/regulated environments.

**Three commands that define it:**
- doc run "fix the failing test" → agent writes code, runs tests, shows proof
- doc verify <repo> --diff <patch> → compile, test, lint, fuzz, produce evidence bundle
- doc explain <bundle> --level human → compress to minimum review surface

**What it solves:**
- "Did the tests actually run?" → false-success gate
- "What did the agent change?" → receipts + hash-chained transcript
- "Is this safe?" → risk engine + sandbox + redaction
- "I can't audit this" → tamper-evident evidence ledger
- "I'm on Windows" → Windows-native AppContainer sandbox
- "I can't afford cloud AI" → single binary, local models, no subscription

**What it doesn't solve (be honest):**
- Doesn't write better code than Claude Code or Codex (no tool loop yet)
- No TUI or streaming (no UX)
- No OAuth or MCP (no ecosystem)
- No evals on real tasks (no measurement)
- No public release or distribution

## 7. The 9-Step Build Order

1. Tool-call loop in provider.rs (function calling, bounded iterations, observation)
2. Sandboxed execution with captured stdout/stderr/exit code (unblock is_qualified())
3. Test runner integration with parsed results
4. Diff-based editing replacing full-file JSON (patches.rs)
5. Repo traversal + file reading (context.rs rewrite)
6. Streaming output (kill "stream": false)
7. Permission policy per tool (allow/ask/deny)
8. Session persistence + resume
9. Real eval suite (20 tasks, run nightly, pass rate tracked)

Everything else (MCP, TUI, subagents, parallelism, IDE, plugins) is P1/P2 and builds on top of these 9.

## 8. Karpathy's Requirements (verified from primary sources)

17 requirements extracted from his own posts, blog, and talks (Jan-Apr 2026):
1. Autonomy slider, not a switch (per-task, 0-6 levels)
2. Hybrid surface (agent + IDE + autocomplete)
3. Diff-first review (per-hunk accept/reject)
4. Lightweight inline plan mode + durable spec
5. Fast loop (streaming, interruptible, low latency)
6. Success criteria, not instructions (tests-first)
7. No silent wrong assumptions (clarification gate)
8. Minimal, surgical diffs (no bloat, no orthogonal edits)
9. Respect repo's real architecture (don't force modal patterns)
10. Tenacity (30+ min unattended run)
11. Explicit permission boundaries + isolated worktrees
12. Auditable, replayable actions
13. Small, auditable, containerized, forkable
14. Memory (persistent markdown, not RAG)
15. Parallel orchestration ("Claws")
16. Agent-native surfaces (CLI, API, MCP, structured logs)
17. Per-model tuning (jagged intelligence)

Do Code scorecard: ~3 strong (permission boundaries, auditable/replayable, small/containerized), 5 partial, 9 missing.

## 9. User Pain Points (from market research)

### Hard data
- SO 2025 (33,662): 84% use/plan AI; 66% "almost right but not quite"; 45.2% debugging harder; 20% less confident
- SO 2026 (13-14k): 73% daily; 48% trust when can validate; 16% trust for most tasks; 66% coding assistants top use
- METR RCT: 16 devs, 246 issues, 19% slower; forecast +24%, believed +20% after
- LF (Oct 2026): 25% slop PRs (31% large); 59% more volume; 53% disagree trust improved; 35% have GenAI policy
- DORA 2025 (~5,000): 90% use AI; 30% little/no trust; AI is "amplifier"
- CodeRabbit (470 PRs): 10.83 vs 6.45 issues/PR (1.7x); logic +75%, readability 3x

### Repeated complaints (ranked)
1. Review volume exceeds human capacity
2. "Almost right but not quite" worse than wrong
3. Agent explanations too long, don't say what matters
4. Confident claims not separated from verified facts
5. Cost unpredictable, limits interrupt flow
6. Sessions die, work lost
7. Debugging AI code takes longer than writing it
8. Trust didn't improve
9. Skill atrophy / loss of understanding
10. Non-coders hit a wall they can't diagnose

### Repeated wants (ranked)
1. Cheap, fast check (verify by looking)
2. Review surface sized to human attention
3. Explicit claim status (verified/assumed/not-checked)
4. Durable work that survives context loss
5. Tenacity (30+ min runs)
6. Cost predictability + hard caps
7. Tests as contract (tests-first)
8. Sandboxing + local/offline
9. Plain-language explanation
10. Agent-native surfaces (CLI, API, MCP, structured logs)

### Gaps nobody is building
1. Agent-agnostic evidence compiler (portable proof)
2. Review-budget-aware output shaping
3. Claim/evidence separation as first-class artifact
4. Severity/risk adjudication locally
5. Windows-native, offline-first, single binary
6. Durable spec that outlives the session

## 10. User Profile (observed from session)

- Non-coder founder/product owner
- Gives vague imperative commands ("do it," "continue," "go deeper")
- Repeats 5 phrases ~50 times each across ~200 messages
- Never writes code, never reads code, never verifies independently
- Asks "check is that true" (wants truth, not flattery)
- Demands "brutal truth and honest"
- Adds features before finishing current ones
- Delegates all decisions to the agent
- No measurable definition of done
- No budget, timeline, or target user persona specified
- Refuses to quit (never said "I give up")
- Obsessive, persistent, verification instinct
- Treats AI reassurance as partial proof
- Confuses wanting with having

## 11. Free Resources (verified)

- GitHub Actions: unlimited on public repos, 2,000 min/month private
- GitHub Releases, mdBook→Pages, crates.io, Discussions, Cloudflare Pages: free
- NLnet grants: €5,000-€50,000, deadline Jan 3 2027, human-written proposals
- GitHub Student Developer Pack: JetBrains, Namecheap domain, Datadog 2yr
- Model providers with free tiers: OpenRouter, Google AI Studio, Groq, Cerebras, DeepSeek API, Moonshot Kimi, Z.ai GLM
- GitHub Models: RETIRED July 30 2026 (do not recommend)
- Ollama: local/offline models
- cargo insta, cargo mutants, cargo nextest, cargo audit, cargo deny, gitleaks, semgrep: all free tools

## 12. Karpathy's 17 Requirements (verified from primary sources)

1. Autonomy slider (0-6, per-task)
2. Hybrid surface (agent + IDE + autocomplete)
3. Diff-first review (per-hunk)
4. Lightweight inline plan + durable spec
5. Fast loop (streaming, interruptible)
6. Success criteria, not instructions (tests-first)
7. No silent wrong assumptions (clarification gate)
8. Minimal, surgical diffs
9. Respect repo architecture
10. Tenacity (30+ min runs)
11. Permission boundaries + isolated worktrees
12. Auditable, replayable actions
13. Small, auditable, containerized, forkable
14. Memory (persistent markdown)
15. Parallel orchestration ("Claws")
16. Agent-native surfaces (CLI, API, MCP, structured logs)
17. Per-model tuning (jagged intelligence)

Do Code scorecard: 3 strong, 5 partial, 9 missing.

## 13. The 14-Day Plan (from research)

Day 1: AGENTS.md + ./check script + SPEC.md
Day 2: Unblock is_qualified() with real predicate
Day 3: sandbox.rs run one command, capture stdout/exit
Day 4: provider.rs tool-call schema for one provider
Day 5: The loop: call → tool → observe → repeat (max 5)
Day 6: Test runner: parse cargo test output
Day 7: patches.rs unified diff apply
Day 8: Wire doc run end-to-end (thin slice)
Day 9: cargo insta snapshots
Day 10: Streaming + progress
Day 11: Cost tracking + hard cap
Day 12: cargo mutants + cargo audit in ./check
Day 13: 20-task eval file, record pass rate
Day 14: README rewrite + demo GIF

## 14. Release Gates (from PDR + research)

- Windows sandbox matrix 20/20 PASS (DONE)
- process.run with receipts (DONE)
- Bounded repair loop ≤2 (DONE)
- False-success gate (DONE)
- Tool-call agent loop (NOT DONE — P0)
- Streaming (NOT DONE — P0)
- Incremental edits (NOT DONE — P0)
- Repo traversal + file reading (NOT DONE — P0)
- Permission policy allow/ask/deny (NOT DONE — P0)
- Session persistence + resume (NOT DONE — P0)
- Real eval suite (NOT DONE — P0)
- Fresh install on all claimed OS (NOT DONE — P0)
- Critical end-to-end workflows pass (NOT DONE — P0)
- No known unresolved critical security issue (PARTIAL)
- Commands/approvals/cancellation/failure tested (PARTIAL)
- Final reports distinguish verified from unverified (DONE — false-success gate)
- Core TUI flows in practical dimensions (NOT DONE — no TUI)
- Provider failures produce understandable errors (PARTIAL)
- Automated checks reproducible from docs (PARTIAL)
- Known limitations disclosed in README (NOT DONE — docs contradict code)

## 15. Launch Strategy

- One sentence: "The only coding agent that proves what it did — every command sandboxed, every change receipted, every claim verified."
- Demo: real coding task, real edits, real verification output, honest about failures
- Assets: README with fast start, short terminal demo, benchmark report, feature/limitation matrix, install instructions, troubleshooting, example workflows, known issues, contribution guide, security reporting, changelog, roadmap
- Channels: GitHub, Reddit (r/rust, r/programming), Hacker News, X, Discord, technical blog posts, video demos
- Ethical rules: no fake stars, no fake accounts, no misleading benchmarks, no fabricated urgency
- Early signals: successful installations, completed real tasks, repeat usage, useful issues/contributions, qualified feedback

## 16. Key Uncertainties

- Whether the tool-call loop can be built in 2-4 weeks by a solo non-coder with AI assistance
- Whether the "verifiable agent" niche has paying customers
- Whether Windows-only sandbox is sufficient for launch (Linux/macOS drivers not implemented)
- Whether the evidence ledger's value proposition resonates with developers (untested)
- Whether the friend can sustain daily shipping (velocity gap: 50 commits vs jcode's 12,454)
- Whether "for all" non-coders is achievable or a distraction
- Whether NLnet funding application will succeed
- Whether the repo will get any external users without distribution effort

## 17. Single Highest-Priority Task

Build the tool-call loop in provider.rs (OpenAI-compatible first): define 4 tool schemas, parse tool_calls, execute via tools::execute(), feed results back, loop until done or budget exhausted. Write one test: loop_runs_three_turns. Wire into agent_loop.rs. This is the one change that converts "proposal generator" into "coding agent." Everything else depends on it.

## 18. Decision Log Reference

All decisions recorded in docs/backlog/DECISION_LOG.md (append-only). Key decisions:
- Rule 1 supreme: keep only what beats alternatives
- PDR frozen Nov 21, 2026
- Moat = evidence gate + Windows sandbox + hash-patched undo + tamper-evident audit
- Deleted: 1000-micro-decision coherence, crypto audit trail, self-pruning memory branding, heavy AST/LSP, diff-storytelling, spec framework, cost-based stopping, cost router, Bastion crate, "build PUBG/ChatGPT", supervisor swarm now, heavy TUI now
- Kept for later: Terminal-Bench (P2), conflict-notify swarm (P3), conditional signed receipts, desktop TUI (post-v1.0)
- Next 3 modules: (1) repair loop + receipts (DONE), (2) cost tiers (DONE), (3) compact outline (DONE)
- Publish when core loop proven (real tests run, false-success=0), not "full-only"
- Windows first → Linux → macOS, one at a time

## 19. Documents Created This Session

1. docs/research/reddit-user-provided.md — Reddit findings (user-provided AI summaries)
2. docs/backlog/FEATURE_LEDGER.md — 90 features A-H + V1.0 release gate
3. docs/backlog/DECISION_LOG.md — append-only decision history
4. docs/backlog/FUTURE_PARKING.md — parked/deleted ideas with unpark conditions
5. docs/STATUS.md — updated with 20/20 matrix + qualified status
6. src/qualification.rs — evidence-driven sandbox qualification
7. src/outline.rs — compact repo outline
8. docs/research/docode-beta-research.md — this document

## 20. What Remains Unproven

- Whether the tool-call loop can be built in 2-4 weeks by a solo non-coder with AI assistance
- Whether the "verifiable agent" niche has paying customers
- Whether Windows-only sandbox is sufficient for launch
- Whether the evidence ledger's value proposition resonates with developers
- Whether the friend can sustain daily shipping (velocity gap: 50 commits vs jcode's 12,454)
- Whether "for all" non-coders is achievable or a distraction
- Whether NLnet funding application will succeed
- Whether the repo will get any external users without distribution effort
- Whether the 14-day plan is realistic for a non-coder
- Whether the market research numbers (SO 2026, PE 2026) are fully verified (some from secondary sources)

## 21. Source Index

- karpathy.bearblog.dev/sequoia-ascent-2026/ — Karpathy's requirements
- karpathy.bearblog.dev/verifiability/ — Verifiability thesis
- karpathy.bearblog.dev/blog/ — Blog index
- Dwarkesh "We're summoning ghosts, not building animals" (coding segment 29:45-40:05)
- X @karpathy 2026-01-26 post (silent assumptions, bloat, orthogonal edits)
- X @karpathy 2026-02-05 post ("agentic engineering" definition)
- X @karpathy 2026-02-25 post (macro actions, "coding agents basically work since December")
- X @karpathy 2026-02-26 post (DGX Spark 30-min run, "Claws" orchestrator)
- Simon Willison ai-assisted-programming tag page (2026 quotes from Boris Cherny, Paul Dix, Linus Torvalds, Kenton Varda, Charity Majors, Florian Herrengt, Georgi Gerganov, Mitchell Hashimoto, Armin Ronacher)
- Armin Ronacher "Agentic Coding Recommendations" (2025-06-12)
- Kent Beck "Augmented Coding: Beyond the Vibes" (June 2025)
- DHH Pragmatic Engineer interview (April 2026)
- Stack Overflow Developer Survey 2025 (33,662 respondents)
- Stack Overflow Developer Survey 2026 (13-14k respondents)
- METR RCT (16 devs, 246 issues, 19% slower; Feb 2026 follow-up)
- Linux Foundation Research (Oct 20, 2026, Red Hat-sponsored)
- DORA 2025 (~5,000 respondents)
- CodeRabbit (Dec 2025, 470 PRs)
- Veracode (86% XSS failure)
- Snyk (80% bypass security policy)
- jcode: github.com/1jehuang/jcode, jcode.sh, docs/HARNESS.md
- OpenCode: opencode.ai, anomalyco/opencode
- Claude Code: code.claude.com
- Codex CLI: developers.openai.com/codex
- Crush: github.com/charmbracelet/crush
- Goose: github.com/aaif-goose/goose
- pi: github.com/earendil-works/pi
- Gemini CLI: github.com/google-gemini/gemini-cli
- Cline: docs.cline.bot
- Cursor CLI: cursor.com/docs/cli
- Kilo Code: kilocode.ai/docs
- Amp: ampcode.com/manual
- Aider: aider.chat/docs/leaderboards
- Terminal-Bench: tbench.ai, laude-institute/terminal-bench
- curl/Stenberg: daniel.haxx.se (May 2025, Jan 2026, Oct 2026, Nov 2026 posts)
- Platformer (Casey Newton) — non-coder account
- Stack Overflow blog — non-coder account (Bolt "Yelp for bad bathrooms")
- Columbia DAPLab — non-coder wall at ~70%
- Replit/Amjad Masad — "self-driving company," cost estimator
- Anthropic/Dario Amodei — Davos "6-12 months" prediction
- Google/Sundar Pichai — "75% AI-generated, approved by engineers"
- Cursor/Michael Truell — "agent requests 15x YoY," browser experiment
- Bun-in-Rust — Jarred Sumner, 11-day rewrite, $165K tokens
- NLnet Foundation — nlnet.nl/propose, deadline Jan 3 2027
- GitHub Student Developer Pack — education.github.com/pack