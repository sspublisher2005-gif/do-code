# Forge CLI (Rust implementation): cross-platform verification and performance plan

## Test matrix

The authored `.github/workflows/rust-ci.yml` matrix has six combinations: Ubuntu, macOS, and Windows with Rust 1.88.0 (the declared `rust-version` minimum) and Rust 1.99.1 (the toolchain pinned in `rust-toolchain.toml`). A workflow file is not evidence of a passing test; each result must come from a completed workflow run for the exact source revision.

For each matrix job, `scripts/ci_pipeline.py` records toolchain setup elapsed time, `cargo check --all-targets`, `cargo test`, `cargo clippy --all-targets`, optimized release-build time, and results from `scripts/verify_cli.py`. The latter exercises the built executable, uses only loopback fixtures and temporary workspaces, and does not make live AI provider calls. Reports are intended to be uploaded as workflow artifacts under `test-results/`.

## Covered by the authored Rust tests / runtime harness

- CLI `--version` and `--help`; init creation, idempotency, and preservation of existing files.
- `doctor` disclosure of missing sandbox and no money-prediction/automatic cost-stop policy.
- Typed config defaults, unknown-key rejection, safe credential environment-variable allowlists, URL scheme/userinfo/query checks, and blocking remote cleartext HTTP.
- Offline model diagnostic by default; explicit opt-in gate before any live request.
- Local loopback HTTP fixtures for a successful completion/usage payload, Authorization bearer header construction, non-success status body non-disclosure, malformed JSON, declared oversized response, redirect refusal, and request timeout. These fixtures should not reach the public Internet.
- File browse/read/search happy paths; common secret-bearing paths and private key/certificate file extensions; parent traversal; symlink escapes where the OS permits creating symlinks; hard-link denial (Unix plus Windows source path; Windows runtime unverified); binary content; invalid/empty limits; bounded/sorted listing.
- Terminal-side credential masking (including quoted/spaced values), control/bidi character escaping, untrusted JSON key sanitization, and obfuscated secret-field-name tests; common JSON output-preservation tests; audit redaction, JSONL sequence handling, event-size cap, and Unix owner-only mode assertions.
- No cost prediction or cost-budget fields/flags. Missing provider-reported cost stays `unknown`.

## Timing methodology

1. **Toolchain setup:** `ci_pipeline.py` times `rustup toolchain install`. It first checks `rustup toolchain list` and annotates when the toolchain was already installed. If preinstalled or cached, this is not a clean first-time download duration. Network throughput and runner cache influence the result.
2. **Release build:** `cargo build --release` wall-clock duration includes compilation and linking under that runner's dependency-cache state. It must not be called a first-ever build unless caches are confirmed clean.
3. **Startup:** the harness runs a new `forge --version` subprocess for every sample (30 default), after three warm-up runs. It reports median, P95, and maximum. It is a repeated process-start benchmark with warm OS filesystem cache, not true cold-boot latency.
4. **Representative work:** measure `doctor`, `files list`, and `config show`; the CLI acceptance suite also covers init, read/search, offline model config checks, and audit behavior. These are basic CLI operations, not the time a future multi-agent coding mission will take.
5. **Binary size:** the runtime report records release executable bytes when available. Memory/RSS is not yet measured consistently across all three OSes; add platform-specific process telemetry before making memory claims.

## Known caveats / release blockers

- The local authoring environment has no Rust compiler/Cargo and cannot resolve `static.rust-lang.org`; no Rust compilation or runtime test has run locally. The new `scripts/static_preflight.py` is intended to rerun available static/package checks and will mark toolchain-dependent work as `BLOCKED`.
- The source still has no committed `Cargo.lock`; dependency selection must be resolved on a connected host, reviewed, committed, and verified with clean locked builds before release.
- Run and verify `cargo fmt --all -- --check` when a compiler host is available. The authoring environment did not have rustfmt.
- Windows opened-handle hard-link count checking now uses the Win32 handle API in source and has a Windows-gated regression test; it is uncompiled and not runtime-verified until the Windows CI job passes. Owner-only `.forge` ACL creation and validation remain incomplete. Fix and review this before claiming cross-platform filesystem security parity.
- TOCTOU-resistant handle-based path traversal has not been proven for hostile same-user races. Symlink checks alone are not a sandbox.
- A sequential role-based proposal workflow, persistent missions, patch preview/apply/checkpoint/undo, and an evidence store are present in source, but the runtime is uncompiled and unverified. No command execution sandbox exists, so build/test/browser execution remains blocked; resume is inspection-only.
- No end-user app installer/distribution exists yet. Installing Rust for a developer build is different from installing a future Forge binary. Do not give a precise app-install duration until binaries/installers are built and tested.
- Provider fixtures now include an Authorization-header check. They still do not replace test coverage for cancellation, concurrent requests, 429/5xx/quota handling, truncated/chunked response bodies, or DNS rebinding; those remain future tests. The added provider tests are authored but unexecuted until the Rust matrix runs. Audit fixtures now also include concurrent appends, malformed history fail-closed, size caps and Unix hard-link refusal.
