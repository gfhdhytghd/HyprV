#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import time

from agent_island import locked_state, public_snapshot


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--watch", action="store_true", help="Stream task state without restarting Python")
    args = parser.parse_args()

    previous = None
    while True:
        with locked_state(write=True) as state:
            snapshot = public_snapshot(state)

        # The state file timestamp changes even when no task has changed.
        # Keep it in one-shot output for compatibility, but omit it from streams.
        if args.watch:
            snapshot.pop("updated_at", None)
            snapshot.pop("pending", None)
            snapshot.pop("pending_count", None)
        output = json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")) if args.json else (
            f"active={str(snapshot['active']).lower()}\ncount={snapshot['count']}"
        )
        if output != previous:
            print(output, flush=True)
            previous = output
        if not args.watch:
            break
        time.sleep(0.65 if snapshot["active"] else 1.2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
