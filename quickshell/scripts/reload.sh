#!/usr/bin/env bash

set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell"
script_dir="$config_dir/scripts"

# These small windows share one render loop instead of per-window render threads.
# Keep an explicit session override available for profiling or other GPUs.
export QSG_RENDER_LOOP="${QSG_RENDER_LOOP:-basic}"

"$script_dir/ui-state.sh" ensure >/dev/null 2>&1 || true
# Keep manual theme changes intact across reload; explicit theme toggles still call apply-ui-state.sh.
# "$script_dir/apply-ui-state.sh" apply >/dev/null 2>&1 || true
/usr/local/libexec/hyprv-fanctl restore >/dev/null 2>&1 || true

quickshell kill -p "$config_dir" >/dev/null 2>&1 || true
sleep 0.2
exec quickshell -p "$config_dir" -d
