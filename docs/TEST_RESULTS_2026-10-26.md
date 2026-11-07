# Do Code — independent test results, 26 October 2026

## Tested artifact

- Product/version: Do Code `0.7.0-rs.1`.
- Artifact: `01_CURRENT_VERIFIED_RUST/binaries/docode-linux-x86_64`.
- Format: dynamically linked, stripped ELF x86-64 for Linux/glibc.
- Size: 8,052,120 bytes.
- SHA-256: `a861235a713b8375b6eeccdfc699ccda046e603e5e6eed4ef658cc6a8d704962`.
- Environment: Linux 6.18.55 x86-64 / glibc 2.41; test harness Python 3.13.5.
- Latest independent black-box report: `04_INDEPENDENT_VERIFICATION/independent_verify_cli_rerun_2026-10-26.json`; console transcript is the matching `.log`.

## Independent black-box suite

Command (from the handoff root):

```sh
python3 01_CURRENT_VERIFIED_RUST/source/docode-cli/scripts/verify_cli.py \
  --binary 01_CURRENT_VERIFIED_RUST/binaries/docode-linux-x86_64 \
  --benchmark-runs 30 \
  --report 04_INDEPENDENT_VERIFICATION/independent_verify_cli_rerun_2026-10-26.json
```

**Result: 24 passed, 0 failed, 0 skipped.** Checks cover version/help, initialization and no-overwrite behavior, doctor/config, no credential value disclosure, Unicode/space paths, offline model diagnostic and explicit live consent, absence of predicted-cost/budget-stop flags, file/path/redaction/binary/size guards, symlink/hard-link rejection, audit ordering/redaction, unknown configuration keys and cleartext remote endpoint blocking. This suite did not call an external model/API.

## Loopback mission lifecycle E2E

Command:

```sh
python3 04_INDEPENDENT_VERIFICATION/reproduce_loopback_e2e.py \
  --binary 01_CURRENT_VERIFIED_RUST/binaries/docode-linux-x86_64
```

**Result: PASS, exit code 0.** Full log: `04_INDEPENDENT_VERIFICATION/independent_fixture_mission_lifecycle_2026-10-26.log`.

1. Initializes a disposable synthetic project locally.
2. Calls a local loopback OpenAI-compatible fixture server for the six-role proposal workflow (not a real model/service).
3. Proves `run` only creates a patch proposal; original file is unchanged until approval.
4. Proves apply preview does not mutate the file.
5. Explicitly approved apply changes the file and SHA-256 from `765af0fb59be2b2dc941a19176f2f444a54f6070af67e3b341819ab769d2aaac` to `18c4bd58e485f329ac43825af4423332540003ca2b7a290a2f8c9a440356bbd5`.
6. `docode verify` refuses to claim task completion because project tests were not actually executed.
7. Undo without approval is refused and does not mutate the file.
8. Approved undo restores the exact original content and SHA-256.
9. Fixture traffic stays on `127.0.0.1`; no external key or service is used.

## Additional direct CLI probes

See `01_CURRENT_VERIFIED_RUST/logs/additional_commands_rerun_2026-10-26.log`. Help/version, `init`, `doctor`, `sandbox status`, `mission list`, `audit --tail 20`, and `config show` were exercised. `sandbox self-test` exited nonzero and did not execute a user command because the native sandbox is absent. This is the intended fail-closed behavior, **not** a successful isolation test.

## Warm-cache timing sample (one Linux runner only)

Fresh subprocess startup used 30 samples after warmup; command measurements used 10 samples. Latest measurements are retained in the JSON report:

| Operation | Samples | Median | p95 | Maximum |
|---|---:|---:|---:|---:|
| New-process `docode --version` startup | 30 | 1.288 ms | 1.900 ms | 1.942 ms |
| `docode doctor` | 10 | 3.614 ms | 4.267 ms | 4.267 ms |
| `docode files list` | 10 | 3.786 ms | 5.765 ms | 5.765 ms |
| `docode config show` | 10 | 4.015 ms | 5.494 ms | 5.494 ms |

These are warm-cache timings on one CI-like Linux container, not cold-start measurements or a promise for end-user hardware. At the time of this initial timing run, RSS was not yet measured; a later Linux RSS sample is included in `PERFORMANCE_BASELINE_LINUX_2026-10-26.md`. Toolchain installation duration and matched-hardware TypeScript comparison remain unmeasured.

## Supplied Cargo build/test evidence (not rerun by this authoring environment)

- `01_CURRENT_VERIFIED_RUST/logs/06_cargo_test_final_70_pass.log` reports 63 unit/module tests + 7 CLI integration tests = **70 passed / 0 failed** in the supplying Claude Rust environment.
- `04_cargo_clippy_final_PASS.log` and `07_cargo_build_release.log` record reported final Clippy/release-build success.
- The final successful output transcripts for `cargo fmt --all -- --check` and `cargo check --all-targets --locked` are absent; the retained transcripts for those are initial failures. Rerun both and retain final outputs.
- This authoring environment has no `rustc`, `cargo`, `rustfmt`, or `rustup`, and Rust distribution endpoints are unreachable, so Cargo tests/compilation were not independently rerun here.

## Parity and security gaps

Read `01_CURRENT_VERIFIED_RUST/source/docode-cli/docs/FEATURE_PARITY_LEDGER.md`. Key gaps remain: native OS sandbox/command runner; evidence/completion wiring; automatic mission resume; review-first memory proposal and `memory open`; patch create/delete/rename/full-diff and crash recovery; ignore/index freshness; complete Windows ACLs; macOS/Windows builds/tests; provider quality checks; supply-chain/installer/release qualification.

`SANDBOX_DESIGN_AND_TEST_SPEC.md` gives reviewed platform design guidance. `SANDBOX_ACCEPTANCE_MATRIX.json` lists **26 adversarial/release tests**. These sandbox driver tests are **NOT RUN / driver not implemented**. The current binary intentionally fails closed; it must not execute project commands until a driver has passed its OS-specific isolation suite and its results are integrated with `docode verify`.

## Release verdict

The supplied Linux binary has reproducible command and fixture-E2E evidence. **Do Code is not yet 100% complete or production-ready for executing untrusted project commands.** The Rust tests/build are reported passing by the supplying environment, but this host has not rerun Cargo and the final format/check logs are missing. Never claim macOS/Windows support or sandbox isolation without real OS-specific evidence.

## Cost-policy invariant

No predicted/fabricated USD cost estimates, dollar-trigger auto-stop rules, or hidden cost threshold. Missing provider-reported actual cost remains `unknown`.


## Additional safety probes (independently rerun on 2026-10-26)

Reproducible script: `scripts/extra_safety_probes.py`. Raw result: `01_CURRENT_VERIFIED_RUST/logs/extra_safety_probes_2026-10-26.log`; machine-readable result: `01_CURRENT_VERIFIED_RUST/logs/extra_safety_probes_2026-10-26.json`.

**Result: 6 PASS, 0 FAIL.** These checks cover version, core CLI help/policy (including absence of predicted-cost flags), honest sandbox status disclosure, the sandbox self-test's nonzero fail-closed response before executing any user command, listing missions in a new empty temporary root, and 12 secondary command help surfaces. This is additional evidence for safe refusal and CLI behavior, **not** proof of a native sandbox; sandbox driver tests remain not run because the driver is not implemented.


## Later independent continuation pass (26 October 2026)

These commands were run against the actual packaged Linux binary/source tooling in the handoff environment; raw logs are included in `04_INDEPENDENT_VERIFICATION/`.

| Check | Result | Important scope note |
|---|---:|---|
| Black-box CLI acceptance | 24 PASS / 0 FAIL / 0 SKIP | Actual Linux ELF; no external model/API calls. |
| Extended offline workflow acceptance | 37 PASS / 0 FAIL | Actual binary; memory, context/index, Decision Arena, freeze and fail-closed sandbox. |
| Extra CLI safety probes | 6 PASS / 0 FAIL | Tests honest sandbox refusal and command surfaces; not isolation proof. |
| Offline SBOM generator unit tests | 5 PASS / 0 FAIL | Python standard-library test suite against Cargo.lock. |
| Python harness syntax | PASS | All `scripts/*.py` parsed via `py_compile`. |
| Linux RSS measurement | 10 samples per operation + warm-up | Results in `PERFORMANCE_RSS_2026-10-26.json`; one Linux container only. |
| Source inventory validation | PASS | Checks all listed byte sizes and SHA-256 after inventory regeneration. |
| RustSec audit | NOT RUN | A workflow was authored; GitHub workflow execution is not available/observed here. |
| Cargo formatter/check/test/build in this pass | BLOCKED | `cargo`, `rustc`, `rustfmt` and `rustup` are not installed in this environment. Prior Claude logs still report 70/70 Rust tests but were not reproduced here. |
| macOS/Windows execution and security | NOT RUN | No actual OS runners were available in this session. |

The preliminary CycloneDX document lists 205 third-party components and 206 dependency graph records. It explicitly does not claim complete license information or an advisory clearance. See `docs/DEPENDENCY_AUDIT_STATUS.md`.

## Further continuation (26 October 2026)

- Cargo-aware CycloneDX 1.5 SBOM validator/wrapper unit tests: **8/8 PASS**, including one mocked-orchestration test. The real `cargo-cyclonedx` generator was not run locally because Cargo is absent; CI is configured to run it only on Linux/Rust 1.99.1.
- Existing preliminary offline SBOM generator tests: **5/5 PASS** (205 third-party Cargo.lock crate components; no license or vulnerability clearance claim).
- Actual supplied Linux binary: black-box **24/24 PASS**; extended offline CLI acceptance **37/37 PASS**; fail-closed/safety probes **6/6 PASS**.
- CI pipeline attempted directly and exited **2 / BLOCKED** because `rustup` and `cargo` are unavailable in the current runner. No current local Cargo compile/test result is claimed.

## CI release-gate evaluator regression tests (26 October 2026)

- Added `scripts/test_ci_pipeline_gates.py`: **7/7 PASS**. Tests demonstrate that a missing required step fails, a failed current `cargo test --all-targets --locked` step is caught, legacy names do not satisfy current required gates, duplicate step names fail closed, and SBOM/RSS steps are required only in their declared CI jobs.
- CI now evaluates the set of required gates after collecting step results and emits a required-release-gate summary; the acceptance/supply-chain test scripts themselves are mandatory gates.
- These are Python test-harness tests; they do not substitute for running Cargo or hosted CI.
