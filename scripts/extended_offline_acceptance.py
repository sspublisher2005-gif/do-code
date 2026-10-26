#!/usr/bin/env python3
"""Extended no-network black-box tests for Forge CLI's local workflows.

All commands run against a supplied compiled Forge binary in isolated temporary
projects. No provider keys are required; no live model or external network call is
made by this harness. This is a runtime acceptance harness, not a Rust test runner.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import platform
from pathlib import Path
from typing import Any

UUID_RE = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b", re.I)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--report", type=Path, default=Path("test-results/extended-offline-acceptance.json"))
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    results: list[dict[str, Any]] = []
    current: dict[str, Any] = {}

    def invoke(root: Path, *argv: str, timeout: int = 15) -> subprocess.CompletedProcess[str]:
        return subprocess.run([str(binary), "--root", str(root), *argv], capture_output=True,
                              text=True, encoding="utf-8", errors="replace", timeout=timeout,
                              env={**os.environ, "RUST_BACKTRACE": "0"}, check=False)

    def check(name: str, predicate: bool, detail: str = "") -> None:
        results.append({"name": name, "status": "PASS" if predicate else "FAIL", "detail": detail})
        print(f"{'PASS' if predicate else 'FAIL'} {name}" + (f": {detail}" if detail and not predicate else ""), flush=True)

    with tempfile.TemporaryDirectory(prefix="forge-extended-acceptance-") as work:
        root = Path(work) / "workspace with spaces – 中文"
        root.mkdir()
        (root / "add.py").write_text("def add(a, b):\n    return a + b\n", encoding="utf-8")
        (root / "README.md").write_text("# Offline acceptance fixture\n", encoding="utf-8")
        (root / ".env").write_text("FORGE_ACCEPTANCE_SECRET=never-show-this-secret\n", encoding="utf-8")

        version = subprocess.run([str(binary), "--version"], capture_output=True, text=True, timeout=10)
        check("version command", version.returncode == 0 and "forge " in version.stdout, version.stdout.strip())

        init = invoke(root, "init")
        check("init succeeds offline", init.returncode == 0 and "Network: not used" in init.stdout,
              init.stdout[-400:].strip())
        (root / "FORGE.md").write_text("# user-customized file\nkeep-me\n", encoding="utf-8")
        init_again = invoke(root, "init")
        check("init preserves user FORGE.md", init_again.returncode == 0 and "keep-me" in (root / "FORGE.md").read_text())

        doctor = invoke(root, "doctor")
        check("doctor does not claim sandbox is implemented", doctor.returncode == 0 and "Sandbox: not implemented" in doctor.stdout)
        config = invoke(root, "config", "show")
        try:
            config_doc = json.loads(config.stdout)
            config_ok = config.returncode == 0 and isinstance(config_doc, dict)
        except json.JSONDecodeError:
            config_ok = False
        check("config show emits parseable JSON", config_ok)

        index = invoke(root, "index")
        check("repository index builds without network", index.returncode == 0 and "No network request was made" in index.stdout)
        context = invoke(root, "context", "fix the add function", "--max-files", "12", "--max-chars", "4096")
        check("context pack includes task source", context.returncode == 0 and "def add" in context.stdout and "No network request was made" in context.stdout)
        check("context pack excludes .env secret", "never-show-this-secret" not in context.stdout)

        no_confirm = invoke(root, "memory", "save", "--type", "decision", "--title", "Confirm required", "--problem", "Test", "--decision", "No save", "--reason", "Safety")
        check("memory save refuses missing explicit confirmation", no_confirm.returncode != 0 and "--confirm" in no_confirm.stderr)
        empty_list = invoke(root, "memory", "list")
        check("unconfirmed memory was not written", empty_list.returncode == 0 and "Confirm required" not in empty_list.stdout)

        save = invoke(root, "memory", "save", "--type", "decision", "--title", "Use pathlib", "--problem", "Need safe paths", "--decision", "Use pathlib", "--reason", "Clear path semantics", "--evidence", "Python pathlib documentation", "--confirm")
        note_id_match = UUID_RE.search(save.stdout)
        note_id = note_id_match.group(0) if note_id_match else None
        check("confirmed memory save creates a note ID", save.returncode == 0 and note_id is not None)

        listing = invoke(root, "memory", "list")
        check("memory list returns saved note", bool(note_id and note_id in listing.stdout and "Use pathlib" in listing.stdout))
        shown = invoke(root, "memory", "show", note_id or "invalid")
        check("memory show returns human-readable note", bool(note_id and shown.returncode == 0 and "Use pathlib" in shown.stdout and "User confirmed: yes" in shown.stdout))
        searched = invoke(root, "memory", "search", "pathlib")
        check("memory search finds local note", bool(note_id and searched.returncode == 0 and note_id in searched.stdout))
        verified = invoke(root, "memory", "verify", note_id or "invalid")
        check("memory verify records user review honestly", bool(note_id and verified.returncode == 0 and "does not independently verify factual claims" in verified.stdout))

        export_path = root / "memory-export.json"
        export = invoke(root, "memory", "export", str(export_path))
        try:
            export_doc = json.loads(export_path.read_text(encoding="utf-8"))
            export_ok = export.returncode == 0 and len(export_doc.get("notes", [])) == 1
        except (OSError, json.JSONDecodeError):
            export_ok = False
        check("memory export creates parseable bundle", export_ok)

        reset_denied = invoke(root, "memory", "reset")
        check("memory reset refuses missing confirmation", reset_denied.returncode != 0 and "confirm" in reset_denied.stderr.lower())
        reset = invoke(root, "memory", "reset", "--confirm")
        check("confirmed reset deletes note", reset.returncode == 0 and "Deleted 1 note" in reset.stdout)
        import_denied = invoke(root, "memory", "import", str(export_path))
        check("memory import refuses missing confirmation", import_denied.returncode != 0 and "confirm" in import_denied.stderr.lower())
        imported = invoke(root, "memory", "import", str(export_path), "--confirm")
        check("confirmed import restores user-confirmed note", imported.returncode == 0 and "Imported 1" in imported.stdout)
        imported_list = invoke(root, "memory", "list")
        imported_id_match = UUID_RE.search(imported_list.stdout)
        imported_id = imported_id_match.group(0) if imported_id_match else None
        check("imported memory remains searchable/listed", "Use pathlib" in imported_list.stdout and imported_id is not None)
        forget_denied = invoke(root, "memory", "forget", imported_id or "invalid")
        check("forget refuses without approval", forget_denied.returncode != 0 and "--approve" in forget_denied.stderr)
        forget = invoke(root, "memory", "forget", imported_id or "invalid", "--approve")
        check("approved forget deletes note", bool(imported_id and forget.returncode == 0 and "deleted locally after approval" in forget.stdout))

        pilot = invoke(root, "pilot", "Choose a safe local metadata strategy", "--option", "SQLite", "--option", "plain files", "--privacy", "--risk", "medium")
        decision_id_match = UUID_RE.search(pilot.stdout)
        decision_id = decision_id_match.group(0) if decision_id_match else None
        check("Decision Arena creates an offline brief", pilot.returncode == 0 and decision_id is not None and "OFFLINE DECISION BRIEF" in pilot.stdout and "no model or external research was used" in pilot.stdout.lower())
        check("Decision Arena does not fabricate dollar amounts", "$" not in pilot.stdout and "does not generate predicted USD costs" in pilot.stdout)
        decisions = invoke(root, "decisions", "list")
        check("saved Decision Arena record appears in history", bool(decision_id and decisions.returncode == 0 and decision_id in decisions.stdout))
        decision = invoke(root, "decisions", "show", decision_id or "invalid")
        check("Decision Arena detail is retrievable", bool(decision_id and decision.returncode == 0 and "Choose a safe local metadata strategy" in decision.stdout))

        freeze = invoke(root, "freeze", "now", "extended offline acceptance safety test")
        check("emergency freeze activates", freeze.returncode == 0 and "Forge frozen" in freeze.stdout)
        status = invoke(root, "freeze", "status")
        check("freeze status confirms active state", status.returncode == 0 and "Frozen: true" in status.stdout)
        live_attempt = invoke(root, "pilot", "must be blocked while frozen", "--option", "A", "--option", "B", "--live", "--approve-context-sharing")
        check("freeze blocks live model workflow before network", live_attempt.returncode != 0 and "FORGE_FROZEN" in live_attempt.stderr and "No prompt was sent" not in live_attempt.stdout)
        unfreeze_denied = invoke(root, "unfreeze")
        check("unfreeze requires explicit approval", unfreeze_denied.returncode != 0 and "--approve" in unfreeze_denied.stderr)
        unfreeze = invoke(root, "unfreeze", "--approve")
        check("approved unfreeze restores normal state", unfreeze.returncode == 0 and "Forge unfrozen: true" in unfreeze.stdout)

        sandbox_status = invoke(root, "sandbox", "status")
        check("sandbox status honestly discloses missing driver", sandbox_status.returncode == 0 and "NOT IMPLEMENTED" in sandbox_status.stdout)
        before = sha256(root / "add.py")
        sandbox_selftest = invoke(root, "sandbox", "self-test")
        check("sandbox self-test fails closed without executing commands", sandbox_selftest.returncode != 0 and "No command was executed" in sandbox_selftest.stderr and sha256(root / "add.py") == before)

        missions = invoke(root, "mission", "list")
        check("mission list reports initialized local state", missions.returncode == 0 and "0 mission(s)" in missions.stdout)
        audit = invoke(root, "audit", "--tail", "20")
        try:
            audit_doc = json.loads(audit.stdout)
            audit_ok = audit.returncode == 0 and isinstance(audit_doc, list) and len(audit_doc) > 0
        except json.JSONDecodeError:
            audit_ok = False
        check("audit trail remains parseable JSON and contains events", audit_ok)
        check("audit does not contain raw test secret", "never-show-this-secret" not in audit.stdout)

    failed = [item for item in results if item["status"] != "PASS"]
    report = {
        "suite": "Forge CLI extended offline acceptance",
        "measured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "environment": {"system": platform.system(), "platform": platform.platform(), "machine": platform.machine()},
        "binary": str(binary),
        "binary_sha256": sha256(binary),
        "network_policy": "no external network or live model calls by test harness; live request path is intentionally blocked via freeze",
        "summary": {"total": len(results), "passed": len(results) - len(failed), "failed": len(failed)},
        "results": results,
        "known_gaps_not_counted_as_passes": [
            "sandbox native driver is not implemented; this suite tests fail-closed behavior only",
            "memory propose/open commands are missing and are documented in the parity ledger",
            "mission resume remains inspection-only",
            "macOS and Windows were not tested by this Linux run",
        ],
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"\nSummary: {report['summary']['passed']} passed, {report['summary']['failed']} failed of {report['summary']['total']}")
    print(f"Report: {args.report.resolve()}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
