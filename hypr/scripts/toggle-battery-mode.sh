#!/usr/bin/env bash

set -euo pipefail

hyprv_dir="$HOME/.config/HyprV"
hypr_lua_link="$HOME/.config/hypr/hyprland.lua"
quickshell_variant_script="$hyprv_dir/quickshell/scripts/set-variant.sh"

current_target="$(readlink -f "$hypr_lua_link" 2>/dev/null || true)"

if [[ "$current_target" == "$hyprv_dir/hypr/hyprland-battery.lua" ]]; then
    next_lua="$hyprv_dir/hypr/hyprland-daily.lua"
    next_variant="v2"
else
    next_lua="$hyprv_dir/hypr/hyprland-battery.lua"
    next_variant="bt"
fi

mkdir -p "$(dirname -- "$hypr_lua_link")"
ln -sfn "$next_lua" "$hypr_lua_link"
"$quickshell_variant_script" "$next_variant"

if command -v hyprctl >/dev/null 2>&1; then
    hyprctl reload >/dev/null 2>&1 || true
fi
