#!/usr/bin/env python3
"""Push agent state changes with a slow process/expiry fallback. Requires python-gobject."""
from __future__ import annotations

import json
import os
import sys
from gi.repository import Gio, GLib
from agent_island import locked_state, public_snapshot, state_dir, state_file, write_state


def main() -> None:
    path = state_file()
    last_written = None
    pending = 0

    def refresh() -> bool:
        nonlocal last_written, pending
        pending = 0
        try:
            with locked_state(write=False) as state:
                snapshot = public_snapshot(state)
                write_state(state)
                # Capture our own write under the lock, so a concurrent hook can
                # never be mistaken for an already delivered snapshot.
                last_written = path.read_bytes()
            print(json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")), flush=True)
        except (OSError, ValueError) as error:
            print(f"agent state: {error}", file=sys.stderr, flush=True)
        return GLib.SOURCE_REMOVE

    def changed(_monitor, file, other, _event) -> None:
        nonlocal pending
        if not any(item and item.get_basename() == path.name for item in (file, other)):
            return
        try:
            if path.read_bytes() == last_written:
                return
        except OSError:
            return
        # Coalesce bursts and atomic renames, without delaying a pending refresh.
        if not pending:
            pending = GLib.timeout_add(150, refresh)

    monitor = Gio.File.new_for_path(str(state_dir())).monitor_directory(Gio.FileMonitorFlags.WATCH_MOVES, None)
    monitor.connect("changed", changed)
    refresh()

    def fallback() -> bool:
        nonlocal pending
        if pending:
            GLib.source_remove(pending)
            pending = 0
        refresh()
        return GLib.SOURCE_CONTINUE

    def requested(_fd, condition) -> bool:
        if condition & GLib.IOCondition.HUP:
            return GLib.SOURCE_REMOVE
        if not os.read(sys.stdin.fileno(), 4096):
            return GLib.SOURCE_REMOVE
        fallback()
        return GLib.SOURCE_CONTINUE

    GLib.io_add_watch(sys.stdin, GLib.IOCondition.IN | GLib.IOCondition.HUP, requested)
    GLib.timeout_add_seconds(5, fallback)
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
