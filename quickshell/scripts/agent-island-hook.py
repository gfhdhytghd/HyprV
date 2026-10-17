#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import sys

from agent_island import (
    cleanup_state,
    event_name,
    locked_state,
    normalize_event,
    normalize_source,
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

    with locked_state(write=True) as state:
        cleanup_state(state)
        _session, request = update_session_from_event(state, payload, source, event)
        # Observe permission/input state; the agent's own UI handles responses.
        # Never hold its hook open waiting for a bar control.
        if request:
            state.get("pending_requests", {}).pop(request["id"], None)
            state["sessions"][_session["id"]].pop("pending_request_id", None)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
