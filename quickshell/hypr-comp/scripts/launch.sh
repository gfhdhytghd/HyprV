#!/usr/bin/env bash
set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
exec quickshell -p "$config_dir" -n -d
