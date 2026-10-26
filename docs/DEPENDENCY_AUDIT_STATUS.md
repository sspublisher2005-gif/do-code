# Forge CLI dependency and supply-chain status

**Status as of 2026-10-26: preliminary inventory generated; security advisory status unknown. Not a release clearance.**

## What is available

- `sbom/forge-cli.cdx.json` is a preliminary offline CycloneDX 1.5 component/dependency inventory generated from `Cargo.lock` without invoking Cargo build scripts.
- The generator records 205 third-party locked package coordinates and package archive SHA-256 values where Cargo.lock provides them.
- `scripts/test_sbom_generator.py` checks JSON/spec structure, locked component count, hashes, dependency references, and that no license claims are fabricated.
- `scripts/ci_pipeline.py` runs the source inventory validation, preliminary SBOM generation, and SBOM generator tests as required CI steps.
- `.github/workflows/security-audit.yml` configures a pinned RustSec advisory audit for dependency manifest/lockfile changes, weekly, and manually.

## What has not been verified

- This handoff environment cannot run Cargo or reach the Rust package/advisory hosts. Therefore `cargo audit --locked` has **not** run here.
- The GitHub RustSec workflow has been authored but has not been observed executing. The project's known/advisory vulnerability status is **unknown**, not “clean”.
- The offline SBOM deliberately does not claim licenses, package reachability, or build/release inclusion completeness. It is not the final release SBOM.
- No dependency vulnerability clearance or license clearance is implied by passing the offline generator tests.

## Release exit criteria

1. Run the pinned RustSec workflow on the exact release candidate and review all findings.
2. Resolve or formally block release for any critical/high finding; retain dated output.
3. Generate a final Cargo-aware CycloneDX SBOM using a trusted, pinned tool in the release pipeline.
4. Review licenses and verify the SBOM, binary, lockfile, and source hashes refer to the same release revision.
5. Preserve workflow logs, SBOM, checksums, and the disposition for all advisories.

## Cargo-aware SBOM generator gate (26 October 2026 continuation)

The CI pipeline now invokes `scripts/cargo_aware_sbom.py` in the Linux/Rust 1.99.1 matrix job. It installs pinned `cargo-cyclonedx` 0.5.9, generates CycloneDX 1.5 JSON through Cargo metadata, validates required fields, and records license-field coverage without treating field presence as legal clearance. Eight network-free validator/orchestration tests pass in the authoring environment. The generator itself has **not** run here because Cargo is unavailable; its CI job must run and upload the report before claiming a Cargo-aware SBOM exists for a release. The separate RustSec advisory workflow also remains unexecuted, so vulnerability status is unknown.
