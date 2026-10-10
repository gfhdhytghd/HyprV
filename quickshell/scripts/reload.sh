#!/usr/bin/env bash

set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell"
script_dir="$config_dir/scripts"

"$script_dir/ui-state.sh" ensure >/dev/null 2>&1 || true
# Keep manual theme changes intact across reload; explicit theme toggles still call apply-ui-state.sh.
# "$script_dir/apply-ui-state.sh" apply >/dev/null 2>&1 || true
# Dell firmware manages laptop cooling.

# Select every instance of this exact configuration, including shells launched
# with a different display environment. Keep other Quickshell configurations.
python3 - "$config_dir" <<'PY_STOP'
import os, pathlib, signal, sys, time
config_dir = pathlib.Path(sys.argv[1]).resolve()
pids = []
for proc in pathlib.Path("/proc").iterdir():
    if not proc.name.isdigit():
        continue
    try:
        if proc.stat().st_uid != os.getuid():
            continue
        argv = (proc / "cmdline").read_bytes().decode().split("\0")
        if pathlib.Path(argv[0]).name != "quickshell" or "ipc" in argv:
            continue
        for flag in ("-p", "--path"):
            if flag in argv and pathlib.Path(argv[argv.index(flag) + 1]).resolve() == config_dir:
                pids.append(int(proc.name))
                break
    except (OSError, IndexError, UnicodeError):
        continue
for pid in pids:
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
time.sleep(0.2)
for pid in pids:
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
PY_STOP
sleep 0.2
exec quickshell -p "$config_dir" -d
