#!/usr/bin/env bash

set -euo pipefail

hyprv_dir="$HOME/.config/HyprV"
hypr_conf_link="$HOME/.config/hypr/hyprland.conf"
hypr_lua_link="$HOME/.config/hypr/hyprland.lua"
quickshell_variant_script="$hyprv_dir/quickshell/scripts/set-variant.sh"

current_target="$(readlink -f "$hypr_lua_link" 2>/dev/null || readlink -f "$hypr_conf_link" 2>/dev/null || true)"

if [[ "$current_target" == "$hyprv_dir/hypr/hyprland-battery.lua" || "$current_target" == "$hyprv_dir/hypr/hyprland-battery.conf" ]]; then
    next_conf="$hyprv_dir/hypr/hyprland-daily.conf"
    next_lua="$hyprv_dir/hypr/hyprland-daily.lua"
    next_variant="v2"
else
    next_conf="$hyprv_dir/hypr/hyprland-battery.conf"
    next_lua="$hyprv_dir/hypr/hyprland-battery.lua"
    next_variant="bt"
fi

mkdir -p "$(dirname -- "$hypr_conf_link")"
ln -sfn "$next_conf" "$hypr_conf_link"
ln -sfn "$next_lua" "$hypr_lua_link"
"$quickshell_variant_script" "$next_variant"

if command -v hyprctl >/dev/null 2>&1; then
    hyprctl reload >/dev/null 2>&1 || true
fi
