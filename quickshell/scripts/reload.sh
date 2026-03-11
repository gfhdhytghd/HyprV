#!/usr/bin/env bash

set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell"
script_dir="$config_dir/scripts"

"$script_dir/ui-state.sh" ensure >/dev/null 2>&1 || true
"$script_dir/apply-ui-state.sh" apply >/dev/null 2>&1 || true

quickshell kill -p "$config_dir" >/dev/null 2>&1 || true
sleep 0.2
exec quickshell -p "$config_dir" -d
