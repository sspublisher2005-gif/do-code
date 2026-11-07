#!/usr/bin/env python3
"""Generate or validate Do Code's SHA-256 source inventory, excluding self and caches."""
from __future__ import annotations
import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path

EXCLUDED_PARTS = {".git", "target", "__pycache__", ".docode", "test-results", "node_modules"}
EXCLUDED_SUFFIXES = {".pyc", ".pyo"}


def collect(root: Path) -> list[dict[str, object]]:
    rows = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root)
        if rel.as_posix() == "SOURCE_INVENTORY.json":
            continue
        if any(part in EXCLUDED_PARTS for part in rel.parts):
            continue
        if path.suffix in EXCLUDED_SUFFIXES or path.name.endswith(".env"):
            continue
        raw = path.read_bytes()
        # Normalize CRLF to LF so hashes match across Windows checkouts
        # (autocrlf) and Linux CI checkouts of the same blobs.
        normalized = raw.replace(b"\r\n", b"\n")
        rows.append({"path": rel.as_posix(), "bytes": len(normalized), "sha256": hashlib.sha256(normalized).hexdigest()})
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--check", action="store_true", help="fail if the saved inventory is stale")
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    rows = collect(root)
    target = root / "SOURCE_INVENTORY.json"
    value = {
        "artifact": "Do Code Rust implementation source inventory",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "compiled": False,
        "note": "Source tree inventory only. Hashes verify file bytes, not runtime safety, compilation, or feature parity.",
        "inventory_count": len(rows),
        "inventory": rows,
    }
    if args.check:
        if not target.is_file():
            print("FAIL: SOURCE_INVENTORY.json is missing")
            return 1
        prior = json.loads(target.read_text(encoding="utf-8"))
        if prior.get("inventory") != rows or prior.get("inventory_count") != len(rows) or prior.get("compiled") is not False:
            prior_map = {e.get("path"): e.get("sha256") for e in prior.get("inventory", []) if isinstance(e, dict)}
            for row in rows:
                if prior_map.get(row["path"]) != row["sha256"]:
                    print(f"MISMATCH: {row['path']} recorded={prior_map.get(row['path'])} actual={row['sha256']}")
            prior_paths = set(prior_map)
            current_paths = {row["path"] for row in rows}
            for extra in sorted(prior_paths - current_paths):
                print(f"EXTRA-IN-INVENTORY: {extra}")
            for missing in sorted(current_paths - prior_paths):
                print(f"MISSING-FROM-INVENTORY: {missing}")
            print(f"FAIL: source inventory is stale; run {Path(__file__).name} without --check")
            return 1
        print(f"PASS: source inventory matches all {len(rows)} files (timestamps ignored)")
        return 0
    target.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    print(f"PASS: wrote {target}; {len(rows)} files inventoried")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
