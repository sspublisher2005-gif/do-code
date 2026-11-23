# DoCode Beta — Product Brief

**As-of date:** 2026-11-24
**Status:** Planning document. No implementation started.

## 1. The Decision

**DoCode Beta is a Windows-first, TUI-first coding agent that executes real coding tasks in a sandbox and produces tamper-evident evidence of what it did.**

It is not "the best coding agent in all categories." It is the first coding agent that proves what it did — every command sandboxed, every change receipted, every claim verified — in a single Rust binary that works on Windows without a runtime.

## 2. Initial Target Segment

**Primary:** Windows developers who use AI coding tools daily and are frustrated by (a) agents that claim success without running tests, (b) no audit trail of what changed, and (c) macOS/Linux-first tools that don't work well on Windows.

**Secondary (adjacent, no scope expansion):** Open-source maintainers drowning in AI slop PRs; security teams needing audit trails; air-gapped/regulated environments.

**Not in initial scope:** Non-coders (despite "for all" ambition), enterprise compliance (SOC2/SSO), cloud long-running tasks, desktop GUI.

## 3. Main Job-to-Be-Done

"Help me fix a bug / add a small feature in my repo, run the tests, and show me proof of what changed — without me having to trust the agent's word."

## 4. Product Positioning

**One sentence:** "The only coding agent that proves what it did — every command sandboxed, every change receipted, every claim verified."

**Differentiator:** Tamper-evident evidence chain. No competitor (Claude Code, Codex, OpenCode, jcode, Cline, Cursor) produces hash-chained, offline-verifiable proof of what the agent did. Do Code already has this primitive (evidence.rs, audit.rs, message_store.rs). The beta makes it the product.

**Supporting strengths:**
1. Windows-native sandbox (AppContainer + Job Object) — real OS-level containment
2. Single Rust binary, no runtime dependency — deploy anywhere, air-gapped

## 5. Why This Over Alternatives

| Alternative | Why not chosen |
|-------------|----------------|
| "Best general-purpose agent" | Cannot out-feature Anthropic/OpenAI/Google/Cursor with 1 dev |
| "Fastest agent" (jcode's wedge) | jcode already owns this (3.3MB/session, 19ms TTI, published benchmarks) |
| "Cheapest agent" | Race to bottom; no sustainable business |
| "Non-coder agent" | No UX foundation; too far from current codebase |
| "Verification layer only" (no agent) | Not a coding agent; user wants an agent |

The chosen wedge (auditable + Windows + single binary) is the intersection of: (a) what Do Code already has, (b) what no competitor has, and (c) what enterprises pay for.

## 6. Beta Scope

### P0 — Beta blockers
- Tool-call agent loop (read_file, edit_file, run_command, search) — currently absent
- Streaming output — currently "stream": false
- Incremental edits (unified diff) — currently full-file JSON replacement
- Repo traversal + file reading — currently 96KB pre-pack with substring ranking
- Permission policy per tool (allow/ask/deny) — currently Deny/Execute only
- Session persistence + resume — currently inspection-only
- Real eval suite (20 tasks, nightly, pass rate tracked) — currently none
- Fix lying docs (README, STATUS.md, parity ledger)
- Ship binary + installer + 5-min quickstart + demo GIF
- AGENTS.md in repo root

### P1 — Beta differentiators
- Verification bundle (doc verify <repo> --diff <patch>) — signed, offline-verifiable
- Plain-language explain (doc explain <bundle> --level human|nontechnical)
- Cost tracking + hard spend cap
- Context compaction (summarize old turns)
- Headless/CI mode (doc exec -p "..." --output-format json)
- AGENTS.md / project instructions support
- Secret redaction wired to tool output
- Prompt-injection defence

### P2 — Useful later (defer)
- TUI (ratatui)
- OAuth / subscription auth
- MCP client
- Hooks
- Skills (agentskills.io)
- Subagents / parallel
- TUI, desktop, web surfaces
- Non-coder guided mode
- a11y / i18n / screen reader
- Open Evidence Standard
- Signed evidence bundles
- Eval suite on real tasks (Terminal-Bench)
- Repo map / tree-sitter

### P3 — Explicitly out of scope
- Multi-agent company simulation
- Game engine / PUBG / ChatGPT claims
- AGI claims
- Training own model
- Enterprise compliance (SOC2/SSO)
- Cloud long-running ticket→PR
- Plugin marketplace
- Desktop GUI
- Voice

## 7. What It Deliberately Does NOT Do (Yet)

- Does not stream (P0 fix planned)
- Does not run tests automatically (P0 fix planned)
- Does not edit incrementally (P0 fix planned)
- Does not support OAuth (P2)
- Does not have MCP (P2)
- Does not have TUI (P2)
- Does not support Linux/macOS sandbox (P2, Windows first)
- Does not have non-coder UX (P2)
- Does not have a public release or distribution (P0 fix planned)

## 8. Success Metrics

- **Primary metric:** Human minutes per verified change (target: < 5 min for a bug fix)
- **Secondary:** Defects per 1,000 changed lines, 30 days post-merge
- **Tertiary:** % of agent claims verified vs assumed (target: 100%)
- **Adoption:** 100+ GitHub stars, 10+ real users, 1+ external contribution in first 90 days
- **Benchmark:** Terminal-Bench resolution rate at fixed model + cost (published)

## 9. Rejected Alternatives

1. "Best general-purpose agent" — cannot out-feature Anthropic/OpenAI/Google/Cursor with 1 dev. Rejected.
2. "Fastest agent" (jcode's wedge) — jcode already owns this with published benchmarks. Rejected.
3. "Cheapest agent" — race to bottom, no sustainable business. Rejected.
4. "Non-coder agent" — no UX foundation, too far from current codebase. Deferred to P2.
5. "Verification layer only" (no agent) — not a coding agent; user wants an agent. Rejected as primary, kept as differentiator.
6. "Virtual company / multi-department agents" — product theater, no working execution loop. Parked.
7. "Build PUBG/ChatGPT" — impossible. Parked forever.

## 10. Risks and Falsification

**Decision could be wrong if:**
- Developers don't care about auditability (no demand signal yet)
- Windows-only sandbox limits adoption (Linux/macOS users can't use sandboxed execution)
- The evidence ledger is too heavy (performance overhead on every action)
- "Verifiable" is a feature, not a product (users want speed, not proof)

**Evidence that would change direction:**
- 10+ user interviews showing auditability is not a buying factor
- Terminal-Bench score below 30% at fixed model (harness not competitive)
- No external users after 30 days of public release
- Cost of evidence ledger exceeds 20% of total execution time

## 11. Open Questions

- Is the "verifiable agent" niche large enough for a sustainable business?
- Will Windows-only sandbox limit adoption to the point where Linux must come first?
- Can a non-coder founder sustain daily shipping?
- Will NLnet funding application succeed (deadline Jan 3 2027)?
- Is the evidence ledger's value proposition legible to non-enterprise developers?