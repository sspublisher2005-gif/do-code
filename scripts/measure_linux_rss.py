#!/usr/bin/env python3
"""Measure Linux peak-RSS and wrapped wall time for a supplied Do Code binary.

This is a host-specific diagnostic. It uses GNU /usr/bin/time and reports the
wrapper-inclusive wall time separately from verify_cli.py's direct startup timing.
It makes no network calls and never invokes a model.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path


def pctl(values: list[float], q: float) -> float:
    vals = sorted(values)
    return vals[max(0, min(len(vals)-1, round((len(vals)-1)*q)))]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if platform.system() != "Linux":
        print("BLOCKED: GNU /usr/bin/time RSS measurement is Linux-only in this script")
        return 2
    time_bin = Path("/usr/bin/time")
    if not time_bin.is_file() or not os.access(time_bin, os.X_OK):
        print("BLOCKED: /usr/bin/time is not executable")
        return 2
    if not 5 <= args.runs <= 100:
        parser.error("--runs must be between 5 and 100")

    results = []
    with tempfile.TemporaryDirectory(prefix="docode-rss-") as temp:
        root = Path(temp) / "workspace"
        root.mkdir()
        init = subprocess.run([str(binary), "--root", str(root), "init"], capture_output=True, text=True, timeout=15)
        if init.returncode:
            print(init.stdout, init.stderr)
            return init.returncode
        (root / "src").mkdir()
        for i in range(30):
            (root / "src" / f"fixture_{i:02d}.rs").write_text(f"// fixture {i}\nfn f{i}() -> usize {{ {i} }}\n", encoding="utf-8")
        operations = {
            "version": ["--version"],
            "doctor": ["--root", str(root), "doctor"],
            "files_list": ["--root", str(root), "files", "list"],
            "config_show": ["--root", str(root), "config", "show"],
        }
        for name, argv in operations.items():
            # Warm up once; report only the fixed-count measured runs below.
            warm = subprocess.run([str(time_bin), "-f", "%M", "--", str(binary), *argv], capture_output=True, text=True, timeout=15)
            if warm.returncode:
                raise RuntimeError(f"warm-up failed for {name}: {warm.stdout}\n{warm.stderr}")
            rss_values: list[float] = []
            elapsed_values: list[float] = []
            for _ in range(args.runs):
                started = time.perf_counter()
                proc = subprocess.run([str(time_bin), "-f", "%M", "--", str(binary), *argv], capture_output=True, text=True, timeout=15)
                elapsed = (time.perf_counter() - started) * 1000.0
                if proc.returncode:
                    raise RuntimeError(f"measurement failed for {name}: {proc.stdout}\n{proc.stderr}")
                lines = proc.stderr.strip().splitlines()
                rss_text = lines[-1].strip() if lines else ""
                if not rss_text.isdigit():
                    raise RuntimeError(f"could not parse GNU time RSS for {name}: {proc.stderr!r}")
                rss_values.append(float(rss_text))
                elapsed_values.append(elapsed)
            results.append({
                "operation": name,
                "runs": args.runs,
                "peak_rss_kib_median": statistics.median(rss_values),
                "peak_rss_kib_max": max(rss_values),
                "wrapper_inclusive_wall_ms_median": round(statistics.median(elapsed_values), 3),
                "wrapper_inclusive_wall_ms_p95": round(pctl(elapsed_values, .95), 3),
                "note": "GNU time observes process peak RSS; wall times include launching the GNU time wrapper and are not comparable to direct subprocess benchmarks.",
            })
    report = {
        "product": "Do Code 0.7.0-rs.1 (Rust implementation WIP)",
        "measured_at_utc": datetime.now(timezone.utc).isoformat(),
        "environment": {"platform": platform.platform(), "python": platform.python_version(), "machine": platform.machine(), "binary_bytes": binary.stat().st_size, "binary_sha256": sha256(binary)},
        "method": {"network_calls": 0, "model_calls": 0, "warmup_per_operation": 1, "measured_runs_per_operation": args.runs, "tool": "/usr/bin/time -f %M", "cache_state": "warmed by one warm-up invocation; not a cold boot"},
        "results": results,
        "limitations": ["Single Linux container only; not a cross-device promise.", "No installation time measured.", "No macOS or Windows figures collected.", "Wrapper-inclusive wall time is separate from direct startup/operation timing.", "Measurements do not establish sandbox security or feature parity."],
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"PASS: wrote {args.report}")
    for result in results:
        print(f"{result['operation']}: peak RSS median {result['peak_rss_kib_median']} KiB; max {result['peak_rss_kib_max']} KiB; wrapper wall median {result['wrapper_inclusive_wall_ms_median']} ms")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
