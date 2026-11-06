# Do Code (`docode`)

**Do Code** is an evidence-first AI coding agent: it reads your repository, plans with your consent, proposes reviewable patches, and refuses to claim anything works without real evidence. The CLI (`docode`) ships today; a desktop app (like OpenCode's CLI + desktop pairing) is planned on top of the same engine.

**Repo:** [github.com/sspublisher2005-gif/do-code](https://github.com/sspublisher2005-gif/do-code) (public) · **Status:** feature-rich prototype, developed and gate-verified on native Windows x64 (Rust 1.99.1 MSVC, MSRV 1.88).

**Verified on the exact current source:** `cargo fmt --check` clean · `cargo check --locked` clean · `cargo clippy -D warnings` clean on **both 1.88 and 1.99 toolchains** · **126 unit + 10 integration tests pass** · release build OK · CycloneDX 1.5 SBOM (200 components, 100% license fields) · `cargo audit` exit 0 (zero vulnerabilities).

**Honest limits (do not remove):** the production execution sandbox has primitives implemented (AppContainer + restricted token + Job Object) but the 26-case adversarial acceptance matrix has NOT run yet — command execution stays disabled and `docode complete` stays fail-closed until that matrix passes on a disposable runner. Do not claim production readiness from local gates.

## Product name history (kept for provenance)

The project began as "Forge CLI" (the name was taken elsewhere); the TypeScript 0.6.0-ts.2 reference remains at `reference/Forge_CLI_TS_0.6.0-ts.2_MemoryDecision_Merge_WIP.zip` for parity work and rollback. Every user-visible surface now says **Do Code**; the binary is `docode`; the crate id stays `docode-cli` for lockfile/SBOM stability.

## Why Rust

The Rust implementation is intentional: a compiled single CLI executable avoids a Python or Node.js runtime dependency. `clap` is used for the command parser, and the Tokio runtime is configured for a single thread because the current CLI only needs bounded async provider diagnostics. This is an architectural choice, not a comparative performance claim. The included Linux binary is about 8 MB and startup/command timings are measured only on one warm-cache Linux runner; no comparison to TypeScript or RSS benchmark is available. Official references: [Cargo installation](https://doc.rust-lang.org/stable/cargo/getting-started/installation.html), [Clap](https://docs.rs/clap/latest/clap/), [Reqwest](https://docs.rs/reqwest/latest/reqwest/).

## Implemented in the supplied Rust source revision (feature parity incomplete; this environment could not rerun compilation)

The current source tree now contains more than the initial CLI skeleton:

- Native `forge` commands for `init`, `doctor`, `config`, `files`, `audit`, `index`, `context`, `agents`, `plan`, `run`, `mission`, `apply`, `verify`, `resume`, `undo`, `freeze`, `unfreeze`, `sandbox`, `memory`, `pilot`, `decisions`, `evidence`, `model test`, `tasks`, `contract`, `lessons`, `approvals`, and `risk`.
- Strict typed TOML configuration, local initialization without network use, privacy defaults, environment-variable-based keys, and URL/provider policy checks.
- Bounded read-only file access, sensitive-path exclusion, redaction at terminal boundaries, local inter-process audit logging, project identity and private state directories.
- Hash-bound mission state, typed steps/transitions, local evidence bytes/manifests, repository index and bounded context packs with a prioritized (but untrusted) project `FORGE.md`.
- Consent-gated sequential role workflow (researcher, architect, coder, tester, reviewer and security). DevOps/build execution remains blocked; the workflow records that step as blocked rather than pretending it ran. Coder output is a proposal only; role perspectives are not independent experts and no shell/build/test/browser execution is performed.
- Reviewable replacement patches for existing text files, explicit approval, checkpoints, hash-conflict checks and conflict-aware undo.
- Local, user-confirmed Learning Memory with project/global separation and list/search/show/edit/forget/verify/reindex/export/import/reset operations.
- Offline and consented live Decision Arena briefs with integrity-checked local records. No repository content or memory is attached to Decision Arena prompts.
- Provider request shapes for OpenAI-compatible and Ollama-compatible services, Anthropic, and Google; requests require explicit consent, disallow redirects, have timeouts/response bounds, and display only provider-reported usage.
- Regression tests and cross-platform CI/test scripts are authored for Linux/macOS/Windows. None has been executed by Cargo in this environment.

These source-presence claims must be read alongside the retained evidence. The supplied build logs report 70/70 Rust tests, passing Clippy, and a successful release build. Independent checks of the supplied Linux binary passed 24/24 black-box cases plus a loopback mission-lifecycle E2E. This checker could not rerun Cargo. The native sandbox/command runner is not implemented; build/test/shell/browser execution remains blocked and missions cannot truthfully be marked complete without executed test evidence. Resume is inspection-only; memory proposal preview/open, patch add/delete/rename, and complete `.gitignore` semantics are not ported. Windows opened-handle hard-link code is in source but not runtime-verified; Windows private ACL enforcement remains incomplete. See `docs/FEATURE_PARITY_LEDGER.md`, `docs/TEST_RESULTS_2026-10-26.md`, `docs/SANDBOX_DESIGN_AND_TEST_SPEC.md`, and `docs/SANDBOX_ACCEPTANCE_MATRIX.json` for reconciled status, platform design and test gates. The sandbox docs are specifications, not proof of an implemented sandbox.

## Mission engine (loop control, contracts, teams, compounding)

Missions now run through a bounded loop controller (`[loop]` config): a maximum step count, a wall-clock budget, and stall detection that pauses at safe boundaries when the same failure repeats — it never kills in-flight work. Before the coder starts, every mission derives and stores a spec-first contract (goal, success criteria, allowed/forbidden paths, required tests, security requirements, rollback plan) and rejects any proposed path the contract forbids. Plans decompose onto a shared task board with per-file locks and dependency checks (`forge tasks`). Role calls route through per-role model routes with an ordered fallback chain (`[routing]`), switching only on provider transport failures and audit-logging every switch. Teams are complexity-gated automatically (simple = architect+coder; standard adds tester+reviewer; complex adds security) with opt-in `run --best-of-n N` for hard tasks. A 0-5 risk engine gates actions: silent for low risk, ask-once-and-remember for reviewable actions (`forge apply --approve --remember`), always-ask for dangerous ones, never-auto-approved for critical ones (`forge risk "<command>"` shows the policy; `forge approvals show|forget` manages memory). The compounding loop turns completed missions into user-approved playbooks (`forge lessons propose|list|approve|reject|archive`) that brief future missions with matching tasks; memory is tiered hot/warm/cold over plain git-friendly files. Every applied change is recorded in a per-mission mutation journal with before/after hashes and checkpoint binding (`forge mission show`).

## Cost policy (explicit product decision)

Forge does **not** predict prices, calculate an estimated cost before a request, or stop missions because an estimated or configured USD budget was reached. This includes no `--max-cost-usd` and no estimated-cost Decision Arena trigger. The CLI may report usage/cost returned by a provider after a request; if a provider does not report a verified USD cost, display `unknown`. It must not guess price or pretend zero cost. Provider/account quotas and provider errors are handled normally. Output-token caps, request timeouts, response-size limits, cancellation, and security/resource limits remain because these are execution-safety controls, not price predictions.

## Build and test

Requires Rust/Cargo stable. Install using the official rustup method from the [Cargo Book](https://doc.rust-lang.org/stable/cargo/getting-started/installation.html).

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
./target/release/forge --version
./target/release/forge --help
```

The CI workflow spans Ubuntu, macOS, and Windows on the declared MSRV (Rust 1.88.0) and the current pinned compiler (Rust 1.99.1). Its Python test harness is for CI only: it exercises the real compiled executable, writes a JSON report, and records toolchain-setup/build durations plus repeated-process startup and representative command latencies. It never calls a live model. Toolchain setup duration may be affected by preinstalled/cached components and is not guaranteed to represent a clean download. The supplied Linux binary has been runtime-tested. The cross-platform matrix still has not run; workflow configuration does not establish OS support.

For static checks when Cargo is unavailable, run `python scripts/static_preflight.py --archive ./forge-cli-source.zip --binary ../binaries/forge-linux-x86_64 --report static-preflight.json`. The Python script verifies manifests/config TOML, CI YAML shape, harness syntax, selected source markers and ZIP integrity; it is not a Rust parser/compiler or security certification. The current project includes the real `Cargo.lock` supplied by the build run. In this independent authoring environment, `rustc`/`cargo` are absent and the distribution hosts cannot be reached; use the supplied logs as provenance and rerun all locked build/test gates on an actual Rust host. See `docs/FEATURE_PARITY_LEDGER.md`, `docs/TEST_RESULTS_2026-10-26.md`, and `docs/CROSS_PLATFORM_TEST_PLAN.md`.

## Basic usage

```bash
forge init
forge doctor
forge config show
forge files list
forge files read src/main.rs
forge files search TODO src
forge audit --tail 20
forge model test                  # no network
forge model test --live --approve-context-sharing  # explicit diagnostic request
```

`model test --live` sends only a fixed diagnostic prompt; it never attaches project source, `FORGE.md`, `.forge` state or memory. Live provider calls can incur provider charges and are subject to the configured provider's retention policies.

## Feature parity roadmap

`docs/FEATURE_PARITY_LEDGER.md` is the refreshed command/module comparison against TypeScript. `docs/RUST_PARITY_LEDGER.md` is now only a compatibility pointer. Do not delete the TypeScript reference or call the Rust line production-ready until build logs are complete, feature parity is resolved, all required tests and OS matrix evidence exist, sandbox review passes, and release gates pass.


## Latest test status (2026-10-26)

See `docs/TEST_RESULTS_2026-10-26.md` for independently repeated Linux results and explicit limits; `docs/DEEP_TEST_REPORT_2026-10-24.md` is historical context. The supplied Cargo log reports 70 tests passing in Claude’s Rust environment. The independent environment did not re-run Cargo; it ran the compiled binary’s 24-check black-box suite and local fixture lifecycle, both passing. Warm-cache start/command timings exist, but install duration, cold-cache behavior and RSS are unmeasured.


## Latest reproducibility/supply-chain update — 26 October 2026

CI now requires a reviewed `Cargo.lock`, uses `cargo test --all-targets --locked`, and has a pinned `cargo-cyclonedx` 0.5.9 wrapper for a Cargo-aware CycloneDX 1.5 SBOM in the Linux/Rust 1.99.1 job. Eight wrapper/validator tests pass locally, including a mocked orchestration test. **The plugin itself has not run in this no-Cargo environment.**

The supplied Linux binary currently passes 24/24 black-box checks, 37/37 extended offline acceptance checks, and 6/6 fail-closed/safety probes. These do not replace Rust compilation. The native OS sandbox and evidence-completion wiring remain unfinished, as do some feature parity and cross-platform verification work.


The required CI-gate evaluator has its own regression suite (`scripts/test_ci_pipeline_gates.py`, **7/7 PASS**) after a review exposed a stale step-name mismatch that could hide a Cargo-test failure in the final summary. CI now treats missing/failed/duplicate required gates as failures. This is test-harness validation; hosted CI is still needed to verify the real pipeline.
