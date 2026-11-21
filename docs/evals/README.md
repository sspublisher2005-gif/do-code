# Do Code — Evaluation design (M0 baseline)

**Status:** proposed design, not results. **Date:** 2026-11-21.
**Purpose:** turn "best / equal or better" from opinion into a measured number. Implements PDR Rule 1.

## Hard gates (release blockers)
- **False-success rate = 0.** Any task that reports success without executed passing evidence fails the release.
- No unauthorized write, network call, install, or credential read in default profile.
- Every claim carries a receipt: command, exit code, output hash, workspace/patch hash, timestamp.
- Blocked / not-run states are honest, never counted as pass.

## Benchmark: 30 fixed P0 tasks
- 10 bug fixes (repo fixture with failing test → fix → test passes).
- 10 small features (new behavior + acceptance test).
- 10 regressions (apply change → full existing suite still green).
Additional families later: context precision (labelled file set), memory precision (relevant vs stale note), resume (crash at each transaction boundary), security denial (path escape, secret read, network, child-process survival), cost/latency (provider-reported only; unknown stays unknown).

## Method (repeatable)
1. Fixed repo snapshot + commit per task; explicit acceptance test.
2. Fixed model/provider/version OR local fixture provider for offline runs; record which.
3. Budgets: max 2 repair attempts, max steps/wall-clock from `loopctl` defaults.
4. Run commands only through the qualified sandbox; capture receipt for every test run.
5. Record: task id, start commit, provider/model, tool trace, tokens/cost (or unknown), wall time, retries, final diff, test results, any false-success.
6. Compare against competitor runs only on same task, same repo commit, declared model; publish sample size and limits.

## Metrics
task success %; test pass %; regression rate; retries; median/p95 latency; tokens; provider-reported cost; **false-success rate**; safety violation count; reset/resume correctness.

## Scoring (equal-or-better claim table — to fill with measured results)
| Category | Do Code (date) | Best competitor (source/date) | Method | Still losing? |
|---|---|---|---|---|
| Task success | UNMEASURED | — | 30 tasks | unknown |
| False-success | UNMEASURED (target 0) | Claude #104504: 80/357 unbacked (2026-10-23) | receipt audit | likely winning if gate holds |
| Startup | old CLI ~1–4 ms (Linux 2026-10-26, not agent loop) | jcode 19.1 ms first input (vendor) | bench script needed | unknown |
| Windows sandbox | 10/10 subset (2026-11-08) | Codex WindowsRestrictedToken (source 2026-11-21) | 26-case matrix | not yet qualified |

No score may be published without method + date + limitations. Vendor speed claims are UNVERIFIED until reproduced.

## Implementation
`bench/agent_loop.rs` (planned): time-to-first-token, RSS, round trips. `evals/` runner: replay 30 tasks, write `evals/latest.json` with receipts and a `false_success` list. CI gates on the hard gates above.

## Status
No results yet. This file defines the test we must pass before calling M1 complete.
