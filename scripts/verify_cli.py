#!/usr/bin/env python3
"""Cross-platform black-box acceptance tests and process-startup benchmarks for Do Code.

This script runs against a compiled Do Code binary. It never performs a live model call.
It uses Python only as a test harness; Python is not a Do Code runtime dependency.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import statistics
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


def percentile(values: list[float], q: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, int(round((len(ordered) - 1) * q))))
    return ordered[index]


class SkipCase(Exception):
    """A runtime capability could not be tested on this OS/permission set."""


class Verifier:
    def __init__(self, binary: Path):
        candidate = Path(binary)
        if os.name == "nt" and not candidate.exists() and not candidate.suffix:
            candidate = Path(str(candidate) + ".exe")
        self.binary = candidate.resolve()
        self.results: list[dict[str, Any]] = []
        self.failures = 0
        self.skips = 0

    def run(self, args: list[str], *, cwd: Path | None = None, timeout: float = 15.0,
            env_overrides: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        if env_overrides:
            env.update(env_overrides)
        return subprocess.run(
            [str(self.binary), *args], cwd=str(cwd) if cwd else None, env=env,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            timeout=timeout, check=False,
        )

    def check(self, name: str, fn) -> None:
        started = time.perf_counter()
        state = "PASS"
        detail = ""
        try:
            outcome = fn()
            if isinstance(outcome, str):
                detail = outcome
        except SkipCase as exc:
            state = "SKIP"
            detail = str(exc)
            self.skips += 1
        except Exception as exc:  # test runner should report every failure instead of stopping at the first
            state = "FAIL"
            detail = f"{type(exc).__name__}: {exc}"
            self.failures += 1
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        self.results.append({"name": name, "status": state, "duration_ms": round(elapsed_ms, 3), "detail": detail})
        print(f"{state:4} {name} ({elapsed_ms:.2f} ms){': ' + detail if detail else ''}")

    def skip(self, name: str, reason: str, *, known_gap: bool = False) -> None:
        del name
        prefix = "KNOWN GAP: " if known_gap else "SKIPPED: "
        raise SkipCase(prefix + reason)

    @staticmethod
    def require(condition: bool, message: str) -> None:
        if not condition:
            raise AssertionError(message)

    def verify(self, benchmark_runs: int = 30) -> dict[str, Any]:
        self.check("binary exists", lambda: self.require(self.binary.is_file(), f"not found: {self.binary}"))
        if not self.binary.is_file():
            return self.report(benchmark_runs, {})

        def version_test() -> str:
            proc = self.run(["--version"])
            self.require(proc.returncode == 0, proc.stderr)
            self.require("0.7.0-rs.1" in proc.stdout, proc.stdout)
            return proc.stdout.strip()

        self.check("version command", version_test)
        self.check("help command", lambda: self.require((lambda p: p.returncode == 0 and "Do Code" in p.stdout)(self.run(["--help"])), "--help failed or omitted product name"))

        with tempfile.TemporaryDirectory(prefix="docode-verify-") as temp_name:
            root = Path(temp_name) / "workspace"
            root.mkdir()

            def initialize() -> str:
                proc = self.run(["--root", str(root), "init"])
                self.require(proc.returncode == 0, proc.stderr)
                self.require("Network: not used" in proc.stdout, proc.stdout)
                self.require((root / "docode.config.toml").is_file(), "config file not created")
                self.require((root / "DOCODE.md").is_file(), "DOCODE.md not created")
                self.require((root / ".docode").is_dir(), ".docode directory not created")
                if os.name != "nt":
                    docode_mode = stat.S_IMODE((root / ".docode").stat().st_mode)
                    audit_mode = stat.S_IMODE((root / ".docode" / "audit.jsonl").stat().st_mode)
                    self.require(docode_mode == 0o700, f".docode mode is {oct(docode_mode)}, expected 0o700")
                    self.require(audit_mode == 0o600, f"audit mode is {oct(audit_mode)}, expected 0o600")
                return "created; Unix private modes checked where applicable; no intentional network call"

            self.check("offline init creates workspace", initialize)

            def idempotent_init() -> None:
                before = {name: (root / name).read_bytes() for name in ("docode.config.toml", "DOCODE.md")}
                proc = self.run(["init", "--root", str(root)])
                self.require(proc.returncode == 0, proc.stderr)
                self.require(before["docode.config.toml"] == (root / "docode.config.toml").read_bytes(), "config overwritten on second init")
                self.require(before["DOCODE.md"] == (root / "DOCODE.md").read_bytes(), "project memory overwritten on second init")

            self.check("init idempotency / no overwrite", idempotent_init)

            def preserve_custom_files() -> None:
                other = Path(temp_name) / "custom"
                other.mkdir()
                config = other / "docode.config.toml"
                memory = other / "DOCODE.md"
                config.write_text("custom config marker\n", encoding="utf-8")
                memory.write_text("custom memory marker\n", encoding="utf-8")
                proc = self.run(["--root", str(other), "init"])
                self.require(proc.returncode == 0, proc.stderr)
                self.require(config.read_text(encoding="utf-8") == "custom config marker\n", "custom config overwritten")
                self.require(memory.read_text(encoding="utf-8") == "custom memory marker\n", "custom project memory overwritten")

            self.check("init preserves pre-existing custom files", preserve_custom_files)

            def doctor_test() -> None:
                proc = self.run(["--root", str(root), "doctor"])
                self.require(proc.returncode == 0, proc.stderr)
                self.require("Sandbox: not implemented" in proc.stdout, "doctor does not disclose sandbox limitation")
                self.require("no predicted cost" in proc.stdout, "doctor does not disclose the cost policy")
                if os.name == "nt":
                    self.require("Windows .docode ACL hardening" in proc.stdout, "Windows security warning missing")

            self.check("doctor reports actual limits and platform warning", doctor_test)

            def doctor_does_not_echo_secret_environment_value() -> None:
                marker = "private-env-marker-987654321"
                proc = self.run(["--root", str(root), "doctor"], env_overrides={"OPENAI_API_KEY": marker})
                self.require(proc.returncode == 0, proc.stderr)
                self.require(marker not in proc.stdout + proc.stderr, "doctor exposed API key value")
                self.require("value hidden" in proc.stdout, "doctor did not state that key value is hidden")

            self.check("doctor never prints credential environment values", doctor_does_not_echo_secret_environment_value)

            def config_show_test() -> None:
                proc = self.run(["--root", str(root), "config", "show"])
                self.require(proc.returncode == 0, proc.stderr)
                parsed = json.loads(proc.stdout)
                self.require(parsed.get("exists") is True, "config existence flag incorrect")
                self.require(parsed["config"]["privacy"]["telemetry"] is False, "telemetry default must be false")

            self.check("config show emits valid JSON and safe defaults", config_show_test)

            def unicode_and_space_paths_are_supported() -> None:
                other = Path(temp_name) / "Do Code workspace – हिन्दी"
                other.mkdir()
                init = self.run(["--root", str(other), "init"])
                self.require(init.returncode == 0, init.stderr)
                show = self.run(["--root", str(other), "config", "show"])
                self.require(show.returncode == 0, show.stderr)
                parsed = json.loads(show.stdout)
                self.require("Do Code workspace – हिन्दी" in parsed["path"], parsed["path"])

            self.check("workspace paths with spaces and Unicode", unicode_and_space_paths_are_supported)

            def offline_model_test() -> None:
                proc = self.run(["--root", str(root), "model", "test"])
                self.require(proc.returncode == 0, proc.stderr)
                self.require("no model request was sent" in proc.stdout, proc.stdout)

            self.check("model test is offline unless explicitly approved", offline_model_test)

            def consent_test() -> None:
                proc = self.run(["--root", str(root), "model", "test", "--live"])
                self.require(proc.returncode != 0, "live request accepted without explicit context-sharing approval")
                self.require("--approve-context-sharing" in proc.stderr, proc.stderr)

            self.check("live diagnostic requires separate consent flag", consent_test)

            def cost_policy_test() -> None:
                help_text = self.run(["--help"]).stdout
                self.require("--max-cost-usd" not in help_text, "cost-stop flag was exposed")
                self.require("--estimated-cost" not in help_text, "cost-estimation flag was exposed")
                config_text = (root / "docode.config.toml").read_text(encoding="utf-8")
                self.require("max_cost" not in config_text and "estimated_cost" not in config_text, "cost budget was added to configuration")

            self.check("no predicted-cost or cost-budget stop flags", cost_policy_test)

            def file_tools_test() -> None:
                (root / "src").mkdir()
                (root / "src" / "example.rs").write_text(
                    'fn main() {}\napi_key="display-secret-value-123"\n// find-marker\n', encoding="utf-8"
                )
                (root / ".env").write_text("API_KEY=must-not-be-read\n", encoding="utf-8")
                (root / "private.pem").write_text("PRIVATE KEY must not be exposed\n", encoding="utf-8")
                (root / "secrets").mkdir()
                (root / "secrets" / "database.txt").write_text("database-secret-must-not-be-listed\n", encoding="utf-8")
                (root / ".ssh").mkdir()
                (root / ".ssh" / "id_ed25519").write_text("private-key-must-not-be-listed\n", encoding="utf-8")
                original_source = (root / "src" / "example.rs").read_bytes()
                listing = self.run(["--root", str(root), "files", "list"])
                self.require(listing.returncode == 0, listing.stderr)
                self.require(".env" not in listing.stdout, listing.stdout)
                self.require("private.pem" not in listing.stdout, listing.stdout)
                self.require("secrets" not in listing.stdout and ".ssh" not in listing.stdout, listing.stdout)
                reading = self.run(["--root", str(root), "files", "read", "src/example.rs"])
                self.require(reading.returncode == 0, reading.stderr)
                self.require("fn main" in reading.stdout, reading.stdout)
                self.require("display-secret-value-123" not in reading.stdout, "read leaked a credential-like value")
                self.require("[REDACTED]" in reading.stdout, "read did not redact credential-like value")
                search = self.run(["--root", str(root), "files", "search", "api_key", "src"])
                self.require(search.returncode == 0, search.stderr)
                self.require("display-secret-value-123" not in search.stdout, "search leaked a credential-like value")
                self.require("[REDACTED]" in search.stdout, "search did not redact credential-like value")
                denied = self.run(["--root", str(root), "files", "read", ".env"])
                self.require(denied.returncode != 0, "read allowed .env")
                self.require("must-not-be-read" not in denied.stdout + denied.stderr, "blocked file contents appeared in output")
                traversal = self.run(["--root", str(root), "files", "read", "../outside.txt"])
                self.require(traversal.returncode != 0, "path traversal succeeded")
                self.require((root / "src" / "example.rs").read_bytes() == original_source, "read/search changed source file bytes")
                binary = root / "image.png"
                binary.write_bytes(b"\x89PNG\r\n\x1a\n")
                denied_binary = self.run(["--root", str(root), "files", "read", "image.png"])
                self.require(denied_binary.returncode != 0, "binary file was rendered as text")
                (root / "invalid-utf8.txt").write_bytes(b"\xff\xfe")
                (root / "nul.txt").write_bytes(b"before\x00after")
                for invalid_name in ("invalid-utf8.txt", "nul.txt"):
                    invalid = self.run(["--root", str(root), "files", "read", invalid_name])
                    self.require(invalid.returncode != 0, f"{invalid_name} should not render as text")
                no_recursive = self.run(["--root", str(root), "files", "list", "--no-recursive"])
                self.require(no_recursive.returncode == 0, no_recursive.stderr)
                self.require("src/example.rs" not in no_recursive.stdout, "--no-recursive included nested file")
                for hostile_path in ("/etc/passwd", "C:\\Windows\\win.ini"):
                    hostile = self.run(["--root", str(root), "files", "read", hostile_path])
                    self.require(hostile.returncode != 0, f"absolute path was accepted: {hostile_path}")

            self.check("file tools: no writes, private paths, redaction, traversal, encoding/binary/limit guards", file_tools_test)

            def symlink_test() -> None:
                outside = Path(temp_name) / "outside.txt"
                outside.write_text("outside-secret", encoding="utf-8")
                link = root / "escape.txt"
                try:
                    os.symlink(outside, link)
                except (OSError, NotImplementedError) as exc:
                    self.skip("symlink escape rejection", f"OS denied symlink creation: {exc}")
                    return
                proc = self.run(["--root", str(root), "files", "read", "escape.txt"])
                self.require(proc.returncode != 0, "symlink pointing outside workspace was followed")
                self.require("outside-secret" not in proc.stdout + proc.stderr, "symlink target content leaked")

            self.check("symlink escape rejection where OS permissions allow", symlink_test)

            def hardlink_test() -> None:
                source = root / "hardlink-source.txt"
                alias = root / "hardlink-alias.txt"
                source.write_text("hardlink-secret", encoding="utf-8")
                os.link(source, alias)
                proc = self.run(["--root", str(root), "files", "read", "hardlink-alias.txt"])
                self.require(proc.returncode != 0, "hard-linked file was read; handle-based hard-link rejection is required on every supported OS")
                self.require("hardlink-secret" not in proc.stdout + proc.stderr, "hard-linked file content leaked")

            self.check("hard-link rejection through opened file handles", hardlink_test)

            def audit_test() -> None:
                proc = self.run(["--root", str(root), "audit", "--tail", "50"])
                self.require(proc.returncode == 0, proc.stderr)
                events = json.loads(proc.stdout)
                self.require(isinstance(events, list), "audit output is not a JSON list")
                sequences = [event["sequence"] for event in events]
                self.require(sequences == sorted(sequences), f"audit sequence ordering broken: {sequences}")
                self.require(all("details" in event for event in events), "audit event missing details object")
                self.require(not any("display-secret-value-123" in json.dumps(event) for event in events), "audit unexpectedly contains file contents")

            self.check("audit JSONL is readable, ordered, and contains no file contents", audit_test)

            def invalid_config_test() -> None:
                config_path = root / "docode.config.toml"
                good = config_path.read_text(encoding="utf-8")
                try:
                    config_path.write_text(good + "\nunknown_injected_key = true\n", encoding="utf-8")
                    proc = self.run(["--root", str(root), "config", "show"])
                    self.require(proc.returncode != 0, "unknown TOML key was accepted")
                finally:
                    config_path.write_text(good, encoding="utf-8")

            self.check("unknown config keys are rejected", invalid_config_test)

            def insecure_url_test() -> None:
                config_path = root / "docode.config.toml"
                good = config_path.read_text(encoding="utf-8")
                try:
                    changed, count = re.subn(
                        r'(?m)^base_url\s*=\s*"[^"]+"',
                        'base_url = "http://example.invalid/v1"', good, count=1
                    )
                    self.require(count == 1, "could not locate base_url in generated config")
                    config_path.write_text(changed, encoding="utf-8")
                    proc = self.run(["--root", str(root), "model", "test"])
                    self.require(proc.returncode != 0, "remote cleartext HTTP endpoint was accepted")
                    self.require("HTTPS" in proc.stderr or "HTTPS" in proc.stdout, proc.stderr + proc.stdout)
                finally:
                    config_path.write_text(good, encoding="utf-8")

            self.check("remote cleartext HTTP provider endpoint is blocked without network", insecure_url_test)

            def invalid_tail_test() -> None:
                for value in ("0", "1001"):
                    proc = self.run(["--root", str(root), "audit", "--tail", value])
                    self.require(proc.returncode != 0, f"invalid audit tail limit {value} accepted")

            self.check("audit tail bounds reject below-minimum and above-maximum values", invalid_tail_test)

            # Repetitions create new CLI processes; OS file cache becomes warm after the first few.
            def benchmark_case(args: list[str], runs: int) -> dict[str, float]:
                for _ in range(3):
                    result = self.run(args, timeout=15.0)
                    self.require(result.returncode == 0, result.stderr)
                samples: list[float] = []
                for _ in range(runs):
                    started = time.perf_counter()
                    result = self.run(args, timeout=15.0)
                    elapsed_ms = (time.perf_counter() - started) * 1000.0
                    samples.append(elapsed_ms)
                    self.require(result.returncode == 0, result.stderr)
                return {
                    "runs": len(samples),
                    "median_ms": round(statistics.median(samples), 3),
                    "p95_ms": round(percentile(samples, 0.95), 3),
                    "max_ms": round(max(samples), 3),
                }

            timings: dict[str, Any] = {}
            self.check("benchmark executable process-startup latency", lambda: timings.update({"version_process_startup": benchmark_case(["--version"], benchmark_runs)}))
            self.check("benchmark doctor latency", lambda: timings.update({"doctor": benchmark_case(["--root", str(root), "doctor"], max(5, benchmark_runs // 3))}))
            self.check("benchmark workspace-list latency", lambda: timings.update({"files_list": benchmark_case(["--root", str(root), "files", "list"], max(5, benchmark_runs // 3))}))
            self.check("benchmark config-read latency", lambda: timings.update({"config_show": benchmark_case(["--root", str(root), "config", "show"], max(5, benchmark_runs // 3))}))
            result = self.report(benchmark_runs, timings)
            return result

    def report(self, benchmark_runs: int, timings: dict[str, Any]) -> dict[str, Any]:
        version_proc = self.run(["--version"]) if self.binary.is_file() else None
        report: dict[str, Any] = {
            "product": "Do Code 0.7.0-rs.1 (Rust implementation WIP)",
            "measured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "environment": {
                "os": platform.platform(),
                "system": platform.system(),
                "release": platform.release(),
                "machine": platform.machine(),
                "python_version_for_test_harness": platform.python_version(),
                "binary": str(self.binary),
                "binary_bytes": self.binary.stat().st_size if self.binary.is_file() else None,
                "version_output": version_proc.stdout.strip() if version_proc else None,
            },
            "test_summary": {
                "passed": sum(item["status"] == "PASS" for item in self.results),
                "failed": self.failures,
                "skipped_or_known_gap": self.skips,
                "tests": self.results,
            },
            "benchmark_method": {
                "startup": "new subprocess per sample; filesystem caches warm after warm-up; not a cold-cache benchmark",
                "startup_runs": benchmark_runs,
                "network_calls": 0,
                "live_model_calls": 0,
                "timings": timings,
            },
            "not_measured_or_not_covered": [
                "Rust toolchain installation duration is not measured by this runtime harness",
                "model role proposal workflows are available, but command/test execution is blocked because no OS sandbox is implemented",
                "Windows opened-handle hard-link checks are present but unverified; private ACL enforcement remains incomplete",
                "macOS and Windows results exist only when this script actually runs on those OS runners",
                "startup measurements are CI-runner timings, not a guarantee for the user's device",
            ],
        }
        return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, help="path to the compiled docode executable")
    parser.add_argument("--benchmark-runs", type=int, default=30)
    parser.add_argument("--report", type=Path, default=Path("test-results/docode-runtime.json"))
    args = parser.parse_args()
    if not 5 <= args.benchmark_runs <= 500:
        parser.error("--benchmark-runs must be between 5 and 500")
    verifier = Verifier(Path(args.binary))
    report = verifier.verify(args.benchmark_runs)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"\nReport: {args.report}")
    print(f"Summary: {report['test_summary']['passed']} passed, {report['test_summary']['failed']} failed, {report['test_summary']['skipped_or_known_gap']} skipped/known gap")
    # This is a release gate, not a best-effort demo. Every recorded skip must be
    # reviewed explicitly; an untested path cannot count as a clean cross-OS pass.
    return 1 if verifier.failures or verifier.skips else 0


if __name__ == "__main__":
    raise SystemExit(main())
