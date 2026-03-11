#!/usr/bin/env bash

# Simple battery notifier for Hyprland environments
# - Sends a critical notification when battery < 10% and discharging
# - Debounces to avoid spam until battery recovers above a reset threshold

set -euo pipefail

THRESHOLD=${THRESHOLD:-10}
RESET_THRESHOLD=${RESET_THRESHOLD:-15}
CHECK_INTERVAL=${CHECK_INTERVAL:-60}   # seconds

# Prefer first BAT* device
BAT_PATH=""
for d in /sys/class/power_supply/BAT*; do
  if [ -d "$d" ]; then
    BAT_PATH="$d"
    break
  fi
done

if [ -z "$BAT_PATH" ]; then
  # No battery found; exit silently
  exit 0
fi

STATE_FILE="${XDG_RUNTIME_DIR:-/tmp}/.battery_notify_state"
last_notified="0"

if [ -f "$STATE_FILE" ]; then
  last_notified=$(cat "$STATE_FILE" 2>/dev/null || echo 0)
fi

notify_low() {
  local pct="$1"
  local icon="battery-caution"
  # Use canonical-private-synchronous to replace prior notifications of same tag
  notify-send -u critical \
    -h string:x-canonical-private-synchronous:battery-low \
    -i "$icon" \
    "电量过低 (${pct}%)" "请尽快连接电源以避免关机。"
}

while :; do
  # Read capacity and status
  if ! pct=$(cat "$BAT_PATH/capacity" 2>/dev/null); then
    sleep "$CHECK_INTERVAL"; continue
  fi
  status=$(cat "$BAT_PATH/status" 2>/dev/null || echo unknown)

  # Only warn when discharging and below threshold
  if [ "$status" = "Discharging" ] && [ "$pct" -lt "$THRESHOLD" ]; then
    if [ "$last_notified" -eq 0 ]; then
      notify_low "$pct"
      last_notified=1
      echo "$last_notified" >"$STATE_FILE" 2>/dev/null || true
    fi
  else
    # Reset debounce once battery recovers sufficiently or charges
    if [ "$pct" -ge "$RESET_THRESHOLD" ] || [ "$status" = "Charging" ] || [ "$status" = "Full" ]; then
      if [ "$last_notified" -ne 0 ]; then
        last_notified=0
        echo "$last_notified" >"$STATE_FILE" 2>/dev/null || true
      fi
    fi
  fi

  sleep "$CHECK_INTERVAL"
done

