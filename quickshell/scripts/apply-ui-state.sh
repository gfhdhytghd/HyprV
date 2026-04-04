#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
state_script="$script_dir/ui-state.sh"

ensure_parent() {
    mkdir -p "$(dirname -- "$1")"
}

theme_suffix() {
    case "${1:-}" in
        dark)
            printf '%s\n' '-dark'
            ;;
        light)
            printf '%s\n' ''
            ;;
        *)
            return 1
            ;;
    esac
}

theme_label() {
    case "${1:-}" in
        dark)
            printf 'Dark\n'
            ;;
        light)
            printf 'Light\n'
            ;;
        *)
            return 1
            ;;
    esac
}

variant_prefix() {
    case "${1:-}" in
        v1|v2|v3|bt)
            printf '%s\n' "$1"
            ;;
        *)
            return 1
            ;;
    esac
}

variant_label() {
    case "${1:-}" in
        v1)
            printf 'HyprV1\n'
            ;;
        v2)
            printf 'HyprV2\n'
            ;;
        v3)
            printf 'HyprV3\n'
            ;;
        bt)
            printf 'HyprV Battery\n'
            ;;
        *)
            return 1
            ;;
    esac
}

notify_ui_state() {
    if command -v notify-send >/dev/null 2>&1; then
        notify-send -h string:x-canonical-private-synchronous:sys-notify -u low "$1"
    fi
}

load_current_state() {
    theme="$("$state_script" get-theme)"
    variant="$("$state_script" get-variant)"
    suffix="$(theme_suffix "$theme")"
    mode_label="$(theme_label "$theme")"
    variant_name="$(variant_prefix "$variant")"
}

apply_links_and_theme() {
    local wofi_target background_target rofi_target alacritty_target ghostty_target swaync_target
    local orchis_suffix color_scheme_suffix

    wofi_target="$HOME/.config/HyprV/wofi/style/${variant_name}-style${suffix}.css"
    background_target="$HOME/.config/HyprV/backgrounds/${variant_name}-background${suffix}.jpg"
    rofi_target="$HOME/.config/HyprV/rofi/colors${suffix}.rasi"
    alacritty_target="$HOME/.config/HyprV/alacritty/alacritty${suffix}.toml"
    ghostty_target="$HOME/.config/HyprV/ghostty/ghostty${suffix}.toml"
    swaync_target="$HOME/.config/HyprV/swaync/style${suffix}.css"

    ensure_parent "$HOME/.config/wofi/style.css"
    ensure_parent "$HOME/.config/rofi/colors.rasi"
    ensure_parent "$HOME/.config/alacritty/alacritty.toml"
    ensure_parent "$HOME/.config/ghostty/config"
    ensure_parent "$HOME/.config/swaync/style.css"

    if [[ -f "$wofi_target" ]]; then
        ln -sfn "$wofi_target" "$HOME/.config/wofi/style.css"
    fi

    if [[ -f "$rofi_target" ]]; then
        ln -sfn "$rofi_target" "$HOME/.config/rofi/colors.rasi"
    fi

    if [[ -f "$alacritty_target" ]]; then
        ln -sfn "$alacritty_target" "$HOME/.config/alacritty/alacritty.toml"
    fi

    if [[ -f "$ghostty_target" ]]; then
        ln -sfn "$ghostty_target" "$HOME/.config/ghostty/config"
    fi

    if [[ -f "$swaync_target" ]]; then
        ln -sfn "$swaync_target" "$HOME/.config/swaync/style.css"
        if command -v swaync-client >/dev/null 2>&1; then
            swaync-client -rs >/dev/null 2>&1 || true
        fi
    fi

    if command -v hyprctl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
        hyprctl clients -j 2>/dev/null | jq -r '
            .[] | select((.class // "" | ascii_downcase) | contains("ghostty")) | .address
        ' | while read -r addr; do
            [[ -n "$addr" ]] || continue
            hyprctl dispatch sendshortcut "CTRL SHIFT, comma, address:$addr" >/dev/null 2>&1 || true
        done
    fi

    orchis_suffix=""
    color_scheme_suffix="-light"
    if [[ "$theme" == "dark" ]]; then
        orchis_suffix="Dark"
        color_scheme_suffix="-dark"
    fi

    if command -v xfconf-query >/dev/null 2>&1; then
        xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
        xfconf-query -c xsettings -p /Net/IconThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
    fi

    if command -v gsettings >/dev/null 2>&1; then
        gsettings set org.gnome.desktop.interface gtk-theme "Orchis-${mode_label}-Compact" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface icon-theme "Fluent${color_scheme_suffix}" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface color-scheme "prefer${color_scheme_suffix}" >/dev/null 2>&1 || true
    fi

    if command -v kvantummanager >/dev/null 2>&1; then
        kvantummanager --set "Orchis${orchis_suffix}" >/dev/null 2>&1 || true
    fi

    if [[ -f "$background_target" ]] && command -v awww >/dev/null 2>&1; then
        if ! pgrep -x awww-daemon >/dev/null 2>&1; then
            nohup awww-daemon >/dev/null 2>&1 &
            sleep 0.2
        fi
        awww img "$background_target" --transition-fps 180 --transition-type wipe --transition-duration 2 >/dev/null 2>&1 || true
    fi

    if command -v kwriteconfig6 >/dev/null 2>&1; then
        kwriteconfig6 --file "$HOME/.config/konsolerc" --group "Desktop Entry" --key "DefaultProfile" "${mode_label}.profile" >/dev/null 2>&1 || true
    fi

    if command -v qdbus6 >/dev/null 2>&1; then
        for service in $(qdbus6 | awk '/org.kde.konsole/ { print $1 }'); do
            for session in $(qdbus6 "$service" | awk '/Sessions\// { print $1 }'); do
                qdbus6 "$service" "$session" org.kde.konsole.Session.setProfile "$mode_label" >/dev/null 2>&1 || true
            done
            for window in $(qdbus6 "$service" | awk '/Windows\// { print $1 }'); do
                qdbus6 "$service" "$window" org.kde.konsole.Window.setDefaultProfile "$mode_label" >/dev/null 2>&1 || true
            done
        done
    fi
}

apply_with_state() {
    load_current_state
    apply_links_and_theme
}

toggle_theme() {
    local new_theme
    new_theme="$("$state_script" toggle-theme)"
    notify_ui_state "switching to $(theme_label "$new_theme")"
    apply_with_state
}

set_variant() {
    local new_variant
    new_variant="${1:-}"
    "$state_script" set-variant "$new_variant"
    notify_ui_state "switching to $(variant_label "$new_variant")"
    apply_with_state
}

set_theme() {
    local new_theme
    new_theme="${1:-}"
    "$state_script" set-theme "$new_theme"
    notify_ui_state "switching to $(theme_label "$new_theme")"
    apply_with_state
}

case "${1:-apply}" in
    apply)
        apply_with_state
        ;;
    toggle-theme)
        toggle_theme
        ;;
    set-theme)
        set_theme "${2:-}"
        ;;
    set-variant)
        set_variant "${2:-}"
        ;;
    *)
        printf 'Usage: %s [apply|toggle-theme|set-theme <light|dark>|set-variant <v1|v2|v3|bt>]\n' "$0" >&2
        exit 1
        ;;
esac
