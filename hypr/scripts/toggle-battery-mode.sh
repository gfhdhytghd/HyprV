#!/usr/bin/env bash

set -euo pipefail

hyprv_dir="$HOME/.config/HyprV"
hypr_lua_link="$HOME/.config/hypr/hyprland.lua"
hyprv_shell="$HOME/.local/bin/hyprv-shell"

current_target="$(readlink -f "$hypr_lua_link" 2>/dev/null || true)"

if [[ "$current_target" == "$hyprv_dir/hypr/hyprland-battery.lua" ]]; then
    next_lua="$hyprv_dir/hypr/hyprland-daily.lua"
    next_variant="v2"
else
    next_lua="$hyprv_dir/hypr/hyprland-battery.lua"
    next_variant="bt"
    # Set once on entry; subsequent manual profile changes remain in effect.
    powerprofilesctl set power-saver
fi

mkdir -p "$(dirname -- "$hypr_lua_link")"
ln -sfn "$next_lua" "$hypr_lua_link"
HYPRV_PROFILE=laptop "$hyprv_shell" desktop setVariant "$next_variant"

if command -v hyprctl >/dev/null 2>&1; then
    hyprctl reload >/dev/null 2>&1 || true
fi
