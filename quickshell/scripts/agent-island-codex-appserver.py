#!/usr/bin/env python3

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from agent_island import clean_for_json, locked_state, project_name


APP_PREFIX = "codexapp:"


def status_from_thread_status(status: dict[str, Any] | None) -> tuple[str, str, float]:
    if not isinstance(status, dict):
        return "processing", "Starting", 0.2
    kind = status.get("type")
    if kind == "active":
        flags = status.get("activeFlags")
        flags = flags if isinstance(flags, list) else []
        if "waitingOnApproval" in flags:
            return "waitingApproval", "Waiting for approval", 0.82
        if "waitingOnUserInput" in flags:
            return "waitingQuestion", "Waiting for input", 0.82
        return "running", "Running", 0.46
    if kind == "idle":
        return "completed", "Idle", 1.0
    if kind in ("systemError", "notLoaded"):
        return "idle", str(kind), 0.0
    return "processing", str(kind or "Working"), 0.32


def update_thread(state: dict[str, Any], thread: dict[str, Any]) -> None:
    thread_id = thread.get("id")
    if not isinstance(thread_id, str) or not thread_id:
        return
    session_id = APP_PREFIX + thread_id
    now = time.time()
    status, detail, progress = status_from_thread_status(thread.get("status"))
    sessions = state.setdefault("sessions", {})
    session = sessions.get(session_id) if isinstance(sessions.get(session_id), dict) else {}
    session.setdefault("id", session_id)
    session["source"] = "codex"
    session["source_label"] = "Codex"
    session["provider"] = "app-server"
    session["provider_session_id"] = thread_id
    session["title"] = thread.get("name") if isinstance(thread.get("name"), str) and thread.get("name") else thread_id[:8]
    cwd = thread.get("cwd") if isinstance(thread.get("cwd"), str) else session.get("cwd") or ""
    session["cwd"] = cwd
    session["project"] = project_name(cwd)
    if isinstance(thread.get("preview"), str) and thread["preview"]:
        session["detail"] = thread["preview"]
    else:
        session["detail"] = detail
    if isinstance(thread.get("path"), str) and thread["path"]:
        session["transcript_path"] = thread["path"]
    session["status"] = status
    session["event"] = "app-server"
    session["progress"] = progress
    session["last_activity"] = now
    session.setdefault("started_at", now)
    if status == "completed":
        session["completed_at"] = now
    sessions[session_id] = clean_for_json(session)


def update_thread_status(state: dict[str, Any], thread_id: str, status_payload: dict[str, Any] | None) -> None:
    session_id = APP_PREFIX + thread_id
    session = state.setdefault("sessions", {}).get(session_id)
    if not isinstance(session, dict):
        return
    status, detail, progress = status_from_thread_status(status_payload)
    session["status"] = status
    session["detail"] = detail
    session["progress"] = progress
    session["last_activity"] = time.time()
    if status == "completed":
        session["completed_at"] = time.time()


def close_thread(state: dict[str, Any], thread_id: str) -> None:
    state.setdefault("sessions", {}).pop(APP_PREFIX + thread_id, None)


def handle_message(message: dict[str, Any]) -> None:
    method = message.get("method")
    params = message.get("params")
    if not isinstance(method, str) or not isinstance(params, dict):
        return
    with locked_state(write=True) as state:
        if method == "thread/started":
            thread = params.get("thread")
            if isinstance(thread, dict):
                update_thread(state, thread)
        elif method == "thread/status/changed":
            thread_id = params.get("threadId")
            if isinstance(thread_id, str):
                status = params.get("status")
                update_thread_status(state, thread_id, status if isinstance(status, dict) else None)
        elif method == "thread/closed":
            thread_id = params.get("threadId")
            if isinstance(thread_id, str):
                close_thread(state, thread_id)


def run_once(codex_path: str) -> int:
    proc = subprocess.Popen(
        [codex_path, "app-server", "--listen", "stdio://"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )
    assert proc.stdin is not None
    assert proc.stdout is not None
    init = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {"name": "HyprV", "version": "agent-island"},
            "capabilities": None,
        },
    }
    proc.stdin.write(json.dumps(init, separators=(",", ":")) + "\n")
    proc.stdin.flush()
    for line in proc.stdout:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except Exception:
            continue
        if isinstance(message, dict):
            handle_message(message)
    return proc.wait()


def main() -> int:
    codex_path = shutil.which("codex")
    if not codex_path:
        return 0
    while True:
        try:
            run_once(codex_path)
        except KeyboardInterrupt:
            return 0
        except Exception as exc:
            print(f"agent-island-codex-appserver: {exc}", file=sys.stderr)
        time.sleep(4)


if __name__ == "__main__":
    raise SystemExit(main())
