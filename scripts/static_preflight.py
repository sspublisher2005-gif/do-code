#!/usr/bin/env python3
"""Run repeatable, non-Rust static checks when the Rust toolchain is unavailable.

This is not a Rust parser, compiler, test runner, or security certification.
It deliberately reports toolchain/runtime checks as BLOCKED instead of PASS.
"""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    yaml = None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--archive", type=Path, default=None)
    parser.add_argument("--binary", type=Path, default=None, help="optional separately built docode/docode.exe executable to probe; this does not replace the black-box test harness")
    parser.add_argument("--report", type=Path, default=None)
    args = parser.parse_args()
    root = args.repo.resolve()
    archive = (args.archive or root.parent / f"{root.name}.zip").resolve()
    report_path = args.report.resolve() if args.report else root.parent / "docode_cli_Static_Preflight_2026-10-24.json"
    results: list[dict[str, Any]] = []
    blockers: list[dict[str, str]] = []

    def record(name: str, status: str, detail: str) -> None:
        results.append({"name": name, "status": status, "detail": detail})
        print(f"{status:7} {name}: {detail}")

    def check(name: str, fn) -> None:
        try:
            detail = fn()
            record(name, "PASS", str(detail or "check completed"))
        except Exception as exc:
            record(name, "FAIL", f"{type(exc).__name__}: {exc}")

    def toml_check() -> str:
        cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text(encoding="utf-8"))
        sample = tomllib.loads((root / "docode.config.toml.example").read_text(encoding="utf-8"))
        assert cargo["package"]["edition"] == "2024"
        assert cargo["package"]["rust-version"] == "1.88"
        assert cargo["bin"][0]["name"] == "docode"
        assert toolchain["toolchain"]["channel"]
        assert sample["privacy"]["telemetry"] is False
        assert "net" in cargo["dependencies"]["tokio"]["features"]
        assert "io-util" in cargo["dependencies"]["tokio"]["features"]
        return "Cargo, pinned toolchain and example config are parseable TOML; expected CLI/MSRV/test features present"

    check("TOML manifests and config", toml_check)

    def python_sources() -> str:
        paths = sorted((root / "scripts").glob("*.py"))
        assert paths, "no Python harness scripts found"
        for path in paths:
            ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        return f"parsed {len(paths)} Python harness files with ast.parse; this does not run their behavior"

    check("Python harness syntax", python_sources)

    def rust_delimiter_sanity() -> str:
        # This is intentionally only a delimiter sanity scan, not a Rust parser or compiler.
        # It skips ordinary/raw strings and nested comments before balancing (), [] and {}.
        paths = sorted(root.glob("src/*.rs")) + sorted(root.glob("tests/*.rs")) + [root / "build.rs"]
        pairs = {")": "(", "]": "[", "}": "{"}
        for path in paths:
            source = path.read_text(encoding="utf-8")
            stack: list[tuple[str, int]] = []
            i = 0
            while i < len(source):
                if source.startswith("//", i):
                    end = source.find("\n", i + 2)
                    i = len(source) if end < 0 else end + 1
                    continue
                if source.startswith("/*", i):
                    depth, i = 1, i + 2
                    while i < len(source) and depth:
                        if source.startswith("/*", i): depth += 1; i += 2
                        elif source.startswith("*/", i): depth -= 1; i += 2
                        else: i += 1
                    assert depth == 0, f"unterminated block comment in {path}"
                    continue
                # Raw strings: r#"..."#, br#"..."#, including zero or more hashes.
                raw_start = None
                for prefix in ("br", "r"):
                    if source.startswith(prefix, i):
                        j = i + len(prefix)
                        while j < len(source) and source[j] == "#": j += 1
                        if j < len(source) and source[j] == '"':
                            raw_start = (j + 1, source[i + len(prefix):j])
                            break
                if raw_start is not None:
                    content_start, hashes = raw_start
                    terminator = '"' + hashes
                    end = source.find(terminator, content_start)
                    assert end >= 0, f"unterminated raw string in {path}"
                    i = end + len(terminator)
                    continue
                # Ordinary/byte strings. Rust escaped quotes and backslashes are skipped.
                quote_at = i + 1 if source.startswith('b"', i) else i
                if quote_at < len(source) and source[quote_at] == '"':
                    i = quote_at + 1
                    while i < len(source):
                        if source[i] == "\\": i += 2
                        elif source[i] == '"': i += 1; break
                        else: i += 1
                    assert i <= len(source) and source[i - 1:i] == '"', f"unterminated string in {path}"
                    continue
                # Character literal vs lifetime: only treat quote as a char literal if a closing quote follows.
                char_start = i + 1 if source.startswith("b'", i) else i
                if char_start < len(source) and source[char_start] == "'":
                    candidate = char_start + 1
                    if candidate < len(source) and source[candidate] == "\\": candidate += 2
                    else: candidate += 1
                    if candidate < len(source) and source[candidate] == "'":
                        i = candidate + 1
                        continue
                ch = source[i]
                if ch in "([{": stack.append((ch, i))
                elif ch in ")]}":
                    assert stack and stack[-1][0] == pairs[ch], f"mismatched {ch!r} in {path} at offset {i}"
                    stack.pop()
                i += 1
            assert not stack, f"unclosed delimiter {stack[-1][0]!r} in {path} at offset {stack[-1][1]}"
        return f"balanced delimiters in {len(paths)} Rust source files after skipping strings/comments; not a type/syntax check"

    check("Rust source delimiter sanity (not a parser)", rust_delimiter_sanity)

    def workflow() -> str:
        if yaml is None:
            raise RuntimeError("PyYAML is not installed; install pyyaml to validate the workflow YAML")
        doc = yaml.load((root / ".github/workflows/rust-ci.yml").read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        matrix = doc["jobs"]["verify"]["strategy"]["matrix"]
        oses = set(matrix["os"])
        versions = set(matrix["toolchain"])
        assert oses == {"ubuntu-latest", "macos-latest", "windows-latest"}, oses
        assert versions == {"1.88.0", "1.99.1"}, versions
        assert len(oses) * len(versions) == 6
        assert "scripts/ci_pipeline.py" in str(doc["jobs"]["verify"]["steps"])
        return "workflow YAML parses; six matrix combinations configured (not evidence any job ran)"

    check("Cross-platform workflow structure", workflow)

    def security_workflow() -> str:
        if yaml is None:
            raise RuntimeError("PyYAML is not installed; cannot parse the security audit workflow")
        path = root / ".github/workflows/security-audit.yml"
        doc = yaml.load(path.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        assert "schedule" in doc["on"], "dependency audit must run on a schedule"
        steps = doc["jobs"]["cargo-audit"]["steps"]
        action_refs = [step.get("uses", "") for step in steps]
        assert any(item == "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1" for item in action_refs), action_refs
        assert any(item == "rustsec/audit-check@69366f33c96575abad1ee0dba8212993eecbe998" for item in action_refs), action_refs
        permissions = doc["permissions"]
        assert permissions["contents"] == "read"
        assert permissions["checks"] == "write" and permissions["issues"] == "write"
        return "pinned RustSec workflow parses, uses least-necessary declared permissions, scans Cargo changes and has weekly/manual triggers; the workflow itself has not run here"

    check("Dependency advisory workflow structure", security_workflow)

    def sbom_gate() -> str:
        generator = (root / "scripts/generate_sbom.py").read_text(encoding="utf-8")
        tests = (root / "scripts/test_sbom_generator.py").read_text(encoding="utf-8")
        pipeline = (root / "scripts/ci_pipeline.py").read_text(encoding="utf-8")
        sbom = json.loads((root / "sbom/docode-cli.cdx.json").read_text(encoding="utf-8"))
        assert sbom["bomFormat"] == "CycloneDX" and sbom["specVersion"] == "1.5"
        assert len(sbom["components"]) == 205, len(sbom["components"])
        assert "Vulnerability audit: NOT PERFORMED" in generator
        assert "Generate preliminary offline CycloneDX SBOM" in pipeline
        assert "Test offline SBOM generator" in pipeline
        assert "Test Cargo-aware SBOM validator" in pipeline
        assert "Test CI required-gate evaluator" in pipeline
        assert "Generate Cargo-aware CycloneDX SBOM" in pipeline
        cargo_aware = (root / "scripts/cargo_aware_sbom.py").read_text(encoding="utf-8")
        cargo_aware_tests = (root / "scripts/test_cargo_aware_sbom.py").read_text(encoding="utf-8")
        assert 'GENERATOR_VERSION = "0.5.9"' in cargo_aware
        assert '"--format", "json", "--spec-version", "1.5"' in cargo_aware
        assert "validate_bom" in cargo_aware_tests and "test_rejects_wrong_format" in cargo_aware_tests
        assert "does not assert crate licenses" in (root / "sbom/README.md").read_text(encoding="utf-8")
        assert "test_license_claims_are_not_fabricated" in tests
        return "preliminary offline SBOM has 205 locked components; CI also requires a pinned Cargo-aware SBOM in the Linux/Rust 1.99.1 job; license coverage is reported, not treated as legal clearance, and advisories remain a separate gate"

    check("Preliminary SBOM and mandatory CI gates", sbom_gate)

    def inventory_check() -> str:
        inventory_path = root / "SOURCE_INVENTORY.json"
        inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
        entries = inventory["inventory"]
        assert inventory["compiled"] is False
        checked = 0
        for entry in entries:
            path = root / entry["path"]
            assert path.is_file(), f"missing inventory file: {entry['path']}"
            payload = path.read_bytes()
            assert len(payload) == entry["bytes"], f"size changed: {entry['path']}"
            assert hashlib.sha256(payload).hexdigest() == entry["sha256"], f"hash changed: {entry['path']}"
            checked += 1
        return f"verified exact byte lengths and SHA-256 for {checked} inventory entries (inventory cannot hash itself)"

    check("Source inventory hashes", inventory_check)

    def cost_policy() -> str:
        config = (root / "src/config.rs").read_text(encoding="utf-8")
        mission = (root / "src/mission.rs").read_text(encoding="utf-8")
        main = (root / "src/main.rs").read_text(encoding="utf-8")
        cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        assert "max_cost_usd" not in config and "estimated_cost_usd" not in config
        assert "max_cost_usd" not in mission and "estimated_cost_usd" not in mission
        assert "max_cost_usd" not in main and "estimated_cost_usd" not in main
        assert "provider_reported_cost_usd" in (root / "src/provider.rs").read_text(encoding="utf-8")
        assert cargo["package"]["name"] == "docode-cli"
        return "active config/domain/CLI contain no USD estimate or cost-stop field; provider-reported actual cost field is present"

    check("No predicted-cost stop in active product model", cost_policy)

    def security_regressions() -> str:
        redaction = (root / "src/redaction.rs").read_text(encoding="utf-8")
        files = (root / "src/files.rs").read_text(encoding="utf-8")
        audit = (root / "src/audit.rs").read_text(encoding="utf-8")
        provider = (root / "src/provider.rs").read_text(encoding="utf-8")
        assert "sensitive value, with spaces" in redaction
        assert "safe_terminal(&redaction::redact(&content))" in (root / "src/main.rs").read_text(encoding="utf-8")
        assert "safe_line" in files and "redaction::redact" in files
        storage = (root / "src/storage.rs").read_text(encoding="utf-8")
        config = (root / "src/config.rs").read_text(encoding="utf-8")
        transcript = (root / "src/message_store.rs").read_text(encoding="utf-8")
        assert "reject_hardlink(&after, &path)" in files
        assert "reject_hardlink_handle(&file, &path)" in files
        assert "reject_hardlink_handle(&file, &log_path)" in audit
        assert "reject_hardlink_handle(&file, path)" in storage
        assert "GetFileInformationByHandle" in storage
        assert "reject_hardlink_handle(&file, &path)" in config
        assert "opened_handle_rejects_hardlinked_state_file_on_windows" in storage
        assert "refuses_hardlinked_audit_log_without_modifying_external_target" in audit
        assert "sends_configured_bearer_token_in_authorization_header" in provider
        assert "anthropic_adapter_sets_key_header_and_parses_text_usage" in provider
        assert "google_adapter_uses_key_header_and_parses_candidate_parts" in provider
        assert "provider_request_timeout_is_enforced" in provider
        assert "transcript_append_is_serialized_across_threads" in transcript
        assert "sanitizes_untrusted_json_object_keys_as_well_as_values" in redaction
        assert "fn is_secret_key" in redaction
        ci_pipeline = (root / "scripts/ci_pipeline.py").read_text(encoding="utf-8")
        assert "cargo fmt --all -- --check" in ci_pipeline
        assert "Cargo.lock is missing" in ci_pipeline and "--all-targets" in ci_pipeline and "--locked" in ci_pipeline
        assert '"cargo test --all-targets --locked"' in ci_pipeline
        assert '"cargo test --locked (unit + integration)"' not in ci_pipeline
        assert "def evaluate_release_gates" in ci_pipeline and "required gate was not recorded" in ci_pipeline
        assert "Test CI required-gate evaluator" in ci_pipeline
        harness = (root / "scripts/verify_cli.py").read_text(encoding="utf-8")
        assert "elapsed_ms = (time.perf_counter() - started) * 1000.0" in harness
        assert '"Windows .docode ACL hardening"' in harness
        assert "Validate source inventory hashes" in ci_pipeline
        assert "Generate preliminary offline CycloneDX SBOM" in ci_pipeline
        assert "Test offline SBOM generator" in ci_pipeline
        assert "Generate Cargo-aware CycloneDX SBOM" in ci_pipeline
        assert "Test Cargo-aware SBOM validator" in ci_pipeline
        inventory_tool = (root / "scripts/update_source_inventory.py").read_text(encoding="utf-8")
        assert "--check" in inventory_tool and "SOURCE_INVENTORY.json" in inventory_tool
        return "source includes terminal redaction, opened-handle hard-link checks on Windows and Unix, transcript/audit/provider fixture regression markers, rustfmt CI gate, source-inventory and SBOM gates"

    check("Security regression coverage markers", security_regressions)

    def reference_archive() -> str:
        path = root / "reference/docode_cli_TS_0.6.0-ts.2_MemoryDecision_Merge_WIP.zip"
        assert path.is_file(), "preserved TypeScript reference archive is missing"
        with zipfile.ZipFile(path) as zf:
            bad = zf.testzip()
            assert bad is None, f"reference archive has a corrupt member: {bad}"
            count = len(zf.namelist())
        return f"preserved previous TypeScript reference ZIP is structurally readable ({count} members)"

    check("Preserved TypeScript reference archive", reference_archive)

    def packaged_archive() -> str:
        assert archive.is_file(), f"package archive not found yet: {archive}"
        with zipfile.ZipFile(archive) as zf:
            bad = zf.testzip()
            assert bad is None, f"archive has corrupt member: {bad}"
            raw_names = set(zf.namelist())
            prefix = root.name + "/"
            names = {name[len(prefix):] for name in raw_names if name.startswith(prefix)}
            required = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "SOURCE_INVENTORY.json", "scripts/verify_cli.py", "scripts/ci_pipeline.py", "scripts/static_preflight.py", "scripts/generate_sbom.py", "scripts/test_sbom_generator.py", "scripts/cargo_aware_sbom.py", "scripts/test_cargo_aware_sbom.py", "scripts/test_ci_pipeline_gates.py", "scripts/update_source_inventory.py", "scripts/measure_linux_rss.py", ".github/workflows/security-audit.yml", "sbom/docode-cli.cdx.json", "reference/docode_cli_TS_0.6.0-ts.2_MemoryDecision_Merge_WIP.zip"}
            missing = required - names
            assert not missing, f"package missing required files: {sorted(missing)}"
            forbidden_fragments = ("target/", "__pycache__/", ".docode/", ".git/")
            assert not any(any(fragment in name for fragment in forbidden_fragments) or name.endswith(".pyc") or name.endswith(".env") for name in names), "build/cache/private artifact leaked into source ZIP"
            return f"source ZIP verifies ({len(raw_names)} members); required sources present, no build/cache/private state or Python bytecode"

    check("Packaged ZIP integrity/content", packaged_archive)

    for command in ("rustc", "cargo", "rustfmt", "rustup"):
        if shutil.which(command) is None:
            blockers.append({"name": f"{command} availability", "detail": f"{command} is not installed in this environment"})
    if not (root / "Cargo.lock").is_file():
        blockers.append({"name": "Cargo.lock", "detail": "not generated; needs dependency resolution from a connected Rust host"})

    # A release binary may be delivered beside the source tree instead of under target/release.
    # Probe it honestly when supplied; do not infer that no binary exists just because Cargo is absent.
    supplied_binary = args.binary.resolve() if args.binary else None
    if supplied_binary is None:
        candidates = [root / "target/release/docode", root / "target/release/docode.exe"]
        supplied_binary = next((candidate for candidate in candidates if candidate.is_file()), None)
    if supplied_binary is None:
        blockers.append({"name": "Rust release build output", "detail": "no release binary was supplied and target/release/docode[.exe] is absent; Cargo build cannot be reproduced without the Rust toolchain"})
    else:
        def binary_probe() -> str:
            assert supplied_binary.is_file(), f"binary not found: {supplied_binary}"
            result = subprocess.run([str(supplied_binary), "--version"], capture_output=True, text=True, timeout=10)
            assert result.returncode == 0, f"--version exited {result.returncode}: {result.stderr.strip()}"
            version_text = (result.stdout or result.stderr).strip()
            assert version_text.startswith("docode "), f"unexpected --version output: {version_text!r}"
            return f"separate release binary exists and --version works: {version_text}; this probe is not the full black-box suite"
        check("Supplied release binary probe", binary_probe)

    blockers.append({"name": "macOS/Windows runtime", "detail": "this authoring runner is Linux; actual OS-matrix runs must be obtained from the configured CI workflow"})
    for item in blockers:
        record(item["name"], "BLOCKED", item["detail"])

    summary = {
        "artifact": "Do Code 0.7.0-rs.1 (Rust implementation WIP)",
        "measured_at_utc": __import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat(),
        "environment": {"system": sys.platform, "python": sys.version.split()[0], "rustc": shutil.which("rustc"), "cargo": shutil.which("cargo"), "rustfmt": shutil.which("rustfmt")},
        "results": results,
        "blockers": blockers,
        "summary": {"pass": sum(r["status"] == "PASS" for r in results), "fail": sum(r["status"] == "FAIL" for r in results), "blocked": len(blockers)},
        "authored_rust_test_attributes": sum(
            path.read_text(encoding="utf-8").count("#[test]") + path.read_text(encoding="utf-8").count("#[tokio::test]")
            for path in (sorted(root.glob("src/*.rs")) + sorted(root.glob("tests/*.rs")))
        ),
        "rust_test_execution_status": "BLOCKED: no rustc/cargo in authoring environment; authored attribute count is not a pass count. A supplied binary probe is distinct from cargo test.",
        "scope_note": "Static preflight does not replace Cargo tests or the black-box acceptance suite. A supplied binary probe runs only --version. A pass does not assert all Rust source compiles or runtime behavior is correct. Do not report blocked Rust tests as passes.",
    }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"\nStatic preflight report: {report_path}")
    print(f"Summary: {summary['summary']['pass']} PASS, {summary['summary']['fail']} FAIL, {summary['summary']['blocked']} BLOCKED")
    return 1 if summary["summary"]["fail"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
