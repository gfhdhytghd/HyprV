#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import sys
import time

from agent_island import (
    cleanup_state,
    event_name,
    locked_state,
    normalize_event,
    normalize_source,
    response_deny,
    response_file,
    update_session_from_event,
)


def parse_payload() -> dict:
    raw = sys.stdin.buffer.read()
    if not raw:
        return {}
    try:
        payload = json.loads(raw.decode("utf-8"))
    except Exception:
        return {}
    return payload if isinstance(payload, dict) else {}


def emit_response(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False, separators=(",", ":")))
    sys.stdout.write("\n")
    sys.stdout.flush()


def wait_for_response(request_id: str, timeout_seconds: float) -> dict:
    path = response_file(request_id)
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        if path.exists():
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except Exception:
                data = {}
            try:
                path.unlink()
            except OSError:
                pass
            return data if isinstance(data, dict) else {}
        time.sleep(0.18)
    return {}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--event")
    args = parser.parse_args()

    source = normalize_source(args.source)
    if source is None:
        return 0

    payload = parse_payload()
    if not payload:
        return 0

    raw_event = event_name(payload, args.event)
    event = normalize_event(raw_event)
    if not event:
        return 0

    payload["_source"] = source
    payload.setdefault("_ppid", os.getppid())

    request_id = None
    with locked_state(write=True) as state:
        cleanup_state(state)
        _session, request = update_session_from_event(state, payload, source, event)
        if request:
            request_id = request["id"]

    if request_id:
        timeout_seconds = float(os.environ.get("HYPRV_AGENT_ISLAND_PERMISSION_TIMEOUT", "86400"))
        response = wait_for_response(request_id, timeout_seconds)
        if not response:
            response = response_deny("No HyprV island response")
        status = "running"
        if response.get("hookSpecificOutput", {}).get("decision", {}).get("behavior") == "deny":
            status = "idle"
        with locked_state(write=True) as state:
            pending = state.setdefault("pending_requests", {})
            request = pending.pop(request_id, None)
            sid = request.get("session_id") if isinstance(request, dict) else None
            if sid and sid in state.get("sessions", {}):
                state["sessions"][sid]["status"] = status
                state["sessions"][sid]["pending_request_id"] = ""
                state["sessions"][sid]["last_activity"] = time.time()
        emit_response(response)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
