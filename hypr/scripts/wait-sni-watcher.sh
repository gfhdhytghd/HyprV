#!/bin/sh
# Start a tray app only once a StatusNotifierWatcher owns its bus name (at most 30 s).
# Ayatana, Electron and fcitx tray items never re-register when the watcher appears
# after them, so a slow kded6 start used to leave the bar tray empty.
i=0
while ! busctl --user status org.kde.StatusNotifierWatcher >/dev/null 2>&1 && [ "$i" -lt 150 ]; do
  sleep 0.2
  i=$((i + 1))
done
exec "$@"
