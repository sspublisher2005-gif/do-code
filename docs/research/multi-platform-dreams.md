# Multi-platform research — what developers/leaders/users dream in a coding agent

**Date:** 2026-11-22 · **Platforms actually used:** X (body-dev eyes_see_browser — SUCCESS), HN (Algolia API + item pages), official blogs/docs, GitHub. **Blocked:** Reddit (CAPTCHA "Prove your humanity" on every thread), video captions (unreadable on research_video), some blogs rate-limited/auth-wall. No fabrication.

## 1. X/Twitter — VERIFIED posts (body-dev, 2026-11-22)
- **Karpathy, "2025 LLM Year in Review"** (https://x.com/karpathy/status/2002118205729562949, fetched): RLVR is the new major stage; objective **non-gameable verifiable rewards** drive reasoning; "Jagged Intelligence" — models are "ghosts, not animals", so you must design around uneven capability. → For Do Code: verification must use non-gameable checks (our hard-to-fool verifier + receipts).
- **Karpathy on nanochat** (https://x.com/karpathy/status/2029701092347630069, fetched earlier via agent, X page confirmed 2026-11-22): "AI Agents iterating on nanochat automatically… 110 changes over the last ~12 hours… the agent works on a feature branch, tries out ideas, merges them when they work and iterates… I've iterated more on the 'meta-setup' where I optimize the agent flows even more than the repo directly." → Dreams: **autonomous keep/revert loop on a branch, meta-harness tuning**, which is exactly autoresearch's LOOP FOREVER.

## 2. Hacker News — VERIFIED threads
- **Karpathy YC talk** (HN 44315096): moderator `dang` singled out "autonomy slider" as the standout insight. Confirms the dream.
- **Ask HN: do you still maintain CLAUDE.md / AGENTS.md?** (HN 48160604, 2026-05-16): users doubt behavior rules are followed ("not surprised you don't expect the rules there to be followed"), but agree **facts** (project dirs, doc refs, commands) always help. → Dreams: a project file that actually works for facts, not wishful behavior rules.
- **Loop Engineering / skills / verification** (Developers Digest 2026-04-18, updated 2026-06-07, fetched): "workflows beat demos, skills beat prompts, orchestration beats autonomy, and verification is the bottleneck"; repeated HN convergence on context preservation over long sessions, inspectable plans, and task-shape-dependent tool quality (no single agent dominates all categories).
- **Claude Code reverse engineering** (HN 44214926, fetched full): boundary-hopping via Python/interpreter, `.env` read via alternate tools, "find /" escalation, cost complaints.
- **Context rot issues** (#17428 117 reactions, #27242 92) and **false success** (HN 50525329, Claude official blog) — already in wishlist-and-painpoints.md.

## 3. Named engineers — VERIFIED
- **Steve Yegge** (full Medium article fetched): dementia problem, markdown plan failure (605 decaying plans, 70k LOC lost), context-end shortcuts ("clean-looking, not clean"), solution = Beads (git-backed JSONL issue graph, `bd ready --json`, throwaway sessions). Already in named-engineers-dreams.md.
- **Karpathy** (X + YC talk + autoresearch): autonomy slider, loop engineering, GUI verification, program.md; "iterate on the meta-setup, not just the repo".

## 4. Blogs / community (fetched)
- aibuilderclub "Loop Engineering, Karpathy-Style" (fetched, rate-limited after head): course outline shows the field's consensus curriculum — memory systems (episodic/semantic/procedural), multi-agent orchestration, MCP security (6 attack vectors), "why MCP is dead & skills replace it", context engineering as the real bottleneck, Plan/Default/Auto modes, **Agent Sandbox: OS-Level Security**, "Loop Engineering: Write Verifiers, Not Prompts", graph vs loop engineering, compound engineering. → Dreams: verifier-first loops, skills over prompts, OS-level sandbox, structured memory.
- Developers Digest: same convergence (above).

## 5. Unified "dream" list (what they actually want, deduplicated)
1. **Verifier-first loops** (non-gameable checks, receipts, no false "done") — strongest consensus, from Karpathy/Yegge/HN/Claude's own blog. **We have the seed (evidence gate) — build receipts + repair.**
2. **Structured external task memory** across sessions (issue graph, ready-work queries, discovered-from) — Yegge's Beads, our taskboard foundation. **Build ready-work JSON + "file a task when you find a problem".**
3. **Throwaway fine-grained sessions** instead of one long drifting session — Yegge/Karpathy. **Support resume; keep tasks small.**
4. **Skills over prompts; project facts file that actually works** (CLAUDE.md/AGENTS.md/DOCODE.md facts, not behavior rules) — HN 48160604, aibuilderclub. **We have DOCODE.md + memory tiers — strengthen facts, don't rely on rules.**
5. **OS-level sandbox that fails closed** — aibuilderclub, Pillar, our own build. **Qualify Windows matrix.**
6. **Autonomy slider / modes** (suggest → scoped edit → supervised background → unattended) — Karpathy. **Config-level, not a marketed feature.**
7. **Portable sessions/memory, no lock-in** — HN 50182883. **P2.**
8. **Model-agnostic routing + meta-harness tuning** — Karpathy's "iterate on meta-setup", OpenCode. **Config tiers.**
9. **GUI for fast human verification** — Karpathy, CodeRabbit. **Principle only; no custom GUI build.**
10. **Honest long-horizon handling** (many small verified chunks + handoff, not 1000 decisions) — METR. **Our loopctl + journal.**

## 6. What is NOT a dream (rejected with evidence)
Plan-based master orchestration (Yegge: 70k LOC lost) · behavior rules in CLAUDE.md that agents ignore (HN 48160604) · 1000-decision coherence (METR) · "single best model wins" (Developers Digest: no agent dominates all task categories) · crypto audit trails / SCITT compliance (no demand).

## 7. Platform honesty
Reddit: CAPTCHA-blocked on both r/ClaudeAI and r/LocalLLaMA threads — **no Reddit content used**. X: two Karpathy posts verified, one attributed via agent. Videos: captions unreadable, no video claims. aibuilderclub: only head fetched before rate limit. Everything above is either fetched text or earlier verified research; no invented quotes.

## Plan impact
None to the frozen PDR. These dreams map 1:1 to FEATURE_LEDGER items A-006..A-015 (receipts/sandbox/verifier), C-007/C-008/C-009 (memory/facts/program.md), B-004 (repair), G-008 (autonomy). All already ranked. Next remains M1.
