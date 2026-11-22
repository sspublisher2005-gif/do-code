# Reddit findings — user-provided (AI-generated summaries)

**Date:** 2026-11-22 · **Provenance:** user pasted Reddit's own AI-generated overviews of threads from r/codex, r/ClaudeAI, r/LocalLLaMA, r/LocalLLM, r/selfhosted, r/codex. Reddit itself labels these "AI-generated from posts and comments and may not be accurate." Treat as **signal, not verified primary quotes**. Quotes below are as surfaced by the summary; not checked against raw threads (Reddit is network-blocked here). Cross-check any claim before using it as evidence.

## Themes (late-2026 snapshots)

### 1. Switching behavior — users churn by task and limits
- "Switched from Codex to Claude Code on the same $20 plan, and I'm blown away" (r/codex, 1.3K votes/461 comments) — big quality difference reported on one plan.
- Common pattern: **"Claude for implementation, Codex for review"** / "Codex as my workhorse and Claude as a reviewer or co-analyst" — multi-model division of labor by role.
- Codex strengths cited: **cached sessions** (restart after interruption costs Claude "10-35% of the 5h budget"); remote management.
- "Today Claude wins, I remember a few months ago when everyone was moving to Codex" — comparisons flip with models/limits. Snapshot, not settled verdict.

### 2. Non-coder users exist and struggle (validates our target user)
- "I'm not a programmer. I run a small project with a few Claude Code agents on my own server, plus one 'main' session I talk to all day." — real non-coder running agents. Their problem: long sessions forget "what I said word for word, what I forbade, what they promised to check."
- "56 compactions in 5 days" thread — non-programmer trying to stop forgetting.

### 3. Long autonomous tasks — the harness question
- r/LocalLLaMA (53 votes/93 comments): best harness for days-long dev/test/update loops. Answers: Pi (lightweight, "let go of the fluff and bloat"), OpenCode ("most hands off"), **Hermes stayed on task through 11 compactions** (one user's report), DSH good with Qwen models.
- Recommendation pattern: **task ledger + progress file + separate worker assignments** so agents resume after context resets. "progress file keeps track of what is implemented and what is in the queue for the plan."

### 4. Context loss — the dominant complaint
- "The failure mode is not the switch itself. It is losing the 'what we already decided' file and re-teaching the next tool from zero."
- "The note file does more for that than another subscription tier has done for me."
- "The owner's words come first, verbatim. A summary of what I said is always a little wrong." → **store critical instructions verbatim, not summarized.**
- "The 'why' is what survives a compaction badly, so it's the part I write down on purpose." → keep rationale + rejected options, not just decisions.
- Practical pattern: finish a small unit → write handoff → compact; search old transcript and paste back only the lost lines (not the whole history).
- Compaction controls vary by environment (Claude Code vs CoWork) — no universal setting.

### 5. Local models — why users go local
- Reasons: privacy/dependence on online services; "massive token count that would blow my online [limit]".
- Goose valued for "a fairly small system prompt and thus helps when context limits are an issue"; Pi lean; model compatibility matters more than harness branding.
- "These are all just harnesses and don't tell us anything at all" (criticism of narrow benchmarks) — supports our honest-eval stance.

## Cross-check with verified research
- Confirms: context rot (#1 pain), non-coder user segment, model-by-role routing, task-ledger/progress-file pattern (Yegge's Beads, our taskboard), small-verified-chunks, portable decisions over subscription tiers (portability gap).
- No contradiction with HN/X/Yegge findings. Adds: **multi-model-by-role as a normal user workflow**, and **session caching/restart cost as a switching factor**.

## Do Code mapping (Rule 1)
| Reddit signal | Our item | Action |
|---|---|---|
| Requirements lost / "verbatim, not summary" | A-012 invariant ledger | store safety/user rules verbatim outside compaction; verify after compaction |
| Progress file + task queue + worker assignments | C-003 taskboard, C-014 structured task memory | ready-work JSON + discovered-from; file a task when a problem is found |
| "Claude impl, Codex review" | D-003 role-tier routing | already HAVE role routing; config tiers (Module 2) |
| Session caching / cheap resume | B-008 resume at boundary | make resume preserve session state, no re-teach from zero |
| Non-coder runs agents all day | G-003 guided mode | P2; guided setup + plain-language plan + handoff notes |
| Small system prompt helps context | B-007 context hygiene | frozen prefix + tool result clearing |

## Honesty
This is third-hand (Reddit AI summary → user → us). Every claim above is labelled as user-provided AI summary, not a verified quote. Do NOT present as "Reddit users said X" in external docs without the provenance label. Reddit direct access remained blocked (403).
