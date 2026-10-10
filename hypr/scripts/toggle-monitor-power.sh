#!/usr/bin/env bash
set -euo pipefail

# LG UltraFine ignores DDC D6=04 on this connection; use compositor DPMS.
exec 9>"${XDG_RUNTIME_DIR:?}/hyprv-monitor-power.lock"
flock -n 9 || exit 0

# Let the shortcut keys be released before switching the output off.
sleep 0.2
hyprctl dispatch 'hl.dsp.dpms({action = "toggle", monitor = "eDP-1"})'
