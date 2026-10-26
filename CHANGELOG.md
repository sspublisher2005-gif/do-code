# Forge CLI — Changelog

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
