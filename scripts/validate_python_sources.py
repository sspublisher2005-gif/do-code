#!/usr/bin/env python3
from __future__ import annotations

import ast
import sys
from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    scripts = sorted((root / "scripts").glob("*.py"))
    if not scripts:
        print("FAIL: no Python harness scripts found", file=sys.stderr)
        return 1

    failures = []
    for path in scripts:
        try:
            ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        except (OSError, SyntaxError) as exc:
            failures.append((path, exc))

    if failures:
        for path, exc in failures:
            print(f"FAIL: {path.relative_to(root)}: {exc}", file=sys.stderr)
        return 1

    print(f"PASS: Python syntax valid for {len(scripts)} harness scripts.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
