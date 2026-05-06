#!/usr/bin/env bash
set -euo pipefail

command -v hyprctl >/dev/null 2>&1 || exit 1
command -v jq >/dev/null 2>&1 || exit 1

active="$(hyprctl activewindow -j 2>/dev/null || true)"
monitors="$(hyprctl monitors -j 2>/dev/null || true)"

if [[ -z "$active" || -z "$monitors" ]]; then
    exit 1
fi

jq -nr -e --argjson active "$active" --argjson monitors "$monitors" '
    ($active.monitor // null) as $monitorId
    | ($active.at[0] // null) as $windowX
    | ($active.size[0] // null) as $windowWidth
    | if $monitorId == null or $windowX == null or $windowWidth == null or $windowWidth <= 0 then
        empty
      else
        ($monitors | map(select((.id // -999999) == $monitorId)) | first) as $monitor
        | if $monitor == null then
            empty
          else
            (($windowX - ($monitor.x // 0)) + $windowWidth | round)
          end
      end
'
