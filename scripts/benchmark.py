#!/usr/bin/env python3
"""Deterministic golden-task benchmark for Do Code.

What this measures: end-to-end harness mechanics — can a local, scripted
provider drive the agent tool loop to a stored, applicable patch, and do the
safety gates hold (sensitive paths, traversal, undo, output recovery)?

What it does NOT measure: model quality, model speed, or "better than X"
claims. The scripted fixture always behaves identically on every machine.

Honest limits: results are specific to this source revision + this fixture;
attach a real provider only via `--live ... --approve-context-sharing` with
your own keys and costs apply. This script never makes network calls beyond
loopback and never prints secrets.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def sha256_file(path: Path) -> str | None:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return None


def chat_response(content: str) -> bytes:
    body = json.dumps({
        "id": "fixture-req",
        "model": "fixture-model",
        "choices": [{"message": {"content": content}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 20},
    }).encode("utf-8")
    return (
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
        b"Content-Length: " + str(len(body)).encode() + b"\r\n"
        b"Connection: close\r\n\r\n" + body
    )


class SequencedFixtureHandler(BaseHTTPRequestHandler):
    """Answers the i-th request with the i-th scripted model turn."""

    responses: list[str] = []
    counter = 0
    lock = threading.Lock()

    def do_POST(self) -> None:  # noqa: N802 (BaseHTTPRequestHandler API)
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        with self.lock:
            index = self.counter
            self.counter += 1
            response = self.responses[index] if index < len(self.responses) else None
        if response is None:
            self.send_response(500)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        payload = chat_response(response)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload) - len(payload.split(b"\r\n\r\n", 1)[0]) - 4))
        self.end_headers()
        self.wfile.write(payload.split(b"\r\n\r\n", 1)[1])

    def log_message(self, fmt: str, *args) -> None:  # noqa: A002
        pass


def spawn_fixture(responses: list[str]) -> ThreadingHTTPServer:
    handler_cls = type(
        "Handler",
        (SequencedFixtureHandler,),
        {
            "responses": responses,
            "counter": 0,
            "lock": threading.Lock(),
        },
    )
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler_cls)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def run_task(binary: Path, tasks_file: Path, task: dict, base_url: str, fixture_stop: threading.Event) -> dict:
    root = Path(tempfile.mkdtemp(prefix="docode-bench-"))
    seed = {name: content for name, content in task["seed_files"].items()}
    snapshots: dict[str, str | None] = {}
    for name, content in seed.items():
        path = root / name
        path.write_text(content, encoding="utf-8")
        snapshots[name] = sha256_file(path)

    init = subprocess.run([str(binary), "--root", str(root), "init"],
                          capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
    if init.returncode != 0:
        return {"id": task["id"], "status": "FAIL", "detail": f"init failed: {init.stdout[-500:]}{init.stderr[-500:]}"}

    config_path = root / "docode.config.toml"
    config_text = config_path.read_text(encoding="utf-8")
    config_text = config_text.replace(
        'base_url = "https://api.openai.com/v1"', f'base_url = "{base_url}"')
    config_path.write_text(config_text, encoding="utf-8")

    agent = subprocess.run(
        [str(binary), "--root", str(root), "agent", task["prompt"], "--live", "--approve-context-sharing"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)

    out = (agent.stdout or "") + (agent.stderr or "")
    mission_id = None
    m = re.search(r"Mission:\s+([0-9a-f-]{36})", out)
    if m:
        mission_id = m.group(1)

    detail_parts: list[str] = []
    ok = agent.returncode == task["expect_exit"]
    if agent.returncode != task["expect_exit"]:
        detail_parts.append(f"agent exit {agent.returncode} != expected {task['expect_exit']}")

    if ok:
        if task.get("undo"):
            if mission_id is None:
                ok = False
                detail_parts.append("no mission id for undo flow")
            else:
                apply_out = subprocess.run(
                    [str(binary), "--root", str(root), "apply", mission_id, "--approve"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
                if apply_out.returncode != 0:
                    ok = False
                    detail_parts.append(f"apply failed: {(apply_out.stdout + apply_out.stderr)[-400:]}")
                else:
                    undo_out = subprocess.run(
                        [str(binary), "--root", str(root), "undo", mission_id, "--approve"],
                        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
                    if undo_out.returncode != 0:
                        ok = False
                        detail_parts.append(f"undo failed: {(undo_out.stdout + undo_out.stderr)[-400:]}")
        else:
            # Apply the stored patch so the checks see the final state.
            if agent.returncode == 0 and mission_id is not None:
                apply_out = subprocess.run(
                    [str(binary), "--root", str(root), "apply", mission_id, "--approve"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
                if apply_out.returncode != 0:
                    ok = False
                    detail_parts.append(f"apply failed: {(apply_out.stdout + apply_out.stderr)[-400:]}")

    for check in task["accept"]:
        kind = check["type"]
        target = check.get("path", "")
        if kind == "file_absent":
            if (root / target).exists():
                ok = False
                detail_parts.append(f"path unexpectedly present: {target}")
            continue
        path = root / target
        if kind == "file_unchanged":
            if not path.exists():
                ok = False
                detail_parts.append(f"missing file: {target}")
            elif snapshots.get(target) != sha256_file(path):
                ok = False
                detail_parts.append(f"file changed but should not have: {target}")
            continue
        if not path.exists():
            ok = False
            detail_parts.append(f"missing file: {target}")
            continue
        content = path.read_text(encoding="utf-8", errors="replace")
        if kind == "file_equals" and content != check["text"]:
            ok = False
            detail_parts.append(f"content mismatch in {target}: {content[:200]!r}")
        elif kind == "file_contains" and check["text"] not in content:
            ok = False
            detail_parts.append(f"expected text missing in {target}: {check['text']!r}")

    return {
        "id": task["id"],
        "status": "PASS" if ok else "FAIL",
        "detail": "; ".join(detail_parts) or "ok",
        "agent_output_tail": out[-800:],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--tasks", type=Path, default=REPO / "benchmarks" / "tasks.json")
    parser.add_argument("--report", type=Path, default=None)
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    tasks = json.loads(args.tasks.read_text(encoding="utf-8"))

    results = []
    for task in tasks:
        server = spawn_fixture([json.dumps(step) for step in task["model_script"]])
        base_url = f"http://127.0.0.1:{server.server_address[1]}/v1"
        started = time.time()
        result = run_task(binary, args.tasks, task, base_url, None)  # type: ignore[arg-type]
        result["duration_s"] = round(time.time() - started, 2)
        server.shutdown()
        results.append(result)
        print(f"{result['status']:4}  {result['id']:34}  {result['duration_s']}s  {result['detail']}")

    passed = sum(1 for r in results if r["status"] == "PASS")
    summary = {
        "schema": "docode-benchmark-report/v1",
        "date_utc": datetime.now(timezone.utc).isoformat(),
        "binary": str(binary),
        "environment": {"os": platform.platform(), "python": platform.python_version()},
        "task_count": len(results),
        "passed": passed,
        "failed": len(results) - passed,
        "claims_limitations": [
            "Measures harness mechanics only (tool-loop, patch storage, apply, undo, safety gates) with a scripted local fixture.",
            "Measures NOTHING about model quality, intelligence, speed on real providers, or rankings vs Codex/Claude Code/OpenCode.",
            "Ranking claims require: qualified Windows sandbox + independent review + real-provider benchmark runs you opt into.",
        ],
        "results": results,
    }
    if args.report is None:
        results_dir = REPO / "benchmarks" / "results"
        results_dir.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
        args.report = results_dir / f"golden-tasks-{stamp}.json"
    args.report.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"\n{passed}/{len(results)} tasks passed. Report: {args.report}")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
