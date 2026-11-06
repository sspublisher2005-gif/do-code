#!/usr/bin/env python3
"""Generate and validate a Cargo-aware CycloneDX SBOM for a trusted checkout.

This invokes Cargo and the pinned cargo-cyclonedx tool; use only for source trees
that have been reviewed. The preliminary offline SBOM remains available when the
Rust toolchain or network is unavailable.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

GENERATOR_VERSION = "0.5.9"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_bom(payload: Any, *, expected_spec: str = "1.5") -> dict[str, Any]:
    """Validate essential CycloneDX structure and report license-field coverage."""
    if not isinstance(payload, dict):
        raise ValueError("SBOM top-level JSON value must be an object")
    if payload.get("bomFormat") != "CycloneDX":
        raise ValueError("SBOM bomFormat must be CycloneDX")
    if str(payload.get("specVersion")) != expected_spec:
        raise ValueError(f"SBOM specVersion must be {expected_spec}")
    metadata = payload.get("metadata")
    if not isinstance(metadata, dict) or not isinstance(metadata.get("component"), dict):
        raise ValueError("SBOM metadata.component is missing")
    root_component = metadata["component"]
    if not root_component.get("name") or not root_component.get("version"):
        raise ValueError("SBOM root component must include name and version")
    components = payload.get("components")
    if not isinstance(components, list) or not components:
        raise ValueError("SBOM components must be a non-empty array")

    bad_indices: list[int] = []
    license_declared = 0
    for index, component in enumerate(components):
        if not isinstance(component, dict) or not component.get("name") or not component.get("version"):
            bad_indices.append(index)
            continue
        licenses = component.get("licenses")
        if isinstance(licenses, list) and licenses:
            license_declared += 1
    if bad_indices:
        raise ValueError(f"SBOM components missing name/version at indices: {bad_indices[:20]}")
    return {
        "bom_format": "CycloneDX",
        "spec_version": expected_spec,
        "root_component": {"name": root_component["name"], "version": root_component["version"]},
        "dependency_components": len(components),
        "components_with_license_fields": license_declared,
        "components_without_license_fields": len(components) - license_declared,
        "license_coverage_percent": round(license_declared * 100 / len(components), 2),
        "important_limitations": [
            "License-field presence is not a legal compatibility review.",
            "An SBOM is not a vulnerability scan; advisory findings must be assessed separately.",
            "Build-script and target inclusion follow the pinned generator and Cargo metadata behavior.",
        ],
    }


def run(command: list[str], *, cwd: Path, timeout: int) -> dict[str, Any]:
    print("$ " + " ".join(command), flush=True)
    started = time.perf_counter()
    try:
        proc = subprocess.run(command, cwd=cwd, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, check=False, timeout=timeout)
    except subprocess.TimeoutExpired as exc:
        output = exc.stdout or ""
        if isinstance(output, bytes):
            output = output.decode("utf-8", errors="replace")
        print(str(output)[-12000:], end="\n", flush=True)
        raise RuntimeError(f"command timed out after {timeout}s: {command[0]}") from exc
    elapsed = round(time.perf_counter() - started, 3)
    output = proc.stdout or ""
    if output:
        print(output, end="" if output.endswith("\n") else "\n", flush=True)
    record = {"command": command, "exit_code": proc.returncode, "elapsed_seconds": elapsed,
              "output_tail": output[-12000:]}
    if proc.returncode != 0:
        raise RuntimeError(f"command failed with exit {proc.returncode}: {' '.join(command)}")
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--toolchain", default="1.99.1")
    parser.add_argument("--skip-install", action="store_true", help="Use an already installed cargo-cyclonedx plugin.")
    parser.add_argument("--report", type=Path, help="Optional JSON provenance/coverage report.")
    args = parser.parse_args()

    root = args.root.resolve()
    manifest = root / "Cargo.toml"
    lockfile = root / "Cargo.lock"
    if not manifest.is_file() or not lockfile.is_file():
        print("ERROR: trusted Cargo.toml and Cargo.lock must exist", file=sys.stderr)
        return 2
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output == manifest or output == lockfile or root in output.parents and output.name in {"Cargo.toml", "Cargo.lock"}:
        print("ERROR: output path conflicts with project inputs", file=sys.stderr)
        return 2

    # The plugin writes beside Cargo.toml. It is generated only in the trusted
    # checkout, then moved into the evidence directory so the source inventory
    # and working tree are not polluted with generated files.
    stem = "docode-cli-cargo-aware-sbom"
    # Only touch names explicitly owned by this generator; never delete a generic
    # `bom.*` file that could belong to the project itself.
    candidates = [root / f"{stem}.json", root / f"{stem}.cdx.json", root / f"{stem}.xml",
                  root / f"{stem}.json.cdx.json", root / f"{stem}.json.json"]
    for candidate in candidates:
        if candidate.is_file():
            candidate.unlink()

    cargo_prefix = [args.cargo, f"+{args.toolchain}"]
    step_records: list[dict[str, Any]] = []
    try:
        if not args.skip_install:
            step_records.append(run(cargo_prefix + ["install", "cargo-cyclonedx", "--version", GENERATOR_VERSION, "--locked"],
                                    cwd=root, timeout=1500))
        started_ns = time.time_ns()
        step_records.append(run(cargo_prefix + ["cyclonedx", "--manifest-path", str(manifest),
                                                  "--format", "json", "--spec-version", "1.5", "--target", "all",
                                                  "--override-filename", f"{stem}.json", "--quiet"],
                                cwd=root, timeout=900))
        existing = [p for p in candidates if p.is_file() and p.stat().st_mtime_ns >= started_ns]
        if not existing:
            # Be tolerant of plugin naming differences, but only accept files
            # that match the explicit custom stem and are JSON.
            existing = [p for p in root.glob(stem + "*.json*") if p.is_file() and p.suffix.lower() == ".json"]
        if not existing:
            raise RuntimeError("cargo-cyclonedx returned success but no expected JSON SBOM file was created")
        source_bom = max(existing, key=lambda p: p.stat().st_mtime_ns)
        payload = json.loads(source_bom.read_text(encoding="utf-8"))
        validation = validate_bom(payload)
        if "docode-cli" not in str(validation["root_component"]["name"]).lower().replace("_", "-"):
            raise ValueError(f"SBOM root component does not appear to describe Forge CLI: {validation['root_component']}")
        output.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        report = {
            "status": "PASS",
            "product": "Forge CLI",
            "generator": {"name": "cargo-cyclonedx", "version": GENERATOR_VERSION},
            "toolchain": args.toolchain,
            "source_manifest_sha256": sha256(manifest),
            "lockfile_sha256": sha256(lockfile),
            "output_path": str(output),
            "output_sha256": sha256(output),
            "validation": validation,
            "steps": step_records,
        }
        if args.report:
            report_path = args.report.resolve()
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print("VALIDATED: Cargo-aware CycloneDX 1.5 SBOM", flush=True)
        print(json.dumps({"output": str(output), "sha256": report["output_sha256"], "validation": validation}, indent=2), flush=True)
    except (RuntimeError, ValueError, json.JSONDecodeError, OSError) as exc:
        error_report = {"status": "FAIL", "product": "Forge CLI", "generator": {"name": "cargo-cyclonedx", "version": GENERATOR_VERSION},
                        "toolchain": args.toolchain, "error": str(exc), "steps": step_records}
        if args.report:
            report_path = args.report.resolve()
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps(error_report, indent=2) + "\n", encoding="utf-8")
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    finally:
        for candidate in candidates:
            try:
                if candidate.is_file():
                    candidate.unlink()
            except OSError:
                print(f"WARNING: unable to remove temporary SBOM file {candidate}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
