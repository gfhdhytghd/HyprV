#!/usr/bin/python3 -I
"""Canned answer for every command the original shell runs inside the nested capture.

The nested quickshell never runs a real helper. Its PATH starts with a shim directory
where every external command name the QML uses links here, and each script under the
fake $HOME (`~/.config/HyprV/quickshell/scripts/*`, `~/.local/bin/dcal`, ...) is a wrapper
that execs this file with `--cmd <script> args…`. Semantics follow parity-host's
Process mock (qml-mocks/Quickshell/Io/Process.qml): the first fixture `processes` entry
whose `match` is a substring of the space-joined command line answers it with
`stdout`, `lines: [{at, data}]` (virtual ms after start + `delay_ms`), `exitCode`, and
`stream: true` (keep running). Unmatched commands exit 1 with no output and are logged.

Environment (set by run.py):
  FS_PROCESSES   JSON file with the fixture's process entries (@HOME@ already resolved)
  FS_STUB_LOG    append-only log of every command line and how it was answered
  VCLOCK_FILE    the harness's virtual clock (int64 µs, little endian)
"""
import json
import os
import select
import struct
import sys


def virtual_ms():
    try:
        with open(os.environ["VCLOCK_FILE"], "rb") as f:
            return struct.unpack("<q", f.read(8))[0] / 1000.0
    except (KeyError, OSError, struct.error):
        return 0.0


def real_sleep(seconds):
    # Not time.sleep(): under the vclock preload it sleeps to an absolute CLOCK_MONOTONIC
    # deadline computed from the *virtual* clock. select() takes a relative timeout.
    select.select([], [], [], seconds)


def wait_until(target_ms):
    # Real sleeps, virtual deadline: the harness moves the clock in ticks.
    while virtual_ms() < target_ms:
        real_sleep(0.001)


def log(line, outcome):
    path = os.environ.get("FS_STUB_LOG")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(json.dumps({"vms": round(virtual_ms(), 3), "cmd": line, "answer": outcome}) + "\n")


def main():
    argv = sys.argv[1:]
    if argv[:1] == ["--cmd"]:
        command = argv[1:]
    else:
        # PATH shim: argv[0] is the symlink's path, i.e. the command name the QML used.
        command = [os.path.basename(sys.argv[0])] + argv
    line = " ".join(command)
    start = virtual_ms()
    try:
        with open(os.environ["FS_PROCESSES"], encoding="utf-8") as f:
            entries = json.load(f)
    except (KeyError, OSError, ValueError):
        entries = []
    entry = next((e for e in entries if e.get("match") and e["match"] in line), None)
    if entry is None:
        log(line, "unmatched")
        return 1
    log(line, entry["match"])
    delay = float(entry.get("delay_ms", 0))
    out = sys.stdout
    if delay:
        wait_until(start + delay)
    if "stdout" in entry:
        out.write(entry["stdout"])
        out.flush()
    for item in entry.get("lines", []):
        wait_until(start + delay + float(item.get("at", 0)))
        out.write(item["data"] + "\n")
        out.flush()
    if entry.get("stream"):
        parent = os.getppid()
        while os.getppid() == parent:  # exit with the shell, even if it crashed
            real_sleep(0.5)
        return 0
    return int(entry.get("exitCode", 0))


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (BrokenPipeError, KeyboardInterrupt):
        sys.exit(0)
