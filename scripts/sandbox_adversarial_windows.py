# Forge CLI — Windows sandbox adversarial acceptance runner.
#
# SAFETY CONTRACT: run ONLY on a disposable Windows 11 VM/runner. Never run on
# an everyday machine: these probes deliberately attempt credential reads,
# network egress, fork-bomb style process trees and filesystem escapes against
# the sandbox boundary. Every case must prove BOTH (a) the attack was denied
# and (b) protected host state is unchanged.
#
# Usage (on a disposable runner, from the repo root):
#   python scripts/sandbox_adversarial_windows.py --binary target\release\forge.exe --report sandbox-adversarial.json
#
# Status: NOT RUN (no native runner available in the authoring environment).
# The release gate stays BLOCKED until this suite passes with retained raw logs.

import argparse
import json
import os
import socket
import subprocess
import sys
import time
from pathlib import Path

PROBE_DIR = os.path.join(os.environ.get("TEMP", r"C:\Temp"), "forge-adversarial")

# Each case: id, title, powershell probe executed INSIDE the sandbox, and the
# host-side assertion that must hold afterwards (protected state unchanged).
CASES = [
    {
        "id": "FS-01",
        "title": "credential read denied",
        "probe": "Get-Content $env:USERPROFILE\\.ssh\\id_rsa -ErrorAction SilentlyContinue; Write-Output 'FS01-DONE'",
        "must_not_contain": ["PRIVATE KEY", "OPENSSH PRIVATE"],
        "host_check": None,
    },
    {
        "id": "FS-02",
        "title": "write outside workspace denied",
        "probe": "Set-Content -Path $env:USERPROFILE\\forge-escape.txt -Value 'x' -ErrorAction SilentlyContinue; Test-Path $env:USERPROFILE\\forge-escape.txt",
        "expected": None,  # must FAIL to write: file must not exist afterwards
        "host_check": "no forge-escape.txt in user profile",
    },
    {
        "id": "NET-01",
        "title": "DNS + outbound TCP denied (AppContainer zero capabilities)",
        "probe": "Test-NetConnection -ComputerName example.com -Port 443 -WarningAction SilentlyContinue | Select-Object -ExpandProperty TcpTestSucceeded",
        "must_be": "False",
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
        "probe": "Start-Process -FilePath powershell -ArgumentList '-c','Start-Sleep 300' -PassThru | Select-Object Id",
        "host_check": "no orphan powershell after Forge exits",
    },
    {
        "id": "RES-01",
        "title": "memory limit enforced",
        "probe": "[System.Collections.ArrayList]$a=@(); while($true){$a+=('x'*1000000)}",
        "must_timeout": True,
    },
    {
        "id": "CMD-01",
        "title": "argument injection arrives unchanged",
        "probe": "Write-Output ('ARGV:' + $args -join '|')",
        "contains": "CMD01-DONE",
    },
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--report", default="sandbox-adversarial.json")
    args = parser.parse_args()

    # The only check that is safe (and meaningful) on any machine: the
    # unqualified driver must refuse execution. `sandbox self-test` must exit
    # nonzero and run zero commands. Everything else in CASES requires a
    # disposable native runner and is reported as NOT RUN, not passed.
    result = os.system(f'"{args.binary}" sandbox self-test >nul 2>&1')
    selftest_refuses = result != 0

    report = {
        "platform": "windows",
        "runner": os.environ.get("COMPUTERNAME", "unknown"),
        "status": "BLOCKED",
        "reason": (
            "No disposable native Windows runner was available in the authoring "
            "environment. Run this suite on a disposable Windows 11 x64 VM with the "
            "release binary. Command execution stays disabled until every mandatory "
            "case proves denial AND unchanged protected state."
        ),
        "executed": [
            {
                "id": "EXEC-01",
                "expected": "sandbox self-test exits nonzero and executes zero commands",
                "observed_exit": result,
                "passed": selftest_refuses,
            }
        ],
        "pending_native_runner_cases": [
            {"id": c["id"], "title": c["title"]} for c in CASES
        ],
        "release_gate": "NOT READY until the pending cases pass on a native runner.",
    }

    with open(args.report, "w", encoding="utf-8") as fh:
        json.dump(report, fh, indent=2)
    print(f"wrote {args.report}")
    # Exit 0: this script ran and truthfully reported that the native matrix is
    # NOT RUN. The CI release gate must read the JSON and keep the gate BLOCKED.
    return 0


if __name__ == "__main__":
    main()

