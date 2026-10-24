#!/usr/bin/env python3
"""Run a recorded agent-island sequence through the LEGACY observer.

usage: agent_parity_legacy.py SCENARIO.json LEGACY_agent_island.py > RECORDING.json

The legacy module is imported read-only and only its globals are replaced:
`time.time`, `list_processes`, `proc_cwd` and `process_exists` answer from
the scenario. Nothing touches /proc, the hook directory or any file.

Each tick is one legacy observer tick (desktop-state.py AgentObserver):
hook writes land in the legacy state first, then `public_snapshot(state)`
runs, scanning when the lease is held (rate-limited by the module's own
`last_process_scan`); without the lease it runs with `scan_processes=False`.
The output also carries the hook-only `state.json` text the read-only native
actor reads at that tick (hooks preserve key order; the scan never writes).
"""
from __future__ import annotations

import importlib.util
import json
import sys
import types

NAMED = ("codex", "claude", "systemd-inhibit")


def main() -> None:
    scenario = json.load(open(sys.argv[1]))
    spec = importlib.util.spec_from_file_location("legacy_agent_island", sys.argv[2])
    legacy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(legacy)

    clock = [0.0]
    procs: dict[int, dict] = {}
    legacy.time = types.SimpleNamespace(time=lambda: clock[0])
    legacy.list_processes = lambda: {
        p["pid"]: {
            "cmdline": list(p["argv"]) if p["comm"] in NAMED else [],
            "ppid": p["ppid"],
            "start_ticks": p["start"],
        }
        for p in procs.values()
    }
    legacy.proc_cwd = lambda pid: procs[pid].get("cwd", "") if pid in procs else ""
    legacy.process_exists = lambda pid, processes=None: pid is not None and pid > 0 and pid in procs

    clock[0] = scenario["ticks"][0]["t"]
    state = legacy.default_state()
    hook_file = {"version": 1, "updated_at": clock[0], "sessions": {}, "pending_requests": {}}
    lease = False
    out = []
    for tick in scenario["ticks"]:
        clock[0] = tick["t"]
        if "procs" in tick:
            procs.clear()
            procs.update({p["pid"]: p for p in tick["procs"]})
        lease = tick.get("lease", lease)
        for sid, patch in tick.get("hook", {}).items():
            # A hook loads state.json, updates (or appends) one session, writes it.
            state["sessions"].setdefault(sid, {}).update(patch)
            hook_file["sessions"].setdefault(sid, {}).update(patch)
            hook_file["updated_at"] = clock[0]
        snapshot = legacy.public_snapshot(state, scan_processes=lease)
        out.append({
            "t": tick["t"],
            "file": json.dumps(hook_file, separators=(",", ":")),
            "legacy": [[s["id"], s["status"], s.get("detail", "")] for s in snapshot["sessions"]],
        })
    json.dump({"name": scenario["name"], "ticks": out}, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
