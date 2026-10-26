#!/usr/bin/env python3
"""Generate a deterministic, offline CycloneDX 1.5 component SBOM from Cargo.lock.

This intentionally does not invoke Cargo or build scripts. It can list locked package
coordinates and registry checksums without executing the project. Because Cargo.lock
has no license texts/expressions, this output does NOT claim license completeness.
Use cargo-cyclonedx or another trusted Cargo-aware generator and review licenses for
release qualification when a connected Rust toolchain is available.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import tomllib
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

PURL_RE = re.compile(r"^[A-Za-z0-9._+-]+$")
NAME_RE = re.compile(r"^([A-Za-z0-9_-]+)(?:\s+([0-9][^\s]*))?")


def direct_dependency_classes(manifest: dict[str, Any]) -> dict[str, str]:
    classes: dict[str, str] = {}
    sections = {
        "dependencies": "direct-runtime",
        "dev-dependencies": "direct-development",
        "build-dependencies": "direct-build",
    }
    for section, label in sections.items():
        values = manifest.get(section, {})
        if isinstance(values, dict):
            for alias, value in values.items():
                name = value.get("package", alias) if isinstance(value, dict) else alias
                classes[name] = label
    for target in manifest.get("target", {}).values():
        if not isinstance(target, dict):
            continue
        for section, label in sections.items():
            values = target.get(section, {})
            if isinstance(values, dict):
                for alias, value in values.items():
                    name = value.get("package", alias) if isinstance(value, dict) else alias
                    classes[name] = "target-specific-" + label.removeprefix("direct-")
    return classes


def package_ref(name: str, version: str) -> str:
    if not PURL_RE.fullmatch(name):
        raise ValueError(f"Package name contains unsupported purl characters: {name!r}")
    return f"pkg:cargo/{name}@{version}"


def parse_dep(text: str) -> tuple[str, str | None]:
    match = NAME_RE.match(text)
    if not match:
        raise ValueError(f"Cannot parse locked dependency entry: {text!r}")
    return match.group(1), match.group(2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1],
                        help="Cargo project root")
    parser.add_argument("--output", type=Path, default=None,
                        help="Output JSON path (default: <root>/sbom/forge-cli.cdx.json)")
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    manifest_path = root / "Cargo.toml"
    lock_path = root / "Cargo.lock"
    if not manifest_path.is_file() or not lock_path.is_file():
        raise SystemExit("ERROR: both Cargo.toml and Cargo.lock are required")

    manifest_bytes = manifest_path.read_bytes()
    lock_bytes = lock_path.read_bytes()
    manifest = tomllib.loads(manifest_bytes.decode("utf-8"))
    lock = tomllib.loads(lock_bytes.decode("utf-8"))
    pkg_meta = manifest.get("package", {})
    app_name = pkg_meta.get("name")
    app_version = pkg_meta.get("version")
    if not app_name or not app_version:
        raise SystemExit("ERROR: Cargo.toml package.name/version are required")

    locked = list(lock.get("package", []))
    locked_index: dict[tuple[str, str], dict[str, Any]] = {}
    for package in locked:
        key = (package["name"], package["version"])
        if key in locked_index:
            raise SystemExit(f"ERROR: duplicate Cargo.lock package coordinate {key}")
        locked_index[key] = package

    direct_classes = direct_dependency_classes(manifest)
    components: list[dict[str, Any]] = []
    ref_for: dict[tuple[str, str], str] = {}
    missing_checksums: list[str] = []
    for package in locked:
        name, version = package["name"], package["version"]
        if name == app_name and version == app_version and "source" not in package:
            continue
        ref = package_ref(name, version)
        ref_for[(name, version)] = ref
        component: dict[str, Any] = {
            "type": "library",
            "bom-ref": ref,
            "name": name,
            "version": version,
            "purl": ref,
            "properties": [
                {"name": "forge:dependency-class", "value": direct_classes.get(name, "transitive-or-development")},
            ],
        }
        checksum = package.get("checksum")
        source = package.get("source", "")
        if checksum and re.fullmatch(r"[a-fA-F0-9]{64}", checksum):
            component["hashes"] = [{"alg": "SHA-256", "content": checksum.lower()}]
        elif source.startswith("registry+"):
            missing_checksums.append(f"{name}@{version}")
        if source.startswith("registry+"):
            component["externalReferences"] = [{
                "type": "website",
                "url": f"https://crates.io/crates/{name}/{version}",
            }]
        elif source.startswith("git+"):
            component["properties"].append({"name": "forge:cargo-source", "value": source})
        components.append(component)

    app_ref = package_ref(app_name, app_version)
    dependencies: list[dict[str, Any]] = []
    app_children = sorted(ref_for[(name, version)]
                          for name, version in ref_for
                          if name in direct_classes)
    dependencies.append({"ref": app_ref, "dependsOn": app_children})
    unresolved_edges: list[str] = []
    by_name: dict[str, list[dict[str, Any]]] = {}
    for package in locked:
        by_name.setdefault(package["name"], []).append(package)
    for package in locked:
        name, version = package["name"], package["version"]
        ref = ref_for.get((name, version))
        if not ref:
            continue
        children: set[str] = set()
        for dep_text in package.get("dependencies", []):
            dep_name, dep_version = parse_dep(dep_text)
            candidates = by_name.get(dep_name, [])
            chosen: dict[str, Any] | None = None
            if dep_version is not None:
                chosen = next((item for item in candidates if item["version"] == dep_version), None)
            elif len(candidates) == 1:
                chosen = candidates[0]
            else:
                unresolved_edges.append(f"{name}@{version} -> {dep_text}")
                continue
            if chosen is None:
                unresolved_edges.append(f"{name}@{version} -> {dep_text}")
                continue
            child_ref = ref_for.get((chosen["name"], chosen["version"]))
            if child_ref:
                children.add(child_ref)
        dependencies.append({"ref": ref, "dependsOn": sorted(children)})

    lock_sha = hashlib.sha256(lock_bytes).hexdigest()
    # Deterministic serial number: same Cargo.lock + manifest coordinates yield same BOM ID.
    serial = uuid.uuid5(uuid.NAMESPACE_URL, f"forge-cli:{app_version}:{lock_sha}")
    bom: dict[str, Any] = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
            "tools": [{"vendor": "Forge CLI project tooling", "name": "Forge offline Cargo.lock SBOM generator", "version": "1.0.0"}],
            "component": {"type": "application", "bom-ref": app_ref, "name": app_name, "version": app_version},
            "properties": [
                {"name": "forge:cargo-lock-sha256", "value": lock_sha},
                {"name": "forge:sbom-scope", "value": "Locked Cargo package coordinates and registry checksums; licenses intentionally not asserted"},
            ],
        },
        "components": sorted(components, key=lambda item: (item["name"], item["version"])),
        "dependencies": sorted(dependencies, key=lambda item: item["ref"]),
    }
    output = (args.output or root / "sbom" / "forge-cli.cdx.json").resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(bom, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    # Validate package references and checksum formatting before reporting success.
    refs = {bom["metadata"]["component"]["bom-ref"]} | {c["bom-ref"] for c in components}
    invalid_edges = [edge["ref"] for edge in dependencies if edge["ref"] not in refs]
    invalid_edges += [dep for edge in dependencies for dep in edge.get("dependsOn", []) if dep not in refs]
    if invalid_edges:
        output.unlink(missing_ok=True)
        raise SystemExit(f"ERROR: dangling BOM dependency references: {invalid_edges[:5]}")

    print(f"PASS: wrote {output}")
    print(f"Locked package records: {len(locked)}")
    print(f"Third-party components: {len(components)}")
    print(f"Dependency records: {len(dependencies)}")
    print(f"Cargo.lock SHA-256: {lock_sha}")
    print("License metadata: NOT ASSERTED (needs Cargo-aware generation and license review)")
    print("Vulnerability audit: NOT PERFORMED by this script")
    if unresolved_edges:
        print(f"WARNING: {len(unresolved_edges)} ambiguous/unresolved dependency edges; inspect dependency graph before release")
        for edge in unresolved_edges[:10]:
            print(f"  {edge}")
    if missing_checksums:
        print(f"WARNING: {len(missing_checksums)} registry packages lack valid SHA-256 checksums")
        for component in missing_checksums[:10]:
            print(f"  {component}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
