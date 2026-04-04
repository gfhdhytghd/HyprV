#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/bluetooth-common.sh"

run_quick() {
    timeout 2s "$@" 2>/dev/null || true
}

# WiFi radio state
wifi_enabled=false
if command -v nmcli >/dev/null 2>&1; then
    state="$(run_quick nmcli radio wifi)"
    [[ "$state" == "enabled" ]] && wifi_enabled=true
fi
printf 'wifi_enabled=%s\n' "$wifi_enabled"

# Bluetooth power state
bluetooth_enabled=false
if command -v bluetoothctl >/dev/null 2>&1; then
    if [[ "$(bt_property_bool "$(bt_show_capture)" "Powered")" == "true" ]]; then
        bluetooth_enabled=true
    fi
fi
printf 'bluetooth_enabled=%s\n' "$bluetooth_enabled"

# Screen brightness
brightness=50
if command -v brightnessctl >/dev/null 2>&1; then
    pct="$(timeout 2s sh -lc "brightnessctl -m 2>/dev/null | cut -d, -f4 | tr -d '%'" 2>/dev/null || true)"
    if [[ -n "$pct" && "$pct" =~ ^[0-9]+$ ]]; then
        brightness="$pct"
    fi
fi
printf 'brightness=%s\n' "$brightness"

# DND (Do Not Disturb) state
dnd=false
if command -v swaync-client >/dev/null 2>&1; then
    dnd_state="$(run_quick swaync-client -D)"
    [[ "$dnd_state" == "true" ]] && dnd=true
fi
printf 'dnd=%s\n' "$dnd"

# Screen recording detection
recording=false
if pgrep -x 'wf-recorder' >/dev/null 2>&1 || \
   pgrep -x 'wl-screenrec' >/dev/null 2>&1 || \
   pgrep -x 'gpu-screen-recorder' >/dev/null 2>&1; then
    recording=true
fi
printf 'recording=%s\n' "$recording"

# Power profile
power_profile="balanced"
if command -v powerprofilesctl >/dev/null 2>&1; then
    power_profile="$(run_quick powerprofilesctl get)"
    [[ -z "$power_profile" ]] && power_profile="balanced"
fi
printf 'power_profile=%s\n' "$power_profile"

# Prevent sleep state
prevent_sleep=false
if [[ -x "${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" ]]; then
    inhibit_status="$("${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh" status 2>/dev/null || true)"
    if [[ "$inhibit_status" == *"enabled=true"* ]]; then
        prevent_sleep=true
    fi
fi
printf 'prevent_sleep=%s\n' "$prevent_sleep"
