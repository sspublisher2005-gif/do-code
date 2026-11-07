# Do Code preliminary offline SBOM

`docode-cli.cdx.json` is a deterministic-component-inventory CycloneDX 1.5 JSON document generated from the checked-in `Cargo.lock` using `scripts/generate_sbom.py`.

It records the 205 third-party locked crate package coordinates, crates.io references, SHA-256 package archive checksums available in the lockfile, and resolved dependency relationships. The serial number is derived from the lockfile hash and package identity. The generation timestamp changes on regeneration.

**Scope limits:** this offline generator does not invoke Cargo, does not execute build scripts, does not assert crate licenses, does not classify built/reachable dependencies, and does not perform a vulnerability audit. It is not a substitute for a Cargo-aware release SBOM or a `cargo audit` result. The license and advisory scan remain release gates.

Regenerate:

```sh
python scripts/generate_sbom.py --root . --output sbom/docode-cli.cdx.json
python scripts/test_sbom_generator.py
```

The test suite verifies JSON structure, package counts, registry checksums, dependency references, and the explicit non-assertion of license metadata.

## Cargo-aware CI SBOM

The cross-platform CI pipeline additionally generates a Cargo-aware CycloneDX 1.5 JSON SBOM once on the Linux/Rust 1.99.1 matrix job using the pinned `cargo-cyclonedx` 0.5.9 generator. The resulting artifact and a coverage/provenance report are placed in that CI job's `test-results/sbom/` folder. The generator invokes Cargo metadata and is therefore intended only for the reviewed, trusted checkout (not an untrusted third-party project).

That CI artifact is not present in this offline authoring environment because `cargo` is absent and the crate registry is unreachable. The checked-in `docode-cli.cdx.json` remains the preliminary lockfile-only inventory. Neither SBOM generation path substitutes for dependency advisory scanning, license compatibility review, or a security sign-off.
