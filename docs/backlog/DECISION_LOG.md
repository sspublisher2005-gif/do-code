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

## Update procedure
Append a line here with: `YYYY-MM-DD | decision | reason | evidence (commit/file:line) | approver`. Never edit history. If a decision is reversed, append a new line that supersedes it by ID/date.
