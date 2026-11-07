# Do Code — Deep Test & Platform Verification Report

**Product:** Do Code  
**Implementation:** Rust (`docode-cli` crate), version `0.7.0-rs.1` WIP  
**Report date:** 2026-10-24  
**Status:** **NOT RELEASE-READY — compiler/runtime tests blocked**

## Executive result

The Rust branch has been expanded to include native CLI commands; strict configuration; bounded read-only repository tools; context indexing and packs; four provider request paths; role-based plan/coding proposals; persistent missions; transcript history; patch preview/apply/checkpoint/undo; evidence storage; local project/global memory; Decision Arena; freeze controls; audit/redaction; and explicit cost policy. The TypeScript reference snapshot is retained in the source archive.

I did not compile or execute this Rust program. The current runner is Debian Linux x86-64, but has no `rustc`, `cargo`, `rustfmt`, or `rustup`; DNS to Rust/crate distribution hosts does not resolve. No Rust executable or `Cargo.lock` is present. I cannot honestly report Rust tests, CLI launch behavior, install time, startup latency or OS compatibility as passed.

## Verification labels

- **Static PASS:** a source/config/package consistency check ran in this environment.
- **Authored test:** Rust test code exists, but its test runner has not executed it.
- **BLOCKED:** the check requires Rust/Cargo, a compiled binary, a connected CI runner or a different OS.
- **NOT IMPLEMENTED/PARTIAL:** the capability itself is absent or incomplete, regardless of test infrastructure.

A static PASS does not mean that Rust compiles or runtime behavior is correct.

## Verification matrix

| Area | Current status | Evidence/next action |
|---|---|---|
| Cargo/TOML manifests and example config | Static-checkable | Parse using Python TOML; still need Cargo dependency resolution and build |
| Python black-box/CI harness syntax | Static PASS | Three harness scripts parsed via Python AST and bytecode compilation; behavioral tests require a compiled `docode` executable |
| Rust source delimiters and workflow structure | Static PASS, limited | Delimiters balance in 20 Rust source files after skipping comments/strings; YAML parses and six OS/toolchain jobs are configured. This is not a Rust parser, compiler, or CI run. |
| Source inventory hashes | Static PASS in final preflight | Exact SHA-256/length match for all 42 inventoried source files; inventory intentionally does not self-hash |
| Main source ZIP and TypeScript reference ZIP | Static PASS in final preflight | Source archive has 43 members with ZIP CRC checks and no build/cache/private artifacts; preserved TypeScript archive has 509 structurally readable members. This is not proof of Rust compilability. |
| `cargo fmt` / `cargo check` / `cargo test` / Clippy | **BLOCKED** | Rust toolchain unavailable |
| Release executable and black-box acceptance | **BLOCKED** | No executable exists; test harness ready but not run |
| Linux runtime | **BLOCKED** | Environment OS is Linux but cannot build/run Do Code |
| macOS/Windows runtime | **BLOCKED** | No actual runner execution evidence |
| Toolchain install duration | **NOT MEASURED** | Must time a successful setup on a connected runner and mark cache/preinstalled status |
| Do Code installation/startup/operation time | **NOT MEASURED** | There is no built binary or end-user installer |
| Sandbox and test/build/browser execution | **NOT IMPLEMENTED** | Intentionally blocked in CLI; per-OS reviewed isolation is a release gate |
| Windows `.docode` ACL/private permissions | **INCOMPLETE** | Implement and verify native ACL handling before treating Windows as supported for untrusted repositories |

## Rust regression tests authored, not executed

The current source contains 70 Rust test attributes across modules and an integration test file. This is a count of authored annotations, not passing results; none of these tests has run in this environment. Test cases include:

- CLI help/version, local `init`, repeat initialization/no-overwrite, offline diagnostic, explicit consent requirement, and cost-policy flags.
- Config schema, unknown-key and API-key-source rejection, HTTPS/loopback endpoint rules, symlinked config refusal and initialization hazards.
- File list/read/search, secret path exclusions, path traversal, symlink escapes, hard links, binary/UTF-8/NUL/size limits, deterministic/bounded enumeration and output redaction.
- Structured and environment-assignment secret masking, secret-like JSON keys, bidi/control characters and safe terminal rendering.
- Audit sequence, redaction, owner-only POSIX modes, concurrent writers, hard-linked log refusal, malformed events and event/log bounds.
- Loopback provider fixtures for diagnostic success, Authorization header, error-body non-disclosure, malformed/oversized responses, redirects, timeouts and unknown provider-reported cost handling.
- Mission persistence/checksum/project binding/concurrent updates; transcript hash-chain tampering and concurrent appends; patch dry-run/approval/undo, preflight conflict and secret path rejection; evidence byte-integrity; memory/Decision Arena validation.
- Windows-gated opened-handle hard-link regression via the `GetFileInformationByHandle` API. This is only authored, not a Windows runtime result.

## Security changes currently present in source

- Terminal file reads and search excerpts use redaction plus terminal-control escaping.
- Sensitive-path filtering includes common secret directories, private keys and certificate formats. This is heuristic; it cannot guarantee all secret content is found.
- Unix and Windows opened-file hard-link rejection code is wired into state/config/audit/transcript/file reads, with platform-specific tests authored. These checks are still uncompiled/unverified, and do not substitute for a reviewed path/ACL boundary.
- Audit IDs use the stable `.docode/project-id`, and existing project IDs must be canonical UUIDs before state is accepted.
- Provider calls require `--live --approve-context-sharing`, reject redirects, use timeouts and response bounds, and avoid echoing provider error bodies.
- No predicted-cost field, maximum USD budget, estimated-cost stop, or cost-triggered auto-stop exists in the active product domain. Explicit provider-reported cost is optional and missing/invalid cost is `unknown`.
- No arbitrary shell, build, test or browser command is exposed until an OS sandbox exists.

These are source-level changes, not a passed penetration test or a security certification.

## Required execution on connected hosts

For **Ubuntu + macOS + Windows × Rust 1.88.0 + Rust 1.99.1**, run the checked-in workflow against the exact package revision. Require successful formatting, `cargo check --all-targets --locked`, `cargo test --locked`, Clippy with warnings denied, release build and black-box acceptance. Record logs and exit codes. Generate and review `Cargo.lock` through real Cargo dependency resolution—never fabricate it.

Before release, complete Windows private ACL hardening, reviewed Linux/macOS sandbox drivers, sandbox negative tests, feature-parity tests against the preserved TypeScript branch, installation checks, dependency/license/SBOM review and independent security review.

## Final verdict

**This is a growing Do Code Rust implementation, not a finished/100%-verified Do Code release.** The current session can produce source and static verification evidence, but lacks the compiler/OS runners needed to prove the Rust code builds or works. Treat runtime behavior, OS compatibility, install time and performance as unverified until reports from actual runs are available.
