#!/usr/bin/env bash

set -euo pipefail

STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell"
STATE_FILE="$STATE_DIR/ui-state.env"

normalize_theme() {
    case "${1:-}" in
        light|dark)
            printf '%s\n' "$1"
            ;;
        *)
            return 1
            ;;
    esac
}

normalize_variant() {
    case "${1:-}" in
        v1|v2|v3|bt)
            printf '%s\n' "$1"
            ;;
        *)
            return 1
            ;;
    esac
}

legacy_theme() {
    local legacy_style
    legacy_style="$(readlink -f "$HOME/.config/waybar/style.css" 2>/dev/null || true)"
    if [[ "$legacy_style" == *"-dark.css" ]]; then
        printf 'dark\n'
    else
        printf 'light\n'
    fi
}

legacy_variant() {
    local legacy_config legacy_style candidate
    legacy_config="$(readlink -f "$HOME/.config/waybar/config.jsonc" 2>/dev/null || true)"
    legacy_style="$(readlink -f "$HOME/.config/waybar/style.css" 2>/dev/null || true)"

    for candidate in bt v1 v2 v3; do
        if [[ "$legacy_config" == *"/${candidate}-config.jsonc" ]] || [[ "$legacy_style" == *"/${candidate}-style"* ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    printf 'v2\n'
}

ensure_state() {
    mkdir -p "$STATE_DIR"
    if [[ ! -f "$STATE_FILE" ]]; then
        printf 'theme=%s\nvariant=%s\n' "$(legacy_theme)" "$(legacy_variant)" > "$STATE_FILE"
    fi
}

load_state() {
    local key value

    ensure_state

    theme="light"
    variant="v2"

    while IFS='=' read -r key value; do
        case "$key" in
            theme)
                theme="$value"
                ;;
            variant)
                variant="$value"
                ;;
        esac
    done < "$STATE_FILE"

    if ! theme="$(normalize_theme "$theme" 2>/dev/null)"; then
        theme="$(legacy_theme)"
    fi
    if ! variant="$(normalize_variant "$variant" 2>/dev/null)"; then
        variant="$(legacy_variant)"
    fi
}

write_state() {
    mkdir -p "$STATE_DIR"
    printf 'theme=%s\nvariant=%s\n' "$theme" "$variant" > "$STATE_FILE"
}

set_theme() {
    load_state
    theme="$(normalize_theme "${1:-}")"
    write_state
}

set_variant() {
    load_state
    variant="$(normalize_variant "${1:-}")"
    write_state
}

toggle_theme() {
    load_state
    if [[ "$theme" == "dark" ]]; then
        theme="light"
    else
        theme="dark"
    fi
    write_state
    printf '%s\n' "$theme"
}

print_state() {
    load_state
    printf 'theme=%s\nvariant=%s\n' "$theme" "$variant"
}

case "${1:-print}" in
    ensure)
        ensure_state
        ;;
    print)
        print_state
        ;;
    get-theme)
        load_state
        printf '%s\n' "$theme"
        ;;
    get-variant)
        load_state
        printf '%s\n' "$variant"
        ;;
    set-theme)
        set_theme "${2:-}"
        ;;
    set-variant)
        set_variant "${2:-}"
        ;;
    toggle-theme)
        toggle_theme
        ;;
    *)
        printf 'Usage: %s [ensure|print|get-theme|get-variant|set-theme <light|dark>|set-variant <v1|v2|v3|bt>|toggle-theme]\n' "$0" >&2
        exit 1
        ;;
esac
