#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
state_script="$script_dir/ui-state.sh"

ensure_parent() {
    mkdir -p "$(dirname -- "$1")"
}

set_ini_value() {
    local file section key value tmp
    file="$1"
    section="$2"
    key="$3"
    value="$4"

    ensure_parent "$file"
    touch "$file"
    tmp="$(mktemp)"

    awk -v section="$section" -v key="$key" -v value="$value" '
        BEGIN {
            target = "[" section "]"
            in_target = 0
            written = 0
        }
        $0 == target {
            if (in_target && !written) {
                print key "=" value
                written = 1
            }
            in_target = 1
            print
            next
        }
        /^\[/ {
            if (in_target && !written) {
                print key "=" value
                written = 1
            }
            in_target = 0
            print
            next
        }
        {
            if (in_target && index($0, key "=") == 1) {
                if (!written) {
                    print key "=" value
                    written = 1
                }
                next
            }
            print
        }
        END {
            if (NR == 0) {
                print target
                print key "=" value
            } else if (in_target && !written) {
                print key "=" value
            } else if (!written) {
                print ""
                print target
                print key "=" value
            }
        }
    ' "$file" > "$tmp"

    mv "$tmp" "$file"
}

set_assignment() {
    local file key value tmp
    file="$1"
    key="$2"
    value="$3"

    ensure_parent "$file"
    touch "$file"
    tmp="$(mktemp)"

    awk -v key="$key" -v value="$value" '
        BEGIN {
            written = 0
        }
        index($0, key "=") == 1 {
            if (!written) {
                print key "=" value
                written = 1
            }
            next
        }
        {
            print
        }
        END {
            if (!written) {
                print key "=" value
            }
        }
    ' "$file" > "$tmp"

    mv "$tmp" "$file"
}

set_xsettings_value() {
    local file key value tmp
    file="$1"
    key="$2"
    value="$3"

    ensure_parent "$file"
    touch "$file"
    tmp="$(mktemp)"

    awk -v key="$key" -v value="$value" '
        BEGIN {
            written = 0
        }
        $1 == key {
            if (!written) {
                print key " " value
                written = 1
            }
            next
        }
        {
            print
        }
        END {
            if (!written) {
                print key " " value
            }
        }
    ' "$file" > "$tmp"

    mv "$tmp" "$file"
}

restart_xsettingsd() {
    if ! command -v xsettingsd >/dev/null 2>&1; then
        return 0
    fi

    pkill -x xsettingsd >/dev/null 2>&1 || true

    if [[ -n "${DISPLAY:-}" ]]; then
        nohup xsettingsd >/dev/null 2>&1 &
    fi
}

sync_activation_environment() {
    local gtk_theme_name
    gtk_theme_name="$1"

    if command -v dbus-update-activation-environment >/dev/null 2>&1; then
        dbus-update-activation-environment --systemd \
            "GTK_THEME=$gtk_theme_name" \
            "QT_QPA_PLATFORMTHEME=qt6ct" \
            "XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-Hyprland}" \
            "XDG_SESSION_DESKTOP=${XDG_SESSION_DESKTOP:-Hyprland}" >/dev/null 2>&1 || true
    fi
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

run_with_timeout() {
    local seconds
    seconds="$1"
    shift

    if command -v timeout >/dev/null 2>&1; then
        timeout "$seconds" "$@" >/dev/null 2>&1 || true
    else
        "$@" >/dev/null 2>&1 || true
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
    local orchis_suffix color_scheme_suffix prefer_dark
    local gtk_theme_name icon_theme_name kvantum_theme
    local kde_color_scheme kde_look_and_feel
    local qt6_color_scheme qt5_color_scheme
    local hypr_clients_json

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
            run_with_timeout 2 swaync-client -rs
        fi
    fi

    if command -v hyprctl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
        hypr_clients_json="$(hyprctl clients -j 2>/dev/null || true)"
        if [[ -n "$hypr_clients_json" ]] && printf '%s' "$hypr_clients_json" | jq -e . >/dev/null 2>&1; then
            printf '%s' "$hypr_clients_json" | jq -r '
                .[] | select((.class // "" | ascii_downcase) | contains("ghostty")) | .address
            ' | while read -r addr; do
                [[ -n "$addr" ]] || continue
                hyprctl dispatch sendshortcut "CTRL SHIFT, comma, address:$addr" >/dev/null 2>&1 || true
            done
        fi
    fi

    orchis_suffix=""
    color_scheme_suffix="-light"
    prefer_dark="false"
    qt6_color_scheme="/usr/share/qt6ct/colors/simple.conf"
    qt5_color_scheme="/usr/share/qt5ct/colors/simple.conf"
    kde_color_scheme="Orchis"
    kde_look_and_feel="com.github.vinceliuice.Orchis"
    if [[ "$theme" == "dark" ]]; then
        orchis_suffix="Dark"
        color_scheme_suffix="-dark"
        prefer_dark="true"
        qt6_color_scheme="/usr/share/qt6ct/colors/darker.conf"
        qt5_color_scheme="/usr/share/qt5ct/colors/darker.conf"
        kde_color_scheme="OrchisDark"
        kde_look_and_feel="com.github.vinceliuice.Orchis-dark"
    fi

    gtk_theme_name="Orchis-${mode_label}-Compact"
    icon_theme_name="Fluent${color_scheme_suffix}"
    kvantum_theme="Orchis${orchis_suffix}"

    if command -v xfconf-query >/dev/null 2>&1; then
        xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
        xfconf-query -c xsettings -p /Net/IconThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
    fi

    if command -v gsettings >/dev/null 2>&1; then
        gsettings set org.gnome.desktop.interface gtk-theme "$gtk_theme_name" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface icon-theme "$icon_theme_name" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface color-scheme "prefer${color_scheme_suffix}" >/dev/null 2>&1 || true
    fi
    sync_activation_environment "$gtk_theme_name"

    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
    set_assignment "$HOME/.gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
    set_assignment "$HOME/.gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""

    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/ThemeName "\"$gtk_theme_name\""
    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/IconThemeName "\"$icon_theme_name\""
    restart_xsettingsd

    set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance style "kvantum"
    set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance icon_theme "$icon_theme_name"
    set_ini_value "$HOME/.config/qt6ct/qt6ct.conf" Appearance color_scheme_path "$qt6_color_scheme"
    set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance style "kvantum"
    set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance icon_theme "$icon_theme_name"
    set_ini_value "$HOME/.config/qt5ct/qt5ct.conf" Appearance color_scheme_path "$qt5_color_scheme"
    set_ini_value "$HOME/.config/Kvantum/kvantum.kvconfig" General theme "$kvantum_theme"

    if command -v plasma-apply-colorscheme >/dev/null 2>&1; then
        run_with_timeout 5 plasma-apply-colorscheme "$kde_color_scheme"
    fi

    if command -v kwriteconfig6 >/dev/null 2>&1; then
        kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key LookAndFeelPackage "$kde_look_and_feel" >/dev/null 2>&1 || true
        kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key DefaultLightLookAndFeel "com.github.vinceliuice.Orchis" >/dev/null 2>&1 || true
        kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key DefaultDarkLookAndFeel "com.github.vinceliuice.Orchis-dark" >/dev/null 2>&1 || true
        kwriteconfig6 --file "$HOME/.config/kdeglobals" --group Icons --key Theme "$icon_theme_name" >/dev/null 2>&1 || true
        kwriteconfig6 --file "$HOME/.config/kdeglobals" --group KDE --key widgetStyle "qt6ct-style" >/dev/null 2>&1 || true
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
