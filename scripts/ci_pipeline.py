#!/usr/bin/env python3
"""Measure toolchain setup/build time, run Rust checks, and black-box test Forge.

Used by the Linux/macOS/Windows CI matrix. Python is a CI test harness only, not a
runtime requirement for the compiled Do Code CLI.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


CORE_REQUIRED_GATES = {
    "Validate source inventory hashes",
    "Generate preliminary offline CycloneDX SBOM",
    "Test offline SBOM generator",
    "Test Cargo-aware SBOM validator",
    "Test CI required-gate evaluator",
    "cargo fmt --all -- --check",
    "cargo check --all-targets --locked",
    "cargo test --all-targets --locked",
    "cargo clippy --all-targets --locked -- -D warnings",
    "cargo build --release --locked (cold/warm runner conditions recorded)",
    "Black-box CLI acceptance tests + startup/operation benchmark",
    "Extended offline acceptance tests (memory, decisions, freeze, fail-closed sandbox)",
}


def evaluate_release_gates(steps: list[dict[str, Any]], *, require_cargo_aware_sbom: bool,
                           require_linux_rss: bool) -> list[dict[str, str]]:
    """Fail on any missing/failed required gate, including renamed test steps."""
    names = [str(step.get("name", "")) for step in steps]
    duplicates = sorted({name for name in names if names.count(name) > 1 and name})
    failures: list[dict[str, str]] = [
        {"name": name, "status": "DUPLICATE", "detail": "gate name appears more than once"}
        for name in duplicates
    ]
    required = set(CORE_REQUIRED_GATES)
    if require_cargo_aware_sbom:
        required.add("Generate Cargo-aware CycloneDX SBOM")
    if require_linux_rss:
        required.add("Linux peak-RSS diagnostic")
    by_name = {str(step.get("name", "")): step for step in steps}
    for name in sorted(required):
        step = by_name.get(name)
        if step is None:
            failures.append({"name": name, "status": "MISSING", "detail": "required gate was not recorded"})
        elif step.get("status") != "PASS":
            failures.append({"name": name, "status": str(step.get("status", "UNKNOWN")),
                             "detail": str(step.get("detail", step.get("output_tail", "required gate did not pass")))} )
    return failures


def run_step(name: str, command: list[str], *, timeout: int = 1800) -> dict[str, Any]:
    print(f"\n=== {name} ===", flush=True)
    started = time.perf_counter()
    try:
        proc = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              check=False, timeout=timeout)
        output = proc.stdout or ""
        if output:
            print(output, end="" if output.endswith("\n") else "\n", flush=True)
        elapsed = time.perf_counter() - started
        status = "PASS" if proc.returncode == 0 else "FAIL"
        print(f"{status}: {name}; exit={proc.returncode}; elapsed={elapsed:.3f}s", flush=True)
        return {"name": name, "status": status, "command": command, "exit_code": proc.returncode,
                "elapsed_seconds": round(elapsed, 3), "output_tail": output[-12000:]}
    except subprocess.TimeoutExpired as exc:
        elapsed = time.perf_counter() - started
        output = exc.stdout or ""
        if isinstance(output, bytes):
            output = output.decode("utf-8", errors="replace")
        print(f"FAIL: {name} timed out after {timeout}s", flush=True)
        return {"name": name, "status": "FAIL", "command": command, "exit_code": None,
                "elapsed_seconds": round(elapsed, 3), "timeout_seconds": timeout,
                "output_tail": str(output)[-12000:]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", default="1.99.1")
    parser.add_argument("--benchmark-runs", type=int, default=30)
    parser.add_argument("--report-dir", type=Path, default=Path("test-results"))
    args = parser.parse_args()
    if not 5 <= args.benchmark_runs <= 500:
        parser.error("--benchmark-runs must be between 5 and 500")

    args.report_dir.mkdir(parents=True, exist_ok=True)
    steps: list[dict[str, Any]] = []
    rustup = shutil.which("rustup")
    cargo = shutil.which("cargo")
    if rustup is None or cargo is None:
        print("BLOCKED: rustup and cargo must be available on this CI runner.", file=sys.stderr)
        return 2

    listed = run_step("Inspect installed Rust toolchains", [rustup, "toolchain", "list"], timeout=60)
    steps.append(listed)
    already_installed = listed["status"] == "PASS" and any(
        (line.strip().split(" ")[0] == args.toolchain or line.strip().split(" ")[0].startswith(args.toolchain + "-"))
        for line in listed.get("output_tail", "").splitlines() if line.strip()
    )
    install_step = run_step(
        "Install or verify pinned Rust toolchain (elapsed reported; may be preinstalled/cached)",
        [rustup, "toolchain", "install", args.toolchain, "--profile", "minimal", "--component", "rustfmt", "--component", "clippy"],
        timeout=1200,
    )
    install_step["was_preinstalled_before_step"] = already_installed
    install_step["interpretation"] = "not a clean-download measure" if already_installed else "toolchain setup elapsed on this runner; network/cache conditions apply"
    steps.append(install_step)
    if install_step["status"] != "PASS":
        write_reports(args.report_dir, args.toolchain, steps, None, lockfile_was_present=Path("Cargo.lock").is_file())
        return 1

    toolchain_prefix = f"+{args.toolchain}"
    lockfile = Path("Cargo.lock")
    lockfile_was_present = lockfile.is_file()
    if not lockfile_was_present:
        print("ERROR: Cargo.lock is missing. Resolve dependencies on a connected maintainer machine, review the lockfile, update SOURCE_INVENTORY.json, and commit both before CI.", file=sys.stderr)
        write_reports(args.report_dir, args.toolchain, steps, None, lockfile_was_present=False)
        return 1
    # CI consumes and records the exact reviewed lockfile; it never resolves a
    # fresh dependency graph that could silently differ from the source revision.
    lock_artifact = args.report_dir / "Cargo.lock.artifact"
    shutil.copy2(lockfile, lock_artifact)

    inventory_script = Path(__file__).with_name("update_source_inventory.py").resolve()
    steps.append(run_step(
        "Validate source inventory hashes",
        [sys.executable, str(inventory_script), "--root", str(Path.cwd().resolve()), "--check"],
        timeout=120))

    # Produce a preliminary offline SBOM and test the generator without invoking
    # Cargo plugins or executing project build scripts. This is useful evidence,
    # but is NOT a substitute for the scheduled cargo-audit workflow or a final
    # license-complete release SBOM.
    sbom_dir = args.report_dir / "sbom"
    sbom_path = sbom_dir / "docode-cli.cdx.json"
    sbom_script = Path(__file__).with_name("generate_sbom.py").resolve()
    sbom_test_script = Path(__file__).with_name("test_sbom_generator.py").resolve()
    steps.append(run_step(
        "Generate preliminary offline CycloneDX SBOM",
        [sys.executable, str(sbom_script), "--root", str(Path.cwd().resolve()),
         "--output", str(sbom_path.resolve())], timeout=120))
    steps.append(run_step(
        "Test offline SBOM generator",
        [sys.executable, str(sbom_test_script)], timeout=120))
    cargo_sbom_test_script = Path(__file__).with_name("test_cargo_aware_sbom.py").resolve()
    steps.append(run_step(
        "Test Cargo-aware SBOM validator",
        [sys.executable, str(cargo_sbom_test_script)], timeout=120))
    ci_gate_test_script = Path(__file__).with_name("test_ci_pipeline_gates.py").resolve()
    steps.append(run_step(
        "Test CI required-gate evaluator",
        [sys.executable, str(ci_gate_test_script)], timeout=120))

    checks = [
        ("cargo fmt --all -- --check", [cargo, toolchain_prefix, "fmt", "--all", "--", "--check"], 600),
        ("cargo check --all-targets --locked", [cargo, toolchain_prefix, "check", "--all-targets", "--locked"], 1800),
        ("cargo test --all-targets --locked", [cargo, toolchain_prefix, "test", "--all-targets", "--locked"], 1800),
        ("cargo clippy --all-targets --locked -- -D warnings", [cargo, toolchain_prefix, "clippy", "--all-targets", "--locked", "--", "-D", "warnings"], 1800),
        ("cargo build --release --locked (cold/warm runner conditions recorded)", [cargo, toolchain_prefix, "build", "--release", "--locked"], 2400),
    ]
    for name, command, timeout in checks:
        steps.append(run_step(name, command, timeout=timeout))

    cargo_aware_sbom_required = sys.platform.startswith("linux") and args.toolchain == "1.99.1"
    if cargo_aware_sbom_required:
        cargo_sbom_script = Path(__file__).with_name("cargo_aware_sbom.py").resolve()
        cargo_sbom_output = sbom_dir / "docode-cli-cargo-aware.cdx.json"
        cargo_sbom_report = sbom_dir / "cargo-aware-sbom-report.json"
        steps.append(run_step(
            "Generate Cargo-aware CycloneDX SBOM",
            [sys.executable, str(cargo_sbom_script), "--root", str(Path.cwd().resolve()),
             "--output", str(cargo_sbom_output.resolve()), "--report", str(cargo_sbom_report.resolve()),
             "--cargo", cargo, "--toolchain", args.toolchain],
            timeout=2400))
    else:
        steps.append({"name": "Generate Cargo-aware CycloneDX SBOM", "status": "NOT_APPLICABLE",
                      "detail": "Generated once in the Linux Rust 1.99.1 matrix job to avoid redundant generator builds on every platform/toolchain."})

    binary = Path("target/release/docode.exe" if os.name == "nt" else "target/release/docode")
    runtime_report_path = args.report_dir / "forge-runtime.json"
    runtime_step: dict[str, Any] | None = None
    if binary.is_file():
        runtime_script = Path(__file__).with_name("verify_cli.py").resolve()
        runtime_step = run_step(
            "Black-box CLI acceptance tests + startup/operation benchmark",
            [sys.executable, str(runtime_script), "--binary", str(binary.resolve()),
             "--benchmark-runs", str(args.benchmark_runs), "--report", str(runtime_report_path.resolve())],
            timeout=900,
        )
        steps.append(runtime_step)
        extended_report_path = args.report_dir / "extended-offline-acceptance.json"
        extended_script = Path(__file__).with_name("extended_offline_acceptance.py").resolve()
        extended_step = run_step(
            "Extended offline acceptance tests (memory, decisions, freeze, fail-closed sandbox)",
            [sys.executable, str(extended_script), "--binary", str(binary.resolve()),
             "--report", str(extended_report_path.resolve())],
            timeout=900,
        )
        steps.append(extended_step)
        if sys.platform.startswith("linux"):
            rss_script = Path(__file__).with_name("measure_linux_rss.py").resolve()
            rss_report_path = args.report_dir / "forge-linux-rss.json"
            rss_step = run_step(
                "Linux peak-RSS diagnostic",
                [sys.executable, str(rss_script), "--binary", str(binary.resolve()),
                 "--runs", str(min(args.benchmark_runs, 100)), "--report", str(rss_report_path.resolve())],
                timeout=300)
            steps.append(rss_step)
        else:
            steps.append({"name": "Linux peak-RSS diagnostic", "status": "NOT_APPLICABLE",
                          "detail": "Linux GNU-time RSS collection is OS-specific; other OS jobs retain their own build/runtime evidence."})
    else:
        steps.append({"name": "Black-box CLI acceptance tests + startup/operation benchmark", "status": "BLOCKED",
                      "detail": f"release binary was not produced at {binary}"})
        steps.append({"name": "Extended offline acceptance tests (memory, decisions, freeze, fail-closed sandbox)",
                      "status": "BLOCKED", "detail": f"release binary was not produced at {binary}"})
        steps.append({"name": "Linux peak-RSS diagnostic", "status": "BLOCKED",
                      "detail": f"release binary was not produced at {binary}"})

    failed = evaluate_release_gates(
        steps,
        require_cargo_aware_sbom=cargo_aware_sbom_required,
        require_linux_rss=sys.platform.startswith("linux"),
    )
    steps.append({"name": "Required release-gate evaluation", "status": "FAIL" if failed else "PASS",
                  "detail": json.dumps(failed, sort_keys=True) if failed else "all required CI gates recorded and passed"})
    write_reports(args.report_dir, args.toolchain, steps, runtime_report_path if runtime_report_path.exists() else None,
                  lockfile_was_present=lockfile_was_present)
    for item in failed:
        print(f"RELEASE GATE {item['status']}: {item['name']}: {item['detail']}", file=sys.stderr, flush=True)
    return 1 if failed else 0


def write_reports(report_dir: Path, toolchain: str, steps: list[dict[str, Any]], runtime_report: Path | None,
                  *, lockfile_was_present: bool | None) -> None:
    lock_path = Path("Cargo.lock")
    lockfile_info = None
    if lock_path.is_file():
        lockfile_info = {
            "present_in_checkout_before_ci": lockfile_was_present,
            "sha256": hashlib.sha256(lock_path.read_bytes()).hexdigest(),
            "artifact_copy": str(report_dir / "Cargo.lock.artifact") if (report_dir / "Cargo.lock.artifact").is_file() else None,
            "review_required": False if bool(lockfile_was_present) else True,
        }
    report = {
        "product": "Do Code 0.7.0-rs.1 (docode, Rust implementation)",
        "measured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "environment": {"os": platform.platform(), "system": platform.system(), "release": platform.release(),
                        "machine": platform.machine(), "python_version_for_test_harness": platform.python_version(),
                        "rust_toolchain_requested": toolchain},
        "setup_and_build_steps": steps,
        "lockfile": lockfile_info,
        "runtime_report_file": str(runtime_report) if runtime_report else None,
        "limitations": [
            "Toolchain install elapsed can be a cached/preinstalled result and is not necessarily first-time installation.",
            "CI timings reflect hosted-runner hardware/network and do not predict the user's personal computer exactly.",
            "Role workflows and approved patch proposal/application are implemented in source, but command/test execution requires a production-reviewed OS sandbox which is not implemented.",
            "Unix and Windows opened-handle hard-link checks are present in source but remain uncompiled/unverified; Windows private .forge ACL enforcement is incomplete.",
            "CI requires the reviewed Cargo.lock to be present in the source revision and uploads an exact copy as Cargo.lock.artifact; it never resolves a changing dependency graph automatically.",
            "The preliminary offline SBOM does not assert complete license data and does not perform a vulnerability audit; the separate RustSec workflow must actually run and findings must be reviewed.",
            "A passed CI job only confirms the specific code revision and toolchain actually tested by that workflow run.",
        ],
    }
    (report_dir / "ci-pipeline.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    summary = [
        "# Forge CLI Rust implementation CI summary",
        "",
        f"- OS: {report['environment']['os']}",
        f"- Requested Rust toolchain: {toolchain}",
        "",
        "| Step | Status | Elapsed seconds |",
        "|---|---:|---:|",
    ]
    for step in steps:
        summary.append(f"| {step.get('name', 'unknown')} | {step.get('status', 'UNKNOWN')} | {step.get('elapsed_seconds', 'n/a')} |")
    summary += ["", "**Interpret timing carefully:** toolchain setup may be cached; CLI startup is repeated-process latency with warm filesystem cache, not a cold boot. Proposal workflows can ask roles and create patches, but no build/test/browser command is executed because a production OS sandbox is not implemented.", ""]
    (report_dir / "ci-summary.md").write_text("\n".join(summary), encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
