#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json

from agent_island import locked_state, public_snapshot


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    with locked_state(write=True) as state:
        snapshot = public_snapshot(state)

    if args.json:
        print(json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")))
    else:
        print(f"active={str(snapshot['active']).lower()}")
        print(f"count={snapshot['count']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
