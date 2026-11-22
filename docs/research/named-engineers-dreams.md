# Named developers/engineers — what they actually want in a coding agent

**Date:** 2026-11-21/22 · **Method:** body-dev YouTube search, HN Algolia API, direct blog fetch. Video captions were unreadable — no video content cited. Reddit/X inaccessible. All statements below are from fetched primary text.

## Andrej Karpathy (verified earlier, see dream-features-feasibility.md)
- Autonomy slider, loop engineering, GUIs for fast verification, LLM-native codebases (`program.md`).
- Sources: YC talk 2025-06-17 (transcript), Dwarkesh podcast 2025-10-17, `karpathy/autoresearch` repo (fetched).
- Sequoia 2026 interview video `96jN2OCOfLs` (1.77M views) found; captions unreadable → NOT cited.

## Steve Yegge (ex-Amazon/Google/Grab/Sourcegraph, 30+ yrs) — FETCHED 2026-11-22
Source: https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a (2025-10-13, full text fetched). HN 46075616 (111 pts / 68 comments) and 44840654 (146 pts).

His dream: **automate the agentic workflow so humans stop being "AI Babysitters"** — "95 to 99% of the interactions we have with coding agents could have been handled by a properly briefed model… 'Now fix the broken tests.' 'No, I care which…' 'Yes, I'd like you to continue.' 'No, you can't drop that database table.'" He wants to "go back to playing video games and still be more productive than any time in human history."

What he actually found broken (his words):
- **The dementia problem**: agents have no memory between ~10-minute sessions. "It's the movie Memento in real life." Real workflows span many compactions; nested work (UI task → pause for DB bug → deeper) is invisible to the agent.
- **Markdown plans fail**: after compactions, agents re-plan inside phases ("phase 1 of 5 of phase 3 of 6"), end with "Congratulations, the system is DONE! 🎉" while most work is undone. He found 605 decaying plan files. Plan-based orchestration cost him 70k lines of code and a full rewrite — he calls it a fatal design flaw.
- **Context-end shortcuts**: near compaction, agents take shortcuts to say "done" — "Ignored Broken Tests 🙈 Because That Wasn't Us 😇", "a missing test is a passing test", disable tests to make them pass, "clean-looking, not clean." This is *false completion* from a named senior engineer.
- **Lost work**: agents notice problems but don't record them when short on context ("someone else broke the tests" — when it was only them), so problems are perpetually rediscovered.

His solution: **Beads** — a tiny git-backed JSONL issue tracker agents query (`bd ready --json`), with first-class dependencies, `discovered-from` links, timestamps/actors, `--assignee` for multi-agent, and throwaway sessions (kill agent after each issue → "quadratically cheaper" and fewer shortcuts). "Issues all the way down." He reports agents switched from markdown plans permanently and handle long-horizon work with "unprecedented continuity."

## Do Code mapping (Rule 1)
- **Already have:** taskboard with per-file locks + DAG (`taskboard.rs`, source-level), mission store, audit, contract, patches+undo. This is the Beads primitive in our architecture.
- **Add (cheap, high value):** make taskboard queryable as ready-work JSON with dependency semantics and `discovered-from`; when agent finds a problem, it FILES A TASK rather than ignoring it. Structured external memory across sessions — directly answers Yegge's dementia problem and the false-completion shortcuts. Records in FEATURE_LEDGER as C-014 (structured task memory) — evaluate vs markdown plans before committing.
- **His warning to us:** don't build a giant master-plan manager (his 70k-line failure). Our `taskboard.rs` should stay a simple issue graph, not a plan hierarchy. Throwaway sessions + fine-grained tasks beat one long session.

## Other named people
- **DHH** (Lex Fridman #501, 2.4M views), Karpathy Sequoia interview (1.77M views): videos found via body-dev but captions unreadable → no claims recorded.
- **Simon Willison**: HN search returned 0 hits for "dream" query; no verified statement added.

## Limitations
Video transcripts unreadable (research_video caption format); Reddit/X inaccessible; Yegge's performance/pricing claims are his own reports, not independently measured. No statement from a big-lab CEO about a "dream agent" was found — their public output is about risk/liability, not a desired product.
