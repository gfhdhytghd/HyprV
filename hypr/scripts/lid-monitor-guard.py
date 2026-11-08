#!/usr/bin/env python3
"""Reconcile the laptop panel with lid state and usable external outputs."""
import fcntl
import glob
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=4).stdout


def reconcile():
    lids = glob.glob('/proc/acpi/button/lid/*/state')
    if not lids:
        return
    closed = all('closed' in Path(p).read_text() for p in lids)
    monitors = json.loads(run('hyprctl', '-j', 'monitors', 'all'))
    panel = next((m for m in monitors if m['name'] == 'eDP-1'), None)
    external = any(not m['name'].startswith(('eDP-', 'HEADLESS-'))
                   and not m.get('disabled', False) and m.get('dpmsStatus', False)
                   for m in monitors)
    docked = closed and external
    if docked:
        if run('powerprofilesctl', 'get').strip() != 'performance':
            run('powerprofilesctl', 'set', 'performance')
        if panel and not panel.get('disabled', False):
            run('hyprctl', 'eval', 'hl.monitor({output="eDP-1", disabled=true})')
    elif panel is None or panel.get('disabled', False):
        # Reuse the current monitor rules, including HDR and brightness settings.
        run('hyprctl', 'eval', f'dofile({json.dumps(str(ROOT / "hypr/hyprland-monitors.lua"))})')
        run('hyprctl', 'dispatch', 'dpms', 'on', 'eDP-1')


def main():
    signature = os.environ['HYPRLAND_INSTANCE_SIGNATURE']
    lock = open(Path(os.environ['XDG_RUNTIME_DIR']) / f'hyprv-lid-{signature}.lock', 'w')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return
    while True:
        try:
            reconcile()
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            print(f'lid-monitor-guard: {error}', flush=True)
        time.sleep(0.5)


if __name__ == '__main__':
    main()
