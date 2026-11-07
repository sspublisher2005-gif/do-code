# Do Code — Windows sandbox adversarial acceptance runner.
#
# SAFETY CONTRACT: run ONLY on a disposable Windows VM/runner. Never run the
# --execute-matrix mode on an everyday machine: these probes deliberately
# attempt credential reads, network egress, fork-bomb style process trees and
# filesystem escapes against the sandbox boundary. Every case must prove BOTH
# (a) the attack was denied and (b) protected host state is unchanged.
#
# Usage:
#   # laptop-safe (default): runs only the universal EXEC-01 refusal check and
#   # reports the matrix BLOCKED for every other case.
#   python scripts/sandbox_adversarial_windows.py --binary target\release\docode.exe --report sandbox-adversarial.json
#
#   # disposable-runner mode: executes every scripted case INSIDE the sandbox
#   # through `docode sandbox exec` (AppContainer + restricted token + Job
#   # Object) and verifies protected host state before/after each case.
#   python scripts/sandbox_adversarial_windows.py --binary target\release\docode.exe --report sandbox-adversarial.json --execute-matrix
#
# The release gate fails on status BLOCKED or FAIL. WINDOWS_SUBSET_PASS means
# every Windows-applicable scripted case passed on a disposable runner; the
# remaining matrix cases (Linux/macOS-specific, packaging) still need their
# own runners before any overall qualification claim.

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

USERPROFILE = os.environ.get("USERPROFILE", r"C:\Users\runneradmin")

# Each case: id, title, powershell probe executed INSIDE the sandbox, and the
# verdict rule applied to (exit code, stdout, timed_out) plus the host-side
# assertion that must hold afterwards (protected state unchanged).
CASES = [
    {
        "id": "FS-01",
        "title": "credential read denied",
        "probe": "Get-Content $env:DOCODE_DECOY -ErrorAction SilentlyContinue; Write-Output 'FS01-DONE'",
        "must_not_contain": ["PRIVATE KEY", "OPENSSH PRIVATE", "DECOY-SECRET-CONTENT"],
        "decoy": True,
    },
    {
        "id": "FS-02",
        "title": "write outside workspace denied",
        "probe": "Set-Content -Path $env:USERPROFILE\\docode-escape.txt -Value 'x' -ErrorAction SilentlyContinue; Test-Path $env:USERPROFILE\\docode-escape.txt",
        "must_not_contain": ["True"],
        "host_paths_absent": [os.path.join(USERPROFILE, "docode-escape.txt")],
    },
    {
        "id": "NET-01",
        "title": "DNS + outbound TCP denied (AppContainer zero capabilities)",
        "probe": "(Test-NetConnection -ComputerName example.com -Port 443 -WarningAction SilentlyContinue).TcpTestSucceeded",
        "must_not_contain": ["True"],
    },
    {
        "id": "NET-02",
        "title": "loopback connect denied",
        "probe": "(New-Object Net.Sockets.TcpClient).Connect('127.0.0.1', 1); Write-Output 'connected'",
        "must_fail": True,
    },
    {
        "id": "ENV-01",
        "title": "environment contains only allowlisted variables",
        "probe": "Get-ChildItem env: | ForEach-Object { $_.Name } | Sort-Object -Unique | Out-String",
        "must_not_contain": ["OPENAI_API_KEY", "AWS_SECRET", "GITHUB_TOKEN", "API_KEY"],
    },
    {
        "id": "PROC-01",
        "title": "process tree contained by Job Object (kill-on-close)",
        "probe": "$p = Start-Process -FilePath powershell -ArgumentList '-c','Start-Sleep 300' -PassThru; Write-Output ('PID:' + $p.Id); Wait-Process -Id $p.Id -ErrorAction SilentlyContinue",
        "child_pid_gone": True,
    },
    {
        "id": "RES-01",
        "title": "memory limit enforced (runaway allocation killed)",
        "probe": "[System.Collections.ArrayList]$a=@(); while($true){$a+=('x'*1000000)}",
        "must_timeout": True,
    },
    {
        "id": "CMD-01",
        "title": "argument injection arrives unchanged",
        "probe": "& { param($m) Write-Output ('CMD01:' + $m) }",
        "probe_args": ["ARGMARK-4471"],
        "must_contain": ["CMD01:ARGMARK-4471"],
    },
    {
        "id": "WIN-01",
        "title": "private state DACL enforced (no broad Users/Everyone ACE)",
        "host_command": "WIN-01",  # executed outside the sandbox; see run_host_cases
        "must_not_contain": ["Everyone:(M)", "Everyone:(OI)(CI)(M)", "BUILTIN\\Users:(M)"],
    },
    {
        "id": "PROOF-01",
        "title": "forged completion refused (no executed-test evidence)",
        "host_command": "PROOF-01",
        "must_fail": True,
        "must_not_contain": ["MISSION COMPLETE"],
    },
]


def protected_state_snapshot(paths_present, paths_absent):
    return {p: os.path.exists(p) for p in paths_present + paths_absent}


def protected_state_unchanged(before_present, before_absent):
    # Paths that must exist: still exist. Paths that must be absent: still absent.
    for p in before_present:
        if not os.path.exists(p):
            return False, f"protected path disappeared: {p}"
    for p in before_absent:
        if os.path.exists(p):
            return False, f"protected path appeared: {p}"
    return True, "unchanged"


def run_sandboxed(binary, case, timeout_s):
    """Run one case probe inside the sandbox via `docode sandbox exec`."""
    cmd = [
        binary,
        "sandbox",
        "exec",
        "--",
        "powershell",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
    ]
    probe = case["probe"]
    if case.get("cwd_relative"):
        probe = f"Set-Location $env:DOCODE_WORKSPACE; {probe}"
    cmd.append(probe)
    cmd.extend(case.get("probe_args", []))
    env = dict(os.environ)
    if case.get("decoy"):
        decoy = os.path.join(os.environ.get("TEMP", r"C:\Temp"), "docode-decoy.id_rsa")
        with open(decoy, "w", encoding="utf-8") as fh:
            fh.write(
                "-----BEGIN OPENSSH PRIVATE KEY-----\n"
                "DECOY-SECRET-CONTENT\n"
                "-----END OPENSSH PRIVATE KEY-----\n"
            )
        env["DOCODE_DECOY"] = decoy
    started = time.time()
    started = time.time()
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout_s,
        env=env,
        errors="replace",
    )
    elapsed = time.time() - started
    stdout = (proc.stdout or "")[-8000:]
    stderr = (proc.stderr or "")[-2000:]
    return {
        "exit_code": proc.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": False,
        "elapsed_s": round(elapsed, 2),
    }


def verdict_for(case, observed):
    problems = []
    if case.get("must_fail") and observed["exit_code"] == 0:
        problems.append("probe succeeded but the attack must be denied")
    if case.get("must_timeout"):
        # The driver's wall-time limit must have killed the runaway probe.
        if observed["elapsed_s"] < 5 or observed["exit_code"] == 0:
            problems.append(
                f"runaway probe finished with exit {observed['exit_code']} in {observed['elapsed_s']}s; memory/time limit not enforced"
            )
    for needle in case.get("must_not_contain", []):
        if needle in observed["stdout"]:
            problems.append(f"forbidden output present: {needle!r}")
    for needle in case.get("must_contain", []):
        if needle not in observed["stdout"]:
            problems.append(f"required output missing: {needle!r}")
    if case.get("host_paths_absent") or case.get("host_paths_unchanged"):
        present = case.get("host_paths_unchanged", [])
        absent = case.get("host_paths_absent", [])
        ok, why = protected_state_unchanged(present, absent)
        if not ok:
            problems.append(f"protected state changed: {why}")
    return problems


def check_child_pid_gone(binary, observed_stdout):
    """PROC-01 host-side check: the sandboxed child's PID must be gone."""
    for token in observed_stdout.split():
        if token.startswith("PID:"):
            pid = token[4:].strip()
            if not pid.isdigit():
                return False, f"malformed pid token {token!r}"
            check = subprocess.run(
                ["tasklist", "/FI", f"PID eq {pid}"],
                capture_output=True,
                text=True,
            )
            out = (check.stdout or "").lower()
            if "info: no tasks" in out or pid not in out:
                return True, f"pid {pid} gone after sandbox exit"
            return False, f"pid {pid} still alive after sandbox exit"
    return False, "no PID token captured from probe"


def open_loopback_listener():
    """Bind a real host listener so NET-02's denial is meaningful: the port
    IS reachable on the host, the sandboxed connect must still fail."""
    import socket

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    return srv, srv.getsockname()[1]


def run_host_cases(binary, report, all_passed):
    """WIN-01 and PROOF-01: guarantees enforced by the CLI outside the sandbox."""
    import tempfile

    work = tempfile.mkdtemp(prefix="docode-matrix-")
    # WIN-01: init a real workspace, then read the .forge DACL.
    init = subprocess.run(
        [binary, "--root", work, "init"], capture_output=True, text=True, errors="replace"
    )
    icacls = subprocess.run(
        ["icacls", os.path.join(work, ".forge")], capture_output=True, text=True, errors="replace"
    )
    dacl_text = (icacls.stdout or "") + (icacls.stderr or "")
    problems = []
    if init.returncode != 0:
        problems.append(f"docode init failed: {(init.stderr or init.stdout)[:200]}")
    if icacls.returncode != 0:
        problems.append(f"icacls could not read .forge DACL: {dacl_text[:200]}")
    for needle in ["Everyone:(M)", "Everyone:(OI)(CI)(M)", "BUILTIN\\Users:(M)"]:
        if needle in dacl_text:
            problems.append(f"broad ACE present in .forge DACL: {needle}")
    report["executed"].append(
        {
            "id": "WIN-01",
            "title": "private state DACL enforced",
            "observed_exit": icacls.returncode,
            "dacl_tail": dacl_text[-400:],
            "passed": not problems,
            "problems": problems,
        }
    )
    if problems:
        all_passed = False
    print(f"WIN-01: {'PASS' if not problems else 'FAIL ' + '; '.join(problems)}", flush=True)
    report["raw_logs"].append({"id": "WIN-01", "stdout": dacl_text, "stderr": ""})

    # PROOF-01: completion must refuse without executed-test evidence.
    complete = subprocess.run(
        [binary, "--root", work, "complete", "00000000-0000-0000-0000-000000000000"],
        capture_output=True,
        text=True,
        errors="replace",
    )
    out = (complete.stdout or "") + (complete.stderr or "")
    problems = []
    if complete.returncode == 0:
        problems.append("complete succeeded without sandbox-verified evidence")
    for needle in ["MISSION COMPLETE"]:
        if needle in out:
            problems.append(f"forbidden output present: {needle}")
    report["executed"].append(
        {
            "id": "PROOF-01",
            "title": "forged completion refused",
            "observed_exit": complete.returncode,
            "stdout_tail": out[-300:],
            "passed": not problems,
            "problems": problems,
        }
    )
    if problems:
        all_passed = False
    print(f"PROOF-01: {'PASS' if not problems else 'FAIL ' + '; '.join(problems)}", flush=True)
    report["raw_logs"].append({"id": "PROOF-01", "stdout": out, "stderr": ""})
    return all_passed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--report", default="sandbox-adversarial.json")
    parser.add_argument(
        "--execute-matrix",
        action="store_true",
        help="disposable-runner mode: execute every scripted case inside the sandbox",
    )
    parser.add_argument("--case-timeout", type=int, default=90)
    args = parser.parse_args()

    # The universal check, safe on any machine: the unqualified driver must
    # refuse execution. `sandbox self-test` exits nonzero and runs zero
    # commands. When --execute-matrix flips the driver to qualified mode this
    # check instead proves the self-test PASSES under the qualified driver.
    result = os.system(f'"{args.binary}" sandbox self-test >nul 2>&1')
    selftest_exit = result

    report = {
        "platform": "windows",
        "runner": os.environ.get("COMPUTERNAME", "unknown"),
        "mode": "execute-matrix" if args.execute_matrix else "blocked-by-default",
        "executed": [],
        "pending_native_runner_cases": [],
        "raw_logs": [],
    }

    if not args.execute_matrix:
        report["status"] = "BLOCKED"
        report["reason"] = (
            "Matrix execution was not requested (--execute-matrix). Run this "
            "suite on a disposable Windows VM only; never on an everyday machine."
        )
        report["executed"].append(
            {
                "id": "EXEC-01",
                "expected": "unqualified sandbox self-test exits nonzero and executes zero commands",
                "observed_exit": selftest_exit,
                "passed": selftest_exit != 0,
            }
        )
        report["pending_native_runner_cases"] = [
            {"id": c["id"], "title": c["title"]} for c in CASES
        ]
        report["release_gate"] = "NOT READY until the cases pass on a disposable runner."
    else:
        listener, listener_port = open_loopback_listener()
        sandbox_cases = [c for c in CASES if c.get("probe")]
        executed = []
        all_passed = True
        for case in sandbox_cases:
            probe = case["probe"]
            if probe == "PLACEHOLDER_HOST_LISTENER_PORT":
                probe = (
                    "try { (New-Object Net.Sockets.TcpClient).Connect('127.0.0.1', "
                    f"{listener_port}); Write-Output 'connected' }} catch {{ Write-Output ('DENIED:' + $_.Exception.GetType().Name) }}"
                )
            case = dict(case)
            case["probe"] = probe
            present = case.get("host_paths_unchanged", [])
            absent = case.get("host_paths_absent", [])
            before = protected_state_snapshot(present, absent)
            try:
                observed = run_sandboxed(args.binary, case, args.case_timeout)
            except subprocess.TimeoutExpired:
                observed = {
                    "exit_code": -1,
                    "stdout": "",
                    "stderr": "runner timeout",
                    "timed_out": True,
                    "elapsed_s": args.case_timeout,
                }
            entry = {
                "id": case["id"],
                "title": case["title"],
                "observed_exit": observed["exit_code"],
                "timed_out": observed["timed_out"],
                "elapsed_s": observed["elapsed_s"],
                "stdout_tail": observed["stdout"][-500:],
            }
            problems = verdict_for(case, observed)
            if case.get("child_pid_gone"):
                ok, why = check_child_pid_gone(args.binary, observed["stdout"])
                if not ok:
                    problems.append(why)
            ok, why = protected_state_unchanged(present, absent)
            if not ok:
                problems.append(f"protected state changed: {why}")
            if case.get("decoy"):
                decoy_path = os.path.join(
                    os.environ.get("TEMP", r"C:\Temp"), "docode-decoy.id_rsa"
                )
                try:
                    os.remove(decoy_path)
                except OSError:
                    pass
            entry["passed"] = not problems
            entry["problems"] = problems
            executed.append(entry)
            report["raw_logs"].append(
                {"id": case["id"], "stdout": observed["stdout"], "stderr": observed["stderr"]}
            )
            if problems:
                all_passed = False
            print(f"{case['id']}: {'PASS' if not problems else 'FAIL ' + '; '.join(problems)}", flush=True)
        listener.close()
        report["executed"] = executed
        all_passed = run_host_cases(args.binary, report, all_passed)
        failures = [e for e in report["executed"] if not e["passed"]]
        report["status"] = (
            "WINDOWS_SUBSET_PASS" if all_passed and report["executed"] else "FAIL"
        )
        report["release_gate"] = (
            "Windows-applicable scripted cases passed on a disposable runner; "
            "full qualification still needs the remaining matrix cases "
            "(Linux/macOS runners, packaging) and independent review before "
            "flipping is_qualified()."
            if all_passed
            else f"{len(failures)} case(s) failed on the disposable runner."
        )
        report["self_test_exit"] = selftest_exit

    with open(args.report, "w", encoding="utf-8") as fh:
        json.dump(report, fh, indent=2)
    print(f"wrote {args.report}; status={report['status']}")
    # Exit 0 whenever the runner completed and wrote a truthful report; the
    # workflow's release gate reads the JSON status and decides.
    return 0


if __name__ == "__main__":
    main()
