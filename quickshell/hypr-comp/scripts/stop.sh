#!/usr/bin/env bash
set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
quickshell kill -p "$config_dir" >/dev/null 2>&1 || true
