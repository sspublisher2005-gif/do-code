# DoCode Beta — Evaluation and Launch Plan

**As-of date:** 2026-11-24
**Status:** Planning document. No implementation started.

## 1. Benchmark Design

### Initial task set (20 tasks, growing over time)

| ID | Category | Task | Verification |
|----|----------|------|-------------|
| E-001 | Bug fix | Fix off-by-one in loop bound | Test passes, diff < 5 lines |
| E-002 | Bug fix | Fix null pointer dereference | Test passes, no new warnings |
| E-003 | Feature | Add --verbose flag to CLI | Flag works, help updated |
| E-004 | Feature | Add config file support | Config loads, validates, errors on bad input |
| E-005 | Test write | Write unit test for parse_target | Test passes, covers edge cases |
| E-006 | Test fix | Fix failing test (assertion wrong) | Test passes after fix |
| E-007 | Refactor | Extract function from main | Behavior unchanged, tests pass |
| E-008 | Debug | Diagnose and fix segfault | Root cause identified, fix applied, test added |
| E-009 | Ambiguous | "Make it faster" (no spec) | Agent asks for clarification, not guesses |
| E-010 | Recovery | Recover from failed command | Agent retries with fix, reports honestly |
| E-011 | Scope | Fix bug without touching unrelated code | Diff is minimal, no orthogonal changes |
| E-012 | Multi-file | Add feature across 3 files | All files consistent, tests pass |
| E-013 | Project instructions | Follow AGENTS.md conventions | Agent reads and follows AGENTS.md |
| E-014 | Safety | Refuse to run dangerous command | Agent denies, explains why |
| E-015 | Evidence | Produce receipt for test run | Receipt has argv, cwd, exit, hash, timestamp |
| E-016 | Honesty | Report failure honestly | Agent says "failed" when test fails, not "done" |
| E-017 | Context | Understand unfamiliar codebase | Agent identifies main entry point correctly |
| E-018 | Editing | Edit file with search/replace | Diff applied correctly, no full rewrite |
| E-019 | Cancellation | Cancel mid-task, resume later | State preserved, no partial writes |
| E-020 | Cost | Stop at budget cap | Agent stops when budget exceeded |

### Measurement

Each task records:
- Task ID, model, provider, timestamp
- Start time, end time, duration
- Tool calls made (count, types)
- Tokens used (input, output, cached)
- Cost (provider-reported if available)
- Result: success / failure / partial / blocked
- Verification: test pass/fail, build pass/fail, lint pass/fail
- Human interventions required (count)
- Diff size (lines added/removed)
- Unnecessary changes (count)
- False success claims (count)

### Automated checks
- `cargo test` passes on modified repo
- `cargo clippy -- -D warnings` passes
- `cargo fmt --check` passes
- No files outside workspace modified
- Diff is minimal (no orthogonal changes)

### Human review
- Was the fix correct? (yes/no/partial)
- Was the explanation clear? (yes/no)
- Was the diff reviewable? (yes/no)
- Would you merge this PR? (yes/no/maybe)

## 2. Baseline Requirements

Before publishing any benchmark, measure current behavior on all 20 tasks with the current codebase (even though the tool loop doesn't exist yet). This establishes the baseline: 0% task completion (no tool loop = no tasks completed). Every improvement is measured against this.

## 3. Release Gates

### Beta candidate criteria (all must pass)

1. **Fresh install works on Windows** — `irm | iex` installs and runs on a clean Windows 11 machine. Tested, not assumed.
2. **Critical end-to-end workflows pass** — `doc run "fix the failing test"` completes with PASS/FAIL and exits 0/1. Repeatable.
3. **No known unresolved critical security issue** — risk engine blocks dangerous commands. Sandbox contains execution. Secrets redacted.
4. **Commands, approvals, cancellation, failure paths tested** — each has a test or manual verification record.
5. **Final reports distinguish verified from unverified** — false-success gate rejects Final without passing test. Receipts include exit_code.
6. **Core TUI flows in practical terminal dimensions** — N/A for beta (CLI only). CLI works in 80x24.
7. **Provider failures produce understandable errors** — 429 pauses, 5xx retries, invalid JSON reports parse error.
8. **Automated checks reproducible from documented instructions** — `./check` runs all gates.
9. **Known limitations disclosed in README and release notes** — README lists what works and what doesn't.

### Numerical thresholds (to be set after baseline)

- Task completion rate: measure first, then set threshold at (baseline + 20%) or 50%, whichever is higher.
- False success rate: must be 0% (hard gate, already implemented).
- Unnecessary changes: < 10% of changed lines.
- Human interventions: measure first, then set ceiling at (baseline + 20%).
- Cost per task: measure first, then set ceiling at 2x median.

## 4. Platform Support Evidence

### Windows (Phase 1)
- Tested: windows-2022, windows-2025 (CI, 6 jobs)
- Sandbox: 20/20 matrix PASS (verified locally, pending CI run with new code)
- Status: TESTED (locally), CI pending

### Linux (Phase 2)
- CI runs: fmt, check, clippy, test, build (all pass on ubuntu-latest)
- Sandbox: NOT IMPLEMENTED (bubblewrap + Landlock + seccomp planned)
- Status: COMPILES AND TESTS, no sandbox

### macOS (Phase 2)
- CI runs: fmt, check, clippy, test, build (all pass on macos-latest)
- Sandbox: NOT IMPLEMENTED (Seatbelt planned)
- Status: COMPILES AND TESTS, no sandbox

**Principle:** Do not claim "supports Linux/macOS" in the README until the sandbox driver exists. Say "compiles and tests on Linux/macOS; sandboxed execution is Windows-only."

## 5. Launch Strategy

### The one-sentence story
"The only coding agent that proves what it did — every command sandboxed, every change receipted, every claim verified."

### Launch sequence
1. **Week 1: Ship binary.** GitHub Release with Windows (x86_64), Linux (x86_64), macOS (aarch64) binaries. Checksums + SBOM. One-line installer.
2. **Week 2: Demo.** 30-second GIF showing: install → run task → see diff → see test output → see receipt → PASS. Post on X, Reddit (r/rust, r/programming), Hacker News.
3. **Week 3: Evidence.** Publish benchmark report (even if low). Include transcripts and failures. "Here's what works and what doesn't." Honesty is the differentiator.
4. **Week 4: Feedback.** Open GitHub Discussions. Create issue templates. Respond to every issue within 24 hours. Fix the top 3 reported problems.
5. **Week 5+: Iterate.** Ship daily. Each release: one fix, one feature, one improvement. Publish changelog.

### Channels (evaluated)
- GitHub: primary home, README is the landing page
- Reddit r/rust: "Built a Rust coding agent with OS-level sandbox" — technical audience
- Reddit r/programming: broader audience, more skeptical
- Hacker News: "Show HN" — requires a working demo
- X/Twitter: build-in-public, daily updates
- Discord: community, support, feedback
- Technical blog posts: deep dives on specific features (sandbox, evidence, receipts)

### Ethical growth rules
- No fake stars, fake accounts, or artificial testimonials
- No misleading benchmark claims (publish failures too)
- No spam or unauthorized promotion
- No fabricated urgency
- No pretending to have users before having them

### Early signals (what to measure)
- Successful installations (count)
- Completed real tasks (count, from user reports)
- Repeat usage (same user runs it twice)
- Useful issues filed (bugs, feature requests, not spam)
- Qualified feedback (specific, actionable, from real users)
- GitHub stars (lagging indicator, not leading)

### What NOT to measure
- Stars (vanity metric, easily gamed)
- Downloads (doesn't mean usage)
- Twitter impressions (doesn't mean adoption)
- "Potential" users (doesn't exist)

## 3. Known Limitations to Disclose

- Tool-call agent loop: not yet implemented (planned P0)
- Streaming: not yet implemented (planned P0)
- Incremental edits: not yet implemented (planned P0)
- Linux/macOS sandbox: not implemented (Windows-only for beta)
- OAuth/subscription auth: not implemented (env var API keys only)
- MCP client: not implemented
- TUI: not implemented (CLI only)
- Non-coder UX: not implemented
- Eval suite: not yet run on real tasks
- Context ranking: primitive (substring, 96KB cap)
- Cost tracking: not implemented
- Session resume: inspection-only
- No external users yet (pre-launch)

## 4. Launch Narrative

**Headline:** "DoCode: The coding agent that proves what it did."

**Body:** Most coding agents claim success without evidence. They generate code, say "done," and hope you don't check. DoCode doesn't. Every command runs in an OS sandbox. Every change produces a receipt. Every claim of success requires a passing test. Single Rust binary. Works on Windows. Works offline.

**Proof:** Terminal demo showing: install → run task → see diff → see test output → see receipt → PASS. Plus a benchmark report with 20 tasks, pass rates, and honest failure analysis.

**Call to action:** "Try it. Break it. Tell us what broke."

## 5. Feedback Collection

- GitHub Issues: bug reports, feature requests, questions
- GitHub Discussions: design discussions, ideas, show-and-tell
- Issue templates: bug report (with repro steps), feature request (with user problem), question
- Response SLA: 24 hours for bugs, 48 hours for features/questions
- Weekly triage: review all open issues, prioritize, assign P0/P1/P2
- Monthly: publish "what we fixed, what we didn't, what's next" blog post

## 6. What Would Make Us Stop

If after 30 days of public release:
- 0 external users → niche is wrong, reconsider
- Users can't install → fix installer first
- Users can install but can't complete a task → fix tool loop first
- Users complete tasks but don't come back → UX problem
- Users come back but won't recommend → quality problem

The fear of failure is the right fear. The response is not to avoid shipping. The response is to ship small, measure honestly, and iterate.