#!/bin/sh

: "${HYPRV_NMCLI_BROKEN_MARKER:=/tmp/hyprv-nmcli-broken}"
: "${HYPRV_NMCLI_RETRY_SECONDS:=20}"

mark_nmcli_failed() {
    failed_at="$(date +%s 2>/dev/null || printf '0')"
    printf '%s\n' "$failed_at" > "$HYPRV_NMCLI_BROKEN_MARKER" 2>/dev/null || :
}

clear_nmcli_failed() {
    rm -f "$HYPRV_NMCLI_BROKEN_MARKER"
}

nmcli_allowed() {
    [ ! -e "$HYPRV_NMCLI_BROKEN_MARKER" ] && return 0

    failed_at="$(cat "$HYPRV_NMCLI_BROKEN_MARKER" 2>/dev/null || true)"
    case "$failed_at" in
        ''|*[!0-9]*)
            clear_nmcli_failed
            return 0
            ;;
    esac

    now="$(date +%s 2>/dev/null || printf '0')"
    case "$now" in
        ''|*[!0-9]*)
            return 1
            ;;
    esac

    if [ $((now - failed_at)) -ge "$HYPRV_NMCLI_RETRY_SECONDS" ]; then
        clear_nmcli_failed
        return 0
    fi

    return 1
}

run_nmcli() {
    if nmcli "$@"; then
        clear_nmcli_failed
        return 0
    fi

    mark_nmcli_failed
    return 1
}

detect_wifi_iface_nmcli() {
    nmcli_allowed || return 1
    run_nmcli -t -f DEVICE,TYPE dev status 2>/dev/null | awk -F: '$2 == "wifi" { print $1; exit }'
}

detect_wifi_iface_sysfs() {
    for path in /sys/class/net/*; do
        [ -d "$path/wireless" ] || continue
        basename "$path"
        return 0
    done
    return 1
}

detect_wifi_iface() {
    iface="$(detect_wifi_iface_nmcli || true)"
    if [ -n "$iface" ]; then
        printf '%s\n' "$iface"
        return 0
    fi

    iface="$(detect_wifi_iface_sysfs || true)"
    if [ -n "$iface" ]; then
        printf '%s\n' "$iface"
        return 0
    fi

    return 1
}

wifi_radio_status() {
    if nmcli_allowed; then
        run_nmcli -t -f WIFI,WIFI-HW general status 2>/dev/null || printf 'enabled:enabled'
    else
        printf 'enabled:enabled'
    fi
}

saved_wifi_ssids() {
    nmcli_allowed || return 0
    run_nmcli -t -f UUID,TYPE connection show 2>/dev/null \
        | awk -F: '$2 == "802-11-wireless" { print $1 }' \
        | while IFS= read -r uuid; do
            run_nmcli -g 802-11-wireless.ssid connection show "$uuid" 2>/dev/null || true
        done \
        | sed '/^$/d' \
        | sort -u
}

require_wifi_iface() {
    iface="${1:-}"
    if [ -n "$iface" ]; then
        return 0
    fi

    printf 'No Wi-Fi device found\n' >&2
    exit 1
}
