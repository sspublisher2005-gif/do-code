# Do Code — Windows sandbox adversarial acceptance runner.
#
# SAFETY CONTRACT: run ONLY on a disposable Windows VM/runner. Never run the
# --execute-matrix mode on an everyday machine: these probes deliberately
# attempt credential reads, network egress, process-spawn containment, and
# memory exhaustion. Every case must prove BOTH (a) the attempt was denied and
# (b) protected host state is unchanged.
#
# Usage:
#   # laptop-safe (default): runs only the universal EXEC-01 refusal check and
#   # reports the matrix BLOCKED for every other case.
#   python scripts/sandbox_adversarial_windows.py --binary target\release\docode.exe --report sandbox-adversarial.json
#
#   # disposable-runner mode: executes every scripted case INSIDE the sandbox
#   # through `docode sandbox exec --approve` (AppContainer + Job Object).
#   # Probes are native code in the docode binary — no shell is available
#   # inside the AppContainer at all.
#   python scripts/sandbox_adversarial_windows.py --binary target\release\docode.exe --report sandbox-adversarial.json --execute-matrix
#
# Probe contract (src/probe.rs): exit 0 = containment HELD, exit 20 = containment
# BREACHED, exit 21 = probe could not run (harness failure). Exit 20 anywhere
# fails the case.
#
# The release gate fails on status BLOCKED or FAIL. WINDOWS_SUBSET_PASS means
# every Windows-applicable scripted case passed on a disposable runner; the
# remaining matrix cases (Linux/macOS-specific) still need their own runners
# before any overall qualification claim.

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

USERPROFILE = os.environ.get("USERPROFILE", r"C:\Users\runneradmin")

CASES = [
    {
        "id": "FS-01",
        "title": "host credential file read denied",
        "probe": "fs-01",
        "extra_args": ["__DOCO_DECOY__"],
        "expected_exit": 0,
        "must_contain": ["FS01-DONE"],
        "must_not_contain": ["FS01:LEAK", "DECOY-SECRET-CONTENT"],
        "decoy": True,
    },
    {
        "id": "FS-02",
        "title": "write outside workspace denied",
        "probe": "fs-02",
        "extra_args": [os.path.join(USERPROFILE, "docode-escape.txt")],
        "expected_exit": 0,
        "must_contain": ["FS02-BLOCKED"],
        "host_paths_absent": [os.path.join(USERPROFILE, "docode-escape.txt")],
    },
    {
        "id": "NET-01",
        "title": "DNS + outbound TCP denied (AppContainer zero capabilities)",
        "probe": "net-01",
        "expected_exit": 0,
        "must_contain": ["NET01:DENIED"],
        "must_not_contain": ["NET01:ESCAPED"],
    },
    {
        "id": "NET-02",
        "title": "loopback connect denied (host listener is deliberately open)",
        "probe": "net-02",
        "extra_args": ["__LOOPBACK_PORT__"],
        "expected_exit": 0,
        "must_contain": ["DENIED"],
    },
    {
        "id": "ENV-01",
        "title": "environment contains only allowlisted variables",
        "probe": "env-01",
        "expected_exit": 0,
        "must_contain": ["SystemRoot"],
        "must_not_contain": [
            "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "AWS_SECRET", "GITHUB_TOKEN",
            "API_KEY", "SECRET_ACCESS_KEY", "AUTHORIZATION",
        ],
    },
    {
        "id": "PROC-01",
        "title": "nested process tree contained by Job Object",
        "probe": "proc-01",
        "expected_exit": 0,
        "must_contain": ["PID:"],
        "child_pid_gone": True,
    },
    {
        "id": "RES-01",
        "title": "memory limit enforced (runaway allocation killed by Job Object)",
        "probe": "res-01",
        # The job's 512 MiB process-memory limit makes the runaway allocation
        # fail; the abort code is an NTSTATUS (0xC0000409), not 1.
        "expected_nonzero": True,
    },
    {
        "id": "CMD-01",
        "title": "argument injection arrives unchanged",
        "probe": "cmd-01",
        "extra_args": ["ARGMARK-4471"],
        "expected_exit": 0,
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
        "title": "docode completion refused (no executed-test evidence)",
        "host_command": "PROOF-01",
        "must_fail": True,
        "must_not_contain": ["MISSION COMPLETE"],
    },
]


def protected_state_snapshot(paths_present, paths_absent):
    return {p: os.path.exists(p) for p in paths_present + paths_absent}


def protected_state_unchanged(before_present, before_absent):
    for p in before_present:
        if not os.path.exists(p):
            return False, f"protected path disappeared: {p}"
    for p in before_absent:
        if os.path.exists(p):
            return False, f"protected path appeared: {p}"
    return True, "unchanged"


def run_sandboxed(binary, case, timeout_s):
    """Run one case probe inside the sandbox via `docode sandbox exec`."""
    decoy = None
    if case.get("decoy"):
        decoy = os.path.join(os.environ.get("TEMP", r"C:\Temp"), "docode-decoy.id_rsa")
        with open(decoy, "w", encoding="utf-8") as fh:
            fh.write(
                "-----BEGIN OPENSSH PRIVATE KEY-----\n"
                "DECOY-SECRET-CONTENT\n"
                "-----END OPENSSH PRIVATE KEY-----\n"
            )
    args = [binary, "sandbox", "exec", "--approve", "--timeout-ms", str(timeout_s * 1000),
            "--", binary, "sandbox", "probe", case["probe"]]
    extra = list(case.get("extra_args", []))
    extra = [
        (decoy if a == "__DOCO_DECOY__" else a)
        for a in extra
    ]
    args.extend(extra)
    env = dict(os.environ)
    started = time.time()
    try:
        proc = subprocess.run(
            args,
            capture_output=True,
            text=True, encoding="utf-8", errors="replace",
            timeout=timeout_s + 30,
            env=env,
        )
        timed_out = False
    except subprocess.TimeoutExpired:
        proc = None
        timed_out = True
    elapsed = time.time() - started
    stdout = (proc.stdout if proc else "") or ""
    stderr = (proc.stderr if proc else "") or ""
    if case.get("decoy") and decoy:
        try:
            os.remove(decoy)
        except OSError:
            pass
    return {
        "exit_code": (proc.returncode if proc else -1),
        "stdout": stdout[-8000:],
        "stderr": stderr[-2000:],
        "timed_out": timed_out,
        "elapsed_s": round(elapsed, 2),
    }


def verdict_for(case, observed):
    problems = []
    exp = case.get("expected_exit")
    if exp is not None and observed["exit_code"] != exp:
        problems.append(f"probe exit {observed['exit_code']}, expected {exp}")
    if case.get("expected_nonzero") and observed["exit_code"] == 0 and not observed.get("timed_out"):
        problems.append("runaway probe exited cleanly; the resource limit did not contain it")
    if case.get("expected_nonzero") and observed["exit_code"] == 20:
        problems.append("probe reported a containment breach")
    if observed["exit_code"] == 21:
        problems.append("probe could not run (exit 21 is a harness failure, not a verdict)")
    for needle in case.get("must_not_contain", []):
        if needle in observed["stdout"]:
            problems.append(f"forbidden output present: {needle!r}")
    for needle in case.get("must_contain", []):
        if needle not in observed["stdout"]:
            problems.append(f"required output missing: {needle!r}")
    if case.get("host_paths_absent"):
        ok, why = protected_state_unchanged([], case["host_paths_absent"])
        if not ok:
            problems.append(f"protected state changed: {why}")
    return problems


def check_child_pid_gone(binary, observed_stdout):
    """PROC-01 host-side check: the sandboxed grandchild PID must be gone."""
    for token in observed_stdout.split():
        if token.startswith("PID:"):
            pid = token[4:].strip()
            if not pid.isdigit():
                return False, f"malformed pid token {token!r}"
            check = subprocess.run(
                ["tasklist", "/FI", f"PID eq {pid}"],
                capture_output=True,
                text=True, encoding="utf-8", errors="replace",
            )
            out = (check.stdout or "").lower()
            if "info: no tasks" in out or pid not in out:
                return True, f"pid {pid} gone after sandbox exit"
            return False, f"pid {pid} still alive after sandbox exit"
    return False, "no PID token captured from probe"


def run_host_cases(binary, report, all_passed):
    """WIN-01 and PROOF-01: guarantees enforced by the CLI outside the sandbox."""
    import tempfile

    work = tempfile.mkdtemp(prefix="docode-matrix-")
    init = subprocess.run(
        [binary, "--root", work, "init"], capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    owner = subprocess.run(
        ["powershell", "-NoProfile", "-Command",
         "(Get-Acl '" + os.path.join(work, ".docode") + "').Owner"],
        capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    icacls = subprocess.run(
        ["icacls", os.path.join(work, ".docode")], capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    dacl_text = (
        "OWNER=" + (owner.stdout or owner.stderr or "").strip()
        + "\n" + (icacls.stdout or "") + (icacls.stderr or "")
    )
    problems = []
    if init.returncode != 0:
        problems.append(f"docode init failed: {(init.stderr or init.stdout)[:200]}")
    if icacls.returncode != 0:
        problems.append(f"icacls could not read .docode DACL: {dacl_text[:200]}")
    for needle in ["Everyone:(M)", "Everyone:(OI)(CI)(M)", "BUILTIN\\Users:(M)"]:
        if needle in dacl_text:
            problems.append(f"broad ACE present in .docode DACL: {needle}")
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
        text=True, encoding="utf-8", errors="replace",
    )
    out = (complete.stdout or "") + (complete.stderr or "")
    problems = []
    if complete.returncode == 0:
        problems.append("complete succeeded without sandbox-verified evidence")
    if "MISSION COMPLETE" in out:
        problems.append("forbidden output present: MISSION COMPLETE")
    report["executed"].append(
        {
            "id": "PROOF-01",
            "title": "docode completion refused",
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
    parser.add_argument("--case-timeout", type=int, default=150)
    args = parser.parse_args()

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

        # Patch per-case argument placeholders.
        for case in sandbox_cases:
            extra = case.get("extra_args", [])
            if any(a == "__LOOPBACK_PORT__" for a in extra):
                extra = [str(listener_port) if a == "__LOOPBACK_PORT__" else a for a in extra]
                case["extra_args"] = extra

        executed = []
        all_passed = True
        for case in sandbox_cases:
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
                if observed["exit_code"] != 0:
                    # Spawning was denied — PROC-01 is containment too.
                    print(f"{case['id']}: contained by spawn denial (exit {observed['exit_code']})", flush=True)
                else:
                    ok, why = check_child_pid_gone(args.binary, observed["stdout"])
                    if not ok:
                        problems.append(why)
            ok, why = protected_state_unchanged(present, absent)
            if not ok:
                problems.append(f"protected state changed: {why}")
            if case.get("decoy"):
                try:
                    pass  # already removed in run_sandboxed
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


def open_loopback_listener():
    import socket
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    return srv, srv.getsockname()[1]


if __name__ == "__main__":
    main()
