#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
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
    local gtk_theme_name cursor_theme_name cursor_size
    gtk_theme_name="$1"
    cursor_theme_name="$2"
    cursor_size="$3"

    if command -v dbus-update-activation-environment >/dev/null 2>&1; then
        dbus-update-activation-environment --systemd \
            "GTK_THEME=$gtk_theme_name" \
            "QT_QPA_PLATFORMTHEME=qt6ct" \
            "XCURSOR_THEME=$cursor_theme_name" \
            "XCURSOR_SIZE=$cursor_size" \
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

send_ghostty_reload_shortcut() {
    local addr
    addr="$1"

    [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 0

    if command -v hyprctl >/dev/null 2>&1; then
        hyprctl eval "hl.dispatch(hl.dsp.send_shortcut({ mods = 'CTRL SHIFT', key = 'comma', window = 'address:$addr' }))" >/dev/null 2>&1 \
            || hyprctl dispatch sendshortcut "CTRL SHIFT, comma, address:$addr" >/dev/null 2>&1 \
            || true
    fi
}

lua_quote() {
    [[ ${1:-} != *"'"* ]] || return 1
    printf "'%s'" "$1"
}

hypr_move_window_hidden() {
    local addr workspace window_q workspace_q
    addr="$1"
    workspace="$2"

    [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 1
    command -v hyprctl >/dev/null 2>&1 || return 1

    window_q="$(lua_quote "address:$addr")" || return 1
    workspace_q="$(lua_quote "$workspace")" || return 1

    hyprctl dispatch "hl.dsp.window.move({ workspace = ${workspace_q}, window = ${window_q}, follow = false })" >/dev/null 2>&1 \
        || hyprctl dispatch movetoworkspacesilent "${workspace},address:${addr}" >/dev/null 2>&1
}

hypr_focus_window_addr() {
    local addr window_q
    addr="$1"

    [[ "$addr" =~ ^0x[0-9A-Fa-f]+$ ]] || return 1
    command -v hyprctl >/dev/null 2>&1 || return 1

    window_q="$(lua_quote "address:$addr")" || return 1
    hyprctl dispatch "hl.dsp.focus({ window = ${window_q} })" >/dev/null 2>&1 \
        || hyprctl dispatch focuswindow "address:$addr" >/dev/null 2>&1
}

wechat_hypr_client_address_by_title() {
    local title
    title="$1"

    hyprctl clients -j 2>/dev/null | jq -r --arg title "$title" '
        .[]
        | select((.class // "") == "wechat" and (.title // "") == $title)
        | .address
    ' | tail -n 1
}

wechat_popup_hypr_address() {
    hyprctl clients -j 2>/dev/null | jq -r '
        .[]
        | select(
            (.class // "") == "wechat"
            and (.title // "") == "wechat"
            and (.floating == true)
            and (.size[0] >= 140 and .size[0] <= 180)
            and (.size[1] >= 90 and .size[1] <= 130)
        )
        | .address
    ' | tail -n 1
}

wechat_xwindow_by_title() {
    local title
    title="$1"

    xdotool search --class wechat 2>/dev/null | while read -r wid; do
        [[ "$(xdotool getwindowname "$wid" 2>/dev/null || true)" == "$title" ]] || continue
        printf '%s\n' "$wid"
    done | tail -n 1
}

wechat_popup_xwindow() {
    local name width height

    xdotool search --class wechat 2>/dev/null | while read -r wid; do
        name="$(xdotool getwindowname "$wid" 2>/dev/null || true)"
        [[ "$name" == "wechat" ]] || continue
        width="$(xwininfo -id "$wid" 2>/dev/null | awk '/Width:/ { print $2; exit }')"
        height="$(xwininfo -id "$wid" 2>/dev/null | awk '/Height:/ { print $2; exit }')"
        [[ "$width" =~ ^[0-9]+$ && "$height" =~ ^[0-9]+$ ]] || continue
        if (( width >= 250 && width <= 420 && height >= 150 && height <= 300 )); then
            printf '%s\n' "$wid"
        fi
    done | tail -n 1
}

wait_for_wechat_settings() {
    local i addr

    for i in {1..30}; do
        addr="$(wechat_hypr_client_address_by_title "设置")"
        if [[ -n "$addr" ]]; then
            printf '%s\n' "$addr"
            return 0
        fi
        sleep 0.1
    done

    return 1
}

wait_for_wechat_popup() {
    local i addr

    for i in {1..20}; do
        addr="$(wechat_popup_hypr_address)"
        if [[ -n "$addr" ]]; then
            printf '%s\n' "$addr"
            return 0
        fi
        sleep 0.05
    done

    return 1
}

sync_wechat_appearance() {
    local target_theme hidden_workspace active_addr main_wid settings_wid settings_addr popup_addr popup_wid menu_y

    target_theme="${1:-}"
    [[ "$target_theme" == "light" || "$target_theme" == "dark" ]] || return 0

    command -v xdotool >/dev/null 2>&1 || return 0
    command -v xwininfo >/dev/null 2>&1 || return 0
    command -v hyprctl >/dev/null 2>&1 || return 0
    command -v jq >/dev/null 2>&1 || return 0

    main_wid="$(wechat_xwindow_by_title "微信")"
    [[ -n "$main_wid" ]] || return 0

    hidden_workspace="special:wechat-ui"
    active_addr="$(hyprctl activewindow -j 2>/dev/null | jq -r '.address // empty' || true)"

    settings_wid="$(wechat_xwindow_by_title "设置")"
    settings_addr="$(wechat_hypr_client_address_by_title "设置")"
    if [[ -z "$settings_wid" || -z "$settings_addr" ]]; then
        xdotool key --window "$main_wid" ctrl+comma >/dev/null 2>&1 || return 0
        settings_addr="$(wait_for_wechat_settings || true)"
        settings_wid="$(wechat_xwindow_by_title "设置")"
    fi

    [[ -n "$settings_wid" && -n "$settings_addr" ]] || return 0

    hypr_move_window_hidden "$settings_addr" "$hidden_workspace" || true
    if [[ -n "$active_addr" ]]; then
        hypr_focus_window_addr "$active_addr" || true
    fi

    # Coordinates are XWayland window pixels from the current WeChat settings UI.
    xdotool mousemove --window "$settings_wid" 120 225 click 1 >/dev/null 2>&1 || true
    sleep 0.2
    xdotool mousemove --window "$settings_wid" 910 490 click 1 >/dev/null 2>&1 || true

    popup_addr="$(wait_for_wechat_popup || true)"
    if [[ -z "$popup_addr" ]]; then
        cleanup_wechat_settings_window "$active_addr"
        return 0
    fi

    if [[ -n "$popup_addr" ]]; then
        hypr_move_window_hidden "$popup_addr" "$hidden_workspace" || true
    fi
    if [[ -n "$active_addr" ]]; then
        hypr_focus_window_addr "$active_addr" || true
    fi

    sleep 0.1
    popup_wid="$(wechat_popup_xwindow)"
    if [[ -z "$popup_wid" ]]; then
        cleanup_wechat_settings_window "$active_addr"
        return 0
    fi

    if [[ "$target_theme" == "dark" ]]; then
        menu_y=150
    else
        menu_y=80
    fi

    xdotool mousemove --window "$popup_wid" 150 "$menu_y" click 1 >/dev/null 2>&1 || true
    sleep 0.3

    # Close the hidden settings window so repeated theme applications start cleanly.
    cleanup_wechat_settings_window "$active_addr"
}

cleanup_wechat_settings_window() {
    local active_addr settings_wid
    active_addr="${1:-}"

    settings_wid="$(wechat_xwindow_by_title "设置")"
    if [[ -n "$settings_wid" ]]; then
        xdotool windowclose "$settings_wid" >/dev/null 2>&1 || true
    fi
    if [[ -n "$active_addr" ]]; then
        hypr_focus_window_addr "$active_addr" || true
    fi
}

load_current_state() {
    theme="$("$state_script" get-theme)"
    variant="$("$state_script" get-variant)"
    suffix="$(theme_suffix "$theme")"
    mode_label="$(theme_label "$theme")"
    variant_name="$(variant_prefix "$variant")"
}

reload_kitty_theme() {
    # Live-apply colors via remote control sockets, then SIGUSR1 for full reload.
    local theme_file sock
    theme_file="$1"

    if command -v kitty >/dev/null 2>&1 && [[ -f "$theme_file" ]]; then
        shopt -s nullglob
        # New sockets live in XDG_RUNTIME_DIR; keep /tmp for instances started before the move
        for sock in "${XDG_RUNTIME_DIR:-/tmp}"/kitty-*.sock "${TEMP:-/tmp}"/kitty-*.sock; do
            run_with_timeout 2 kitty @ --to "unix:${sock}" set-colors --all --configured "$theme_file" >/dev/null 2>&1 || true
            run_with_timeout 2 kitty @ --to "unix:${sock}" load-config >/dev/null 2>&1 || true
        done
        shopt -u nullglob
    fi

    if command -v pkill >/dev/null 2>&1; then
        pkill -USR1 -x kitty >/dev/null 2>&1 || true
    fi
}

apply_links_and_theme() {
    local wofi_target background_target rofi_target alacritty_target ghostty_target kitty_target kitty_conf_target swaync_target
    local orchis_suffix color_scheme_suffix prefer_dark
    local gtk_theme_name icon_theme_name kvantum_theme
    local cursor_theme_name cursor_size
    local kde_color_scheme kde_look_and_feel
    local qt6_color_scheme qt5_color_scheme
    local hypr_clients_json

    wofi_target="$HOME/.config/HyprV/wofi/style/${variant_name}-style${suffix}.css"
    background_target="$HOME/.config/HyprV/backgrounds/${variant_name}-background${suffix}.jpg"
    rofi_target="$HOME/.config/HyprV/rofi/colors${suffix}.rasi"
    alacritty_target="$HOME/.config/HyprV/alacritty/alacritty${suffix}.toml"
    ghostty_target="$HOME/.config/HyprV/ghostty/ghostty${suffix}.toml"
    kitty_target="$HOME/.config/HyprV/kitty/theme${suffix}.conf"
    kitty_conf_target="$HOME/.config/HyprV/kitty/kitty.conf"
    swaync_target="$HOME/.config/HyprV/swaync/style${suffix}.css"

    ensure_parent "$HOME/.config/wofi/style.css"
    ensure_parent "$HOME/.config/rofi/colors.rasi"
    ensure_parent "$HOME/.config/alacritty/alacritty.toml"
    ensure_parent "$HOME/.config/ghostty/config"
    ensure_parent "$HOME/.config/kitty/kitty.conf"
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

    if [[ -f "$kitty_conf_target" ]]; then
        ln -sfn "$kitty_conf_target" "$HOME/.config/kitty/kitty.conf"
    fi

    if [[ -f "$kitty_target" ]]; then
        ln -sfn "$kitty_target" "$HOME/.config/kitty/current-theme.conf"
        reload_kitty_theme "$kitty_target"
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
                .[]
                | select(
                    ((.class // "") | ascii_downcase | contains("ghostty"))
                    or ((.initialClass // "") | ascii_downcase | contains("ghostty"))
                )
                | .address
            ' | while read -r addr; do
                [[ -n "$addr" ]] || continue
                send_ghostty_reload_shortcut "$addr"
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
    cursor_theme_name="Adwaita"
    cursor_size="24"

    if command -v xfconf-query >/dev/null 2>&1; then
        xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
        xfconf-query -c xsettings -p /Net/IconThemeName -s "Adwaita${suffix}" >/dev/null 2>&1 || true
        xfconf-query -c xsettings -p /Gtk/CursorThemeName -s "$cursor_theme_name" >/dev/null 2>&1 || true
        xfconf-query -c xsettings -p /Gtk/CursorThemeSize -s "$cursor_size" >/dev/null 2>&1 || true
    fi

    if command -v gsettings >/dev/null 2>&1; then
        gsettings set org.gnome.desktop.interface gtk-theme "$gtk_theme_name" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface icon-theme "$icon_theme_name" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface cursor-theme "$cursor_theme_name" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface cursor-size "$cursor_size" >/dev/null 2>&1 || true
        gsettings set org.gnome.desktop.interface color-scheme "prefer${color_scheme_suffix}" >/dev/null 2>&1 || true
    fi
    sync_activation_environment "$gtk_theme_name" "$cursor_theme_name" "$cursor_size"
    if command -v hyprctl >/dev/null 2>&1; then
        hyprctl setcursor "$cursor_theme_name" "$cursor_size" >/dev/null 2>&1 || true
    fi

    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-cursor-theme-name "$cursor_theme_name"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-cursor-theme-size "$cursor_size"
    set_ini_value "$HOME/.config/gtk-3.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-theme-name "$gtk_theme_name"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-icon-theme-name "$icon_theme_name"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-cursor-theme-name "$cursor_theme_name"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-cursor-theme-size "$cursor_size"
    set_ini_value "$HOME/.config/gtk-4.0/settings.ini" Settings gtk-application-prefer-dark-theme "$prefer_dark"
    set_assignment "$HOME/.gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
    set_assignment "$HOME/.gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""
    set_assignment "$HOME/.gtkrc-2.0" gtk-cursor-theme-name "\"$cursor_theme_name\""
    set_assignment "$HOME/.gtkrc-2.0" gtk-cursor-theme-size "$cursor_size"
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-theme-name "\"$gtk_theme_name\""
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-icon-theme-name "\"$icon_theme_name\""
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-cursor-theme-name "\"$cursor_theme_name\""
    set_assignment "$HOME/.config/gtkrc-2.0" gtk-cursor-theme-size "$cursor_size"

    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/ThemeName "\"$gtk_theme_name\""
    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/IconThemeName "\"$icon_theme_name\""
    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/CursorThemeName "\"$cursor_theme_name\""
    set_xsettings_value "$HOME/.config/xsettingsd/xsettingsd.conf" Net/CursorThemeSize "$cursor_size"
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

    sync_wechat_appearance "$theme"
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
