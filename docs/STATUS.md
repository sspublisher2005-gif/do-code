# Do Code — current Rust status

**As-of date:** 2026-11-08  
**Product:** Do Code (`docode`, repo `sspublisher2005-gif/do-code`)  
**Rust source version:** `0.7.0-rs.1`  
**Release status:** Cross-platform CI GREEN on all 6 jobs; sandbox driver executes on Windows with real containment; adversarial matrix partially proven; **not production-ready** until the full matrix + review gates pass.

## 2026-11-08 update — first fully green cross-platform CI + real sandbox execution

**CI (run 42413387320, commit 784dbeb): all 6 jobs SUCCESS on GitHub-hosted runners** — Rust 1.88.0 and 1.99.1 × ubuntu-latest, macos-latest, windows-latest. This is the project's first complete hosted-CI pass (fmt, check, clippy -D warnings, 127 unit + 10 integration tests, release build, black-box acceptance, extended offline acceptance, SBOM, release-gate evaluation — on every OS/toolchain combination).

**Windows sandbox: the driver now really executes.** The matrix found and I fixed a chain of real bugs (each proven by run artifacts):
1. Missing `CREATE_UNICODE_ENVIRONMENT` — CreateProcessW never spawned.
2. Minimal env blocks fail with ERROR_ENVVAR_NOT_FOUND (203) inside AppContainer — five-variant bisect; the design now uses the full inherited environment with credential-shaped keys/values dropped, plus an explicit private workspace as cwd.
3. Private per-run workspace: fresh temp dir, DACL grants the container SID full control (additive ACE).
4. `.forge`→`.docode`, product renamed **Do Code** everywhere (zero occurrences left in the repo; only the README provenance paragraph and the reference/ archive name mention the historical name).
5. Administrators-owned dirs accepted: the raw SID bytes were wrong twice (endianness, then sub-authority count); now ground-truth-pinned by a unit test that compares our bytes against Windows' own `ConvertStringSidToSidW("S-1-5-32-544")` via EqualSid.
6. Source inventory made deterministic (sorted rows — os.walk order differs NTFS vs ext4) and excludes generated `test-results/`.
7. All harness subprocess calls decode UTF-8 explicitly (the runner's locale broke Unicode-path checks).

**Locally verified additionally:** sandboxed spawns stable (6/6), workspace-only writes, outside reads return nothing; extended acceptance 37/37; black-box 23/23.

**Still open (honest):** the Windows matrix subset is partially proven (FS-01/02, NET-01, ENV-01, RES-01, PROOF-01 PASS on the runner so far; PowerShell-in-container probes still denied on GitHub runner images — Windows-specific sandbox hardening continues); Linux/macOS drivers not implemented; restricted primary token not wired (windows-sys 0.61 lacks the attribute); `docode complete` stays fail-closed; hosted CI must stay green on every push; independent security review outstanding.

---

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


---

## 2026-11-08 (later) - code-fix pass on `work/windows-security-sandbox` (uncommitted)

Continued from the orphaned pre-rename session. Scope was deliberately code-only; nothing was committed or pushed.

**Fixes landed (working tree only):**
- `scripts/sandbox_adversarial_windows.py`: duplicate `errors="replace"` / timer removed (was the SyntaxError blocking the matrix harness parse).
- `scripts/validate_python_sources.py` (new) + required gate in `scripts/ci_pipeline.py` + regression test in `scripts/test_ci_pipeline_gates.py`: Python harness syntax is now a required CI gate.
- `.github/workflows/sandbox-matrix-windows.yml`: matrix now triggers on push/PR touching the runner, workflow, src/sandbox.rs, src/main.rs, src/storage.rs, src/error.rs, Cargo.*, or the matrix JSON (previously: workflow YAML only, so runner fixes never triggered the gate).
- `src/contract.rs`: removed the hard-coded `approved: true` field; contracts print "Enforced:" not a fake human approval. Derivation/enforcement behavior unchanged.
- `src/teams.rs`: Standard and Complex teams now actually staff a Coder (order: Architect, Coder, Tester, Reviewer[, Security]).
- `src/workflow.rs`: loop budget is enforced at six step boundaries (research/plan/test/implement/review/security); a pause is not a failure — mission moves to Interrupted with a persisted loop summary and an audit record. User-confirmed project memory is injected as advisory brief (max 3 notes) into the architect prompt and coder prompt.
- `src/storage.rs`: Windows atomic replace retries transient access-denied/sharing-violation (5/32/33) up to ~1 s; updates were already mutex-serialized, this covers external handle contention (e.g. real-time scanners).
- `src/mission_store.rs`: concurrency test now stresses 12 contention rounds (was 1 lucky run).
- `src/sandbox.rs`: on timeout the WHOLE process tree is killed via TerminateJobObject (was main-process only). Disposable container workspaces are now removed after every `sandbox exec` run and on spawn failure paths. Removed dead `SandboxProfile` policy scaffolding.
- `src/mission.rs`: removed dead duplicate `completion_allowed` policy function (the real gate remains in `workflow.rs::complete`).
- `docs/STATUS.md`: this entry; counts updated here - 125 unit + 10 integration.

**Verified on this machine (Windows, MSVC 1.99.1, offline):** `cargo fmt --check` clean; `cargo check --all-targets --locked` clean; `cargo clippy --all-targets --locked -- -D warnings` clean; `cargo test --all-targets --locked` = **125 unit + 10 integration, 0 failed**; previously reported 127 unit reflects two deleted dead-code tests. One `concurrent_mission_updates_do_not_lose_appends` failure (access denied, code 5, transient Windows handle contention) was observed intermittently under full-suite load before the retry fix.

**Root-cause note:** the ten integration tests were all failing with path-not-found after the 2026-11-08 folder rename — the stale prebuilt test binary embedded the old `forge cli\...` path in `env!("CARGO_BIN_EXE_docode")`. `cargo clean -p docode-cli` rebuilt everything against the new path; 10/10 pass.

**Still missing / not claimed:** the 26-case adversarial matrix has NEVER run — `is_qualified()` stays false, `docode run` and arbitrary execution stay disabled; the `--native` probe executor was never written; restricted primary token still unwired (windows-sys 0.61); cancellation propagation for provider requests not done; independent security review outstanding; Linux/macOS drivers absent; the "6-job green CI" figure remains a prior-session claim, not independently re-verified this pass. These documents and the code changes are uncommitted; commit only on explicit approval.


---

## 2026-11-08 (P1) - single-agent tool loop landed (`docode agent`)

License-compliant decision recorded: open-source dependencies may be vendored with notices; closed-source code is never copied; specs/ideas reimplemented independently. (User decision, 2026-11-08.)

**New (uncommitted working tree):**
- `src/tools.rs` (new): typed tool registry with hand-checked schemas - `workspace.list`, `workspace.read`, `workspace.search` execute read-only with bounded/redacted output; `process.run` and unknown tools are policy-denied in this build (no qualified execution driver). Traversal/absolute paths, wrong types, out-of-range bounds rejected before any side effect.
- `src/agent_loop.rs` (new): one bounded loop per mission - model turn → validated tool call → policy → bounded local execution → observed result → ... → final patch proposal stored as a normal patch record (applicable via `docode apply --approve`). Nothing is applied unattended; completion remains impossible (evidence gate).
- `src/workflow.rs`: extracted `build_patch_changes` shared by the multi-role workflow and the agent loop (identical per-operation contract checks); `parse_patch_json`/`validate_task`/`budget_checkpoint`/`fail_mission` now `pub(crate)`.
- `src/main.rs`: new `agent` subcommand (`docode agent "<task>" --live --approve-context-sharing`), same consent gating as `docode run`. Step ledger honesty: research/plan steps recorded `SKIPPED_AGENT_TOOL_LOOP`, test stays `TEST_PLAN_ONLY_NOT_EXECUTED`, review/security stay Pending.
- `tests/agent_loop.rs` (new, integration): scripted fixture provider (no network) exercises: schema traversal rejection before execution, policy denial of `process.run`, successful read after errors, proposal stored, **nothing written to disk**, patch applicable via `apply --approve`, audit trail shows denials and the proposal.
- `src/teams.rs` / `src/mission.rs` (earlier in this session): Standard/Complex teams now staff a Coder; dead duplicate `completion_allowed` removed; loop budget enforced at six step boundaries; Windows atomic replace retries transient sharing violations; sandbox timeout now kills the whole tree via TerminateJobObject; container workspaces deleted after every run; `SandboxProfile` scaffold removed.
- Extra probes updated: the safety probe asserting the old `NOT IMPLEMENTED` status string now asserts the truthful `NOT qualified` disclosure (stale test repaired, strictness unchanged).

**Verified on this machine (Windows, MSVC 1.99.1, offline):** `cargo fmt --check` clean; `cargo check --all-targets --locked` clean; `cargo clippy --all-targets --locked -- -D warnings` clean; `cargo test --all-targets --locked` = **131 unit + 11 integration, 0 failed**; `verify_cli.py` 23 pass / 1 skip (OS-denied symlink); `extended_offline_acceptance.py` 37/37; `extra_safety_probes.py` 6/6; Python harness syntax 14/14; CI gate tests 8/8.

**Not done / not claimed:** no live-model run of the agent loop (needs provider keys and the interlock); the loop does not execute tests/builds for the user (sandbox unqualified, `is_qualified()` false); review/security steps and `docode complete` remain unreachable without trusted evidence; restricted primary token unwired; independent security review outstanding; matrix NEVER run. All of this and every file above remain uncommitted pending explicit approval.

**Restricted primary token — closed as a non-gap with evidence (2026-11-08):** PROC_THREAD_ATTRIBUTE_TOKEN is absent from the documented UpdateProcThreadAttribute attribute table; CreateProcessWithTokenW requires SeImpersonatePrivilege. There is no unprivileged primary-token API for assigning a child process's primary token on Windows. Codex solves this with an elevated broker service; we deliberately do not (local-first CLI). AppContainer SID inheritance is the only documented unprivileged injection mechanism. src/sandbox.rs comments updated to say exactly this.

---

## 2026-11-08 (sandbox matrix) - Windows subset green on a disposable runner

**Evidence:** GitHub Actions run 42483293327, workflow `sandbox-matrix-windows.yml`, `windows-2022`, commit 6d1f2b2. Artifact `sandbox-adversarial.json`: `status = WINDOWS_SUBSET_PASS`, 10/10 cases passed, 0 failed, every probe reported `PROBE-CWD:<workspace>:readable=true`, `PID:3996` gone after exit, RES-01 killed with NTSTATUS 0xC0000409 by the 512 MiB job memory limit, PROOF-01 refused with `mission ... does not exist in this project`.

**What was actually broken (the matrix was never measuring containment before):**
- The probe child died in `canonicalize(".")` before dispatching, so all eight sandboxed cases reported "output missing" as an access-denied harness error, not a verdict. Probes use no project root; they now dispatch without canonicalizing and print `PROBE-CWD:<path>:readable=<bool>` so the failure can never hide again.
- `sandbox exec` returned exit 0 regardless of the child status; it now propagates it (timeout is never success).
- proc-01 spawned the probe's own binary, which the AppContainer is correctly denied reading; it now spawns a System32 image (nested creation is what the case measures).
- The workspace was passed as a `\\?\` verbatim cwd, which makes `cmd.exe` relocate to `%SystemRoot%`; the prefix is stripped.
- RES-01 expected exit 1; the job memory limit actually aborts the runaway allocation with 0xC0000409, so the case now requires a non-zero status.
- NET-01 expected the PowerShell-era literal `False`; the resolver deny path printed `NET01:RESOLVER-DENIED`. Both denials now print `NET01:DENIED`.
- PROOF-01 passed on any failure: `complete` on an unknown mission leaked a raw `\\?\` path I/O error. `mission_store::get` now states `mission <id> does not exist in this project`, and the case requires that reason.
- Exit 21 (probe could not run) now always fails a case instead of slipping through.

**What this does NOT establish:** `is_qualified()` stays false. This is the Windows-applicable subset on one disposable VM: no independent security review, no Linux/macOS driver, no packaging cases, restricted primary token still unwired (no unprivileged Windows API provides it - see the earlier non-gap note), and the RBAC/network outcome is the AppContainer zero-capability model only. `docode run` and arbitrary execution remain disabled.
