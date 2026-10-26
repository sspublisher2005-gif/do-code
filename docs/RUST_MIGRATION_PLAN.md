# Forge CLI — Rust migration and completion plan

**Product:** Forge CLI  
**Implementation language:** Rust  
**Working source version:** `0.7.0-rs.1`  
**Reference snapshot:** `reference/Forge_CLI_TS_0.6.0-ts.2_MemoryDecision_Merge_WIP.zip`

## Non-negotiable migration rules

1. Forge CLI remains the product; Rust is only the implementation language.
2. The TypeScript branch remains preserved until parity is demonstrated feature-by-feature with tests and evidence. Do not discard it or copy historical test counts into Rust reports.
3. Do not add predicted USD pricing, `max_cost_usd`, estimated-cost fields, or mission-stop triggers based on guessed price. Actual provider-reported cost may be shown after the request; absent/invalid values are `unknown`.
4. The CLI must fail closed when approval, path policy, evidence, state integrity, or sandbox readiness is missing. Do not add a host-shell fallback.
5. Source presence is not functional verification. Mark every item `implemented-unverified`, `partial`, `blocked`, or `verified` with current-run evidence. Never mark `verified` based on comments or authored tests.

## Current port inventory — source is not yet compiler-verified

| Feature | Rust source currently present | Known limitation / proof needed |
|---|---|---|
| Native CLI and configuration | `main.rs`, `config.rs` | Build, clap behavior, config compatibility fixtures, clean install |
| Safe read-only repository tools | `files.rs` | Adversarial path/identity tests on actual Windows/macOS/Linux; not a sandbox |
| Index and context packs | `context.rs` | Heuristic index; not AST/LSP; ignore-file parity is partial; test limits and stale hashes |
| Explicit-consent provider clients | `provider.rs` | OpenAI-compatible, Ollama-compatible, Anthropic and Google request paths in source; require fixture/contract tests and provider-specific error/usage coverage |
| Multi-role proposal workflow | `workflow.rs` | Sequential perspectives are not independent experts; model output is advisory; test/build/browser execution is blocked |
| Persistent missions/transitions | `mission.rs`, `mission_store.rs` | `resume` is inspection-only; test power loss, cross-process updates and recovery |
| Transcript history | `message_store.rs` | Local hash-chained JSONL source; compile, concurrency and tamper tests needed |
| Patch preview/apply/checkpoint/undo | `patches.rs` | Existing text-file replacements only; no add/delete/rename/binary changes; multi-file rollback and crash recovery need hardening/tests |
| Evidence manifest/content store | `evidence.rs` | Hash/length verifies integrity, not factual truth; test redaction/provenance and corruption behavior |
| Local project/global memory | `memory.rs` | Explicit user confirmation; not silently included in model prompts; verify import atomicity and editor handling |
| Decision Arena | `decision_arena.rs` | Decision support, not external research or ground-truth scoring; validate triggers and provider handling |
| Freeze, audit and redaction | `freeze.rs`, `audit.rs`, `redaction.rs` | Regex redaction is heuristic; stress lock/corruption/concurrency/races on each OS |
| Windows opened-handle hard-link check | `storage.rs` plus Windows-gated test | Newly authored against the Win32 API, still uncompiled/unverified; private `.forge` ACL creation/validation is incomplete |
| OS command/build/test/browser sandbox | No production driver | **Blocked release gate.** Never execute repository commands until reviewed platform-specific isolation and negative tests pass |
| Installers, release artifacts and performance | Not release-complete | No compiled binary/installer; no install/startup/operation timing results yet |

## Work stages

### 1. Establish reproducible compilation

On a connected clean runner, install the pinned Rust 1.99.1 toolchain, resolve dependencies, review and commit the real generated `Cargo.lock`, then run:

```sh
cargo +1.99.1 fmt --all -- --check
cargo +1.99.1 check --all-targets --locked
cargo +1.99.1 test --locked
cargo +1.99.1 clippy --all-targets --locked -- -D warnings
cargo +1.99.1 build --release --locked
```

Do not generate a fake lockfile. Fix every compiler, formatting, test and lint failure and rerun the full suite after the last code change.

### 2. Run real black-box acceptance tests

Build the release executable, run `scripts/verify_cli.py`, retain the JSON report, and require zero failures and zero skips for a clean matrix result; any OS capability-based skip must be separately investigated and explicitly adjudicated rather than counted as passing coverage. The harness tests help/version, local initialization, idempotence, config restrictions, private file exclusions, terminal redaction, traversal/symlink/hard-link handling, audit order, model consent, and cost-policy invariants. Its timing data is only collected once a real binary exists.

### 3. Run the six host/toolchain combinations

The checked-in workflow defines Ubuntu, macOS and Windows on Rust 1.88.0 and 1.99.1. The source environment does not have access to that CI run; retrieve actual workflow evidence for the exact code revision. Rust 1.88 is the declared MSRV, so dependency lockfile and all target APIs must be proven compatible with it. If not, either replace incompatible dependencies or explicitly raise the MSRV and update docs/CI.

### 4. Complete OS security boundaries

- Windows: implement and test owner-only `.forge` ACL creation/validation, reparse-point/race behavior, opened-handle identity and ACL failure modes. Current hard-link check source does not close the ACL gap.
- Linux: implement a dedicated sandbox driver with namespaces and a reviewed Landlock/seccomp policy (or reviewed equivalent). Test denied host/home/secrets, denied networking, write boundaries, CPU/memory/PID caps, output caps, timeouts and process-tree cleanup.
- macOS: implement a separately reviewed Seatbelt profile and test equivalent negative cases.
- Do not enable arbitrary test/build/browser commands by using the current process environment as a fallback. An unsupported platform must report blocked and stay blocked.

### 5. Reach feature parity with the preserved TypeScript branch

Create a testable list of each TypeScript capability/config field/command and map it to a Rust implementation plus fixture tests. Remaining known areas include full mission-resume automation, robust stale-lock/crash recovery, patch add/delete/rename/binary support, complete `.gitignore` semantics, richer context indexing, global/project-memory fault tolerance, full provider error/usage parity and sandbox-backed test evidence. Port only after reviewing the source/reference and preserving intentional product-policy differences.

### 6. Release qualification

After code and sandbox completion, run security review, dependency/license audit, SBOM generation, vulnerability review, locked clean builds, install/uninstall smoke tests, checksums, package provenance, and matched benchmarks. Measure toolchain download/setup separately from Forge install. Measure 30+ repeated fresh-process startup samples, operation latency, executable bytes and RSS on each OS. A hosted runner's timings are not an individual laptop guarantee.

## Current environment blocker

This authoring environment has no `rustc`, `cargo`, `rustfmt`, or `rustup`, no existing Rust installation was found in typical tool locations, and DNS to Rust/crate distribution hosts does not resolve. There is no connected GitHub repository available through the current account integration. Thus I can edit source/docs and run static/package preflight, but cannot honestly claim `cargo check`, `cargo test`, runtime benchmarks, or real macOS/Windows results.

## Completion definition

“100%” requires a reviewed feature-parity matrix, a real lockfile, clean locked compilation and tests, zero outstanding critical/high bugs, all six OS/toolchain jobs with passing evidence, production sandboxes with negative isolation tests, clean-install evidence, measured startup/task performance and independent security review. This WIP does not meet that definition yet.
