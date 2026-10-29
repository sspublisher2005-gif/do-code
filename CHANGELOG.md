# Forge CLI — Changelog

## 27-29 October 2026 — Mission engine: loop control, contracts, teams, compounding (Windows-native session)

Implemented on native Windows x86-64 (Rust 1.99.1 MSVC). All gates pass locally: `cargo fmt --check`, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked` (115 unit + 9 integration), and a release build. The sandbox adversarial matrix remains NOT RUN (needs a disposable Windows runner); command execution stays disabled and `forge complete` stays fail-closed.

- Loop controller (`loopctl.rs`, `[loop]` config): bounded `while tool_use` mission loop — max steps, wall-clock budget, stall detection on repeated identical failures, pause only at safe boundaries; per-mission loop state persists so resume explains pauses and stall detection survives restarts.
- Spec-first contract gate (`contract.rs`): every run derives a mission contract (goal, success criteria, allowed/forbidden paths, required tests, security requirements, rollback plan) from the plan and context, stores it as evidence before any patch request, and rejects proposed paths the contract forbids. The coder cannot start without a valid contract.
- Shared task board + file locks (`taskboard.rs`): plans decompose into tasks with owners, per-file locks, dependency DAG, and acceptance criteria; lock conflicts fail closed before any model call; the board persists with the mission (`forge tasks`), and applied patches mark covered tasks done.
- TDD-first ordering retained and made explicit: the tester plan step runs before the coder, and required tests are frozen into the contract before implementation.
- Multi-model routing (`router.rs`, `[routing]` config): per-role `provider/model` routes with an ordered fallback chain; only provider transport failures route around (safety/consent errors fail closed); route switches are audit-logged.
- Risk engine + approval memory (`risk.rs`): 0-5 risk scoring for commands; risk 0-1 silent, risk 2 asks once and remembers at the chosen scope (once/mission/project), risk 3+ always asks fresh, risk 5 never auto-approved. `forge apply --approve --remember` stores a project-scope approval so later applies stop asking; `forge approvals show|forget` and `forge risk check "<command>"` make the policy transparent.
- Complexity-gated teams (`teams.rs`): Simple = architect+coder, Standard adds tester+reviewer, Complex adds security; staffing is automatic, recorded on the ledger, and shown by `forge agents`. Opt-in `--best-of-n N` requests N coder proposals and keeps the first that passes all safety guards (cost multiplies by N).
- Compounding engine + memory tiers (`compounding.rs`): hot (live mission ledger) / warm (lessons + playbooks) / cold (archive) tiers over plain git-friendly files in `.forge/memory/<scope>/`; missions propose lessons only via explicit `forge lessons propose`, and user-approved lessons become playbooks that brief future missions whose task text matches their keywords. Never auto-saved.
- Code-over-API workspace (`workspace.rs`): the coder works by writing real file contents for allowed paths; every applied mutation is recorded in an append-only per-mission journal (before/after hashes, checkpoint binding, reversibility) shown by `forge mission show`.
- Transport hardening: one retry for connection-reset/closed-before-response on provider requests (never on timeout or HTTP error status), and the loopback E2E fixture now drains the full HTTP request before responding, one thread per connection — removes an intermittent race.
- Honest completion unchanged: `forge complete` still requires trusted sandbox-verified evidence that is currently unobtainable; usage/cost display still reports provider-reported values only, `unknown` otherwise.

## 0.7.0-rs.1 — Forge CLI Rust implementation WIP

- Implement Forge CLI in Rust as the native CLI/core; preserve the TypeScript 0.6.0-ts.2 branch as a parity reference.
- Add a single-thread Tokio runtime configuration for this CLI entrypoint and trim Clap default features to avoid unnecessary terminal features.
- Bound per-directory name retention for workspace listing, cap each directory scan at 100,000 entries, and cap a search to 16 MiB aggregate source text.
- Tighten audit log mode enforcement through the opened file handle, enforce a regular-file check after opening, cap reads, and apply path redaction at the top-level and nested field levels.
- Remove predicted-cost budgeting and USD-cost auto-stop from the active Rust domain model and CLI. Keep only explicit provider-reported usage display.
- Add a feature-parity ledger and provisional multi-platform Rust CI workflow. Build/test results remain unverified until Cargo runs in a Rust-enabled environment.

- Confirmed the no-estimated-cost / no-dollar-auto-stop policy; only provider-reported usage is eligible for display.
- `doctor` now discloses that Windows `.forge` private ACL enforcement remains incomplete; opened-handle hard-link checks are authored but have not yet been compiled or verified on Windows.
- Rust build/test remains blocked until the pinned toolchain and Cargo dependencies can be resolved; no passing test claim is made.

- Port hash-bound mission persistence, sequential researcher/architect/coder/tester/reviewer/security role proposals, reviewable patches, explicit apply/checkpoint/undo, bounded local repository index/context packs, project/global user-confirmed memory, Decision Arena, evidence manifests, emergency freeze, and hash-chained mission transcripts. These are source-presence claims, pending Rust build/test validation.
- Add direct Anthropic and Google provider request shapes alongside OpenAI-compatible and Ollama-compatible APIs, subject to local-fixture tests and provider contract verification.
- Add Windows opened-handle hard-link count checks using `GetFileInformationByHandle`, with a Windows-gated test; Windows `.forge` ACL enforcement remains a release blocker.
- Align audit project IDs with the stable `.forge/project-id`, enforce canonical project UUID formatting, and check hard links on the opened config handle.
- Keep command/build/test/browser execution disabled until a separately reviewed sandbox is implemented and negative isolation tests pass.


## 26 October 2026 — Verification and supply-chain tooling

- Added a source inventory generator/validator so checked-in source hashes can be enforced as a CI gate.
- Added a preliminary offline CycloneDX SBOM generator with focused tests; license and vulnerability status remain explicitly unknown pending trusted release scanning.
- Added a pinned RustSec GitHub Actions workflow for dependency changes and scheduled scans; workflow execution is not yet evidenced in this handoff.
- Extended CI to run the 37-case offline workflow suite, SBOM checks, and Linux peak-RSS measurement alongside existing runtime checks.
- Added reproducible Linux RSS measurement script and current baseline report.
- No Rust runtime implementation was changed in this continuation because this environment has no Rust toolchain; the sandbox and feature parity release blockers remain open.

## 26 October 2026 — CI reproducibility and Cargo-aware SBOM

- Added a pinned Cargo-aware CycloneDX 1.5 SBOM generation/validation wrapper and eight network-free validator/orchestration tests.
- Added a required Cargo-aware SBOM step to the Linux/Rust 1.99.1 CI job and kept license-field coverage separate from legal/license clearance.
- CI now requires a checked-in reviewed `Cargo.lock` rather than silently generating one, and uses `cargo test --all-targets --locked`.
- These script-level changes were syntax checked and the SBOM helper tests passed; no Rust build or actual cargo-cyclonedx run is claimed from the offline authoring environment.
- Added a fail-closed required-gate evaluator plus seven regression tests after finding that a renamed Cargo test step could otherwise evade the pipeline's final failure filter.
