# Do Code — current Rust status

**As-of date:** 2026-10-29  
**Product:** Do Code (`docode`)  
**Rust source version:** `0.7.0-rs.1`  
**Release status:** Feature-rich Windows-verified prototype with a mission engine; **not production-ready** (sandbox adversarial proof outstanding).

## 2026-10-29 update — native Windows verification + mission engine

Everything below this section is the 2026-10-26 status and is retained for the audit trail. What changed since:

**Build environment moved to the user's native Windows x86-64 machine.** All five gates pass on the exact current source: `cargo fmt --all -- --check` clean; `cargo check --all-targets --locked` clean; `cargo clippy --all-targets --locked -- -D warnings` clean; `cargo test --all-targets --locked` = **115 unit + 9 integration tests passed, 0 failed**; `cargo build --release --locked` produced a working `docode.exe` (about 6.4 MB). `docode --version`, `agents`, `risk`, `approvals`, `lessons`, and the full lessons lifecycle were smoke-tested against the release binary.

**New mission engine (implemented + unit-tested, wired into `docode run`/`apply`):**

- Loop controller: bounded mission loop (max steps, wall clock, stall detection) that pauses only at safe boundaries; per-mission loop state persists for resume.
- Spec-first contract gate: every run stores a mission contract (goal, success criteria, allowed/forbidden paths, required tests, rollback) before the coder starts, and rejects out-of-contract paths.
- Shared task board with per-file locks, dependency DAG, and conflict detection (`docode tasks`); applied patches mark covered tasks done.
- Multi-model routing with per-role routes and ordered fallback (`[routing]`); transport failures route around, safety/consent errors never do.
- Risk engine (0-5) + approval memory: silent/ask-once-remember/always-ask/never-auto policies; `docode apply --approve --remember`, `docode approvals show|docodet`, `docode risk "<command>"`.
- Complexity-gated teams with opt-in `--best-of-n` (keeps the first proposal that passes all safety guards).
- Compounding engine + memory tiers: explicit lesson propose/approve/reject/archive to git-friendly playbooks that brief future matching missions; hot/warm/cold layout under `.docode/memory/<scope>/`.
- Code-over-API workspace journal: append-only per-mission mutation ledger with before/after hashes and checkpoint binding, visible via `docode mission show`.
- Windows DACL enforcement with owner-SID readback, sandbox driver primitives (AppContainer + restricted token + Job Object + suspended spawn + sanitized env), `docode complete` evidence gate, crash recovery, context engine (nested .gitignore, stale-index detection) — all from the 2026-10-27 Windows session, now gate-verified.

**Still true from the blockers list below (unchanged):** command execution remains disabled — the sandbox primitives have NOT been run against the adversarial acceptance matrix (needs a disposable Windows runner); `docode complete` still refuses without sandbox-verified evidence (currently unobtainable); no macOS/Linux runtime evidence in this environment; hosted CI six-matrix, RustSec scan, license clearance, installer, and benchmarks remain outstanding. The cost policy is unchanged: no predicted USD, provider-reported only, `unknown` otherwise.

---

**As-of date (historical):** 2026-10-26  
**Product:** Do Code (`docode`)  
**Rust source version:** `0.7.0-rs.1`  
**Release status:** Useful Linux prototype with independent runtime evidence; **not production-ready**.

## What evidence exists now

### Supplied Claude build logs (reported execution in Claude's environment)

The package includes a real Rust 1.99.1 build and a stripped Linux x86-64 ELF binary (`8,052,120` bytes). The retained Cargo test log reports **63 module/unit tests + 7 CLI integration tests = 70 passed, 0 failed**. Retained logs show final Clippy and release-build commands. Claude's status reports formatting and `cargo check` passing after fixes, but explicit final success transcripts for those two commands are not retained; rerun them and archive the output. These supplied logs are evidence from that environment; they have not been rerun with Cargo in the independent checker environment.

### Independent checks run against the supplied binary in this handoff environment

- Binary `--version`, `--help`: PASS; version is `docode 0.7.0-rs.1`.
- `scripts/verify_cli.py --benchmark-runs 30`: **24 PASS, 0 FAIL, 0 SKIP**, on Linux x86-64 / glibc.
- Loopback-only synthetic-provider lifecycle: PASS for `init → six-role run/proposal → apply preview → explicit approved apply → verify refuses completion without executed tests → unapproved undo rejected → approved undo restores exact original content/SHA-256`.
- Direct CLI probes: init, doctor, sandbox status, sandbox self-test, mission list, audit tail and config show behaved as recorded in the logs. `sandbox self-test` correctly returned nonzero and executed no command because no native sandbox exists.
- SHA-256 manifests: all entries in the Rust package's manifest match the files in `01_CURRENT_VERIFIED_RUST`. Git history bundle cloned successfully and its commit history was inspected.
- Python static preflight previously reported 10 pass / 0 fail / 5 blocked, but should be rerun after the refreshed docs/source archive. Static preflight is not a Rust compile or security certification.

### Environment limits

This independent checker container has no `rustc`, `cargo`, `rustfmt`, `rustup`, or container engine. DNS for Rust/crate distribution and package hosts is unavailable. Therefore this environment cannot rerun Cargo compile/unit tests or execute macOS/Windows runners. Do not imply otherwise.

## Implemented source areas

- Rust CLI and strict TOML configuration; local-only initialization and diagnostics.
- Bounded workspace file listing/read/search with secret redaction, traversal, symlink, hard-link, binary and size guards.
- Local audit JSONL with locks/bounds/redaction; checksum-bound mission persistence and append-only hash-chained messages.
- OpenAI-compatible/Ollama-compatible/Anthropic/Google adapter code, explicit live/context sharing consent and local fixture tests in supplied logs.
- Hash-bound repository index/context pack; sequential Researcher/Architect/Tester/Coder/Reviewer/Security proposal workflow.
- Mission lifecycle, review-only patch preview, explicit apply approval, checkpoint and undo for tested existing-text-file replacements.
- Local memory and Decision Arena records, evidence store, freeze controls.

Presence does not imply complete TypeScript parity. Exact command and module gaps are documented in `docs/FEATURE_PARITY_LEDGER.md`.

## Critical release blockers

1. No native OS sandbox/command runner in the Rust branch. Build/test/shell/browser execution must remain disabled. Do not add an unsafe host-shell fallback.
2. Evidence/completion-gate structures exist, but are not fully wired to live missions. Do Code must not claim task completion without verified test evidence.
3. Resume is inspection-only. Automatic safe mission reconstruction is not implemented.
4. Rust memory CLI lacks the TypeScript review-first `memory propose` workflow and `memory open` command; save semantics/flags differ.
5. Patch support is limited to modifying existing contextualized text files; add/delete/rename/binary/full unified-diff operations and strong crash recovery remain incomplete.
6. Windows private `.docode` ACL creation/verification remains incomplete. Windows handle-based hard-link code exists in source but is not runtime verified here.
7. No macOS/Windows Rust build or test results; no actual six-matrix CI run.
8. A preliminary offline CycloneDX SBOM and pinned RustSec audit workflow now exist. The actual advisory workflow has not run in this authoring environment; license classification, a final Cargo-aware release SBOM, independent security review, installer/upgrader qualification and signed provenance remain outstanding.
9. Real provider/model quality has not been tested; workflow validation uses a loopback fixture. Warm-cache Linux timing and peak-RSS measurements are recorded in `docs/PERFORMANCE_BASELINE_LINUX_2026-10-26.md`, but they are not end-user guarantees.

## Current independent timing sample (warm-cache runner, not a promise)

On 26 October 2026, `scripts/verify_cli.py` ran 30 fresh subprocess samples after warmup: `--version` median **1.288 ms**, p95 **1.900 ms**, maximum **1.942 ms**. Ten-run samples: `doctor` median **3.614 ms**, `files list` median **3.786 ms**, `config show` median **4.015 ms**. Installation duration and cold-start performance remain unmeasured; a separate one-container Linux peak-RSS sample now exists in `docs/PERFORMANCE_BASELINE_LINUX_2026-10-26.md`.

## Required next steps

1. In a connected Rust environment, rerun and retain successful logs for formatting, locked check, test, Clippy, release build and static/black-box checks at one source revision.
2. Resolve `FEATURE_PARITY_LEDGER.md` gaps or explicitly scope them out.
3. Design/implement native OS sandbox isolation and connect it to trusted evidence/completion gates; independent adversarial review is required before enabling commands.
4. Complete Windows ACL and execute actual Linux/macOS/Windows CI.
5. Run the pinned RustSec advisory workflow on GitHub, generate a Cargo-aware license-reviewed release SBOM, and complete installer/upgrader work; then conduct a limited public beta and triage failures before stable release.

## Cost policy invariant

No guessed prices, predicted USD fields, max-cost/dollar threshold flags, Decision Arena cost triggers, or auto-stop due to estimated/provider-reported cost. Provider-reported actual costs may be displayed if supplied; absent/invalid values are `unknown`.


## Additional continuation evidence (26 October 2026, later pass)

- Black-box binary acceptance rerun: 24/24 PASS, 0 FAIL, 0 SKIP; log and JSON in `04_INDEPENDENT_VERIFICATION/BLACK_BOX_BENCHMARK_2026-10-26.*`.
- Extended offline workflows: 37/37 PASS, 0 FAIL, covering repository index/context, memory save/search/export/import/reset/docodet approvals, Decision Arena honesty/persistence, freeze/unfreeze, and sandbox fail-closed behavior.
- Additional safety probes: 6/6 PASS, 0 FAIL; this only proves CLI refusal/disclosure behavior, not isolation.
- Preliminary SBOM generator tests: 5/5 PASS, 205 third-party lockfile components and 206 dependency records; licenses intentionally not claimed and no advisory scan occurred.
- Python syntax validation: all project Python helper scripts passed `py_compile`.
- Added CI source-inventory validation, SBOM generation/tests, Linux RSS metrics, and `.github/workflows/security-audit.yml` (pinned RustSec action). These CI steps are authored/configured; hosted workflow execution remains unobserved.
- New inventory checker confirms the stored source inventory can be validated by hash; it is wired as a required CI step.

The environment still has no `cargo`, `rustc`, `rustfmt`, or `rustup`. The current Rust source did not undergo a new Cargo compile in this pass, and native sandbox, evidence-gate wiring, OS-specific sandboxes, Windows ACLs, and macOS/Windows builds remain release blockers.


## Continuation — reproducibility and supply-chain gates (26 October 2026)

- The CI pipeline now requires `Cargo.lock` in the source checkout rather than silently resolving a new lockfile; its exact bytes are captured as `Cargo.lock.artifact`. The Rust test invocation was strengthened to `cargo test --all-targets --locked`.
- Added a `cargo-cyclonedx` 0.5.9 wrapper and eight network-free SBOM-validator/orchestration tests (8/8 passed). The real generator is configured to execute only in the Linux/Rust 1.99.1 hosted CI job; that run has not occurred from this authoring environment.
- Existing binary checks rerun here: 24/24 basic black-box, 37/37 extended offline acceptance, 6/6 safety probes. These do not compile the Rust source.
- A local attempt to start the full Rust CI pipeline exits 2 because `cargo` and `rustup` are absent. Native OS sandbox, complete evidence-gate wiring, full parity, Windows ACL hardening and macOS/Windows test evidence remain release blockers.

### CI gate-integrity follow-up (26 October 2026)

A review caught a real verification-pipeline bug: the `cargo test` display name had changed while the final required-gate name had not, so the final status filter could fail to classify a failing test step under its new name. The code now has a centralized evaluator that fails missing, non-PASS, or duplicate required gates. `scripts/test_ci_pipeline_gates.py` exercises these cases: **7/7 PASS**. The source preflight asserts the current test name and evaluator are present; this script test does not replace actual Cargo/hosted CI runs.
