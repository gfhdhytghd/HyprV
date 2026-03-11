#!/bin/sh

detect_wifi_iface_nmcli() {
    nmcli -t -f DEVICE,TYPE dev status 2>/dev/null | awk -F: '$2 == "wifi" { print $1; exit }'
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
    nmcli -t -f WIFI,WIFI-HW general status 2>/dev/null || printf 'enabled:enabled'
}

saved_wifi_ssids() {
    nmcli -t -f UUID,TYPE connection show 2>/dev/null \
        | awk -F: '$2 == "802-11-wireless" { print $1 }' \
        | while IFS= read -r uuid; do
            nmcli -g 802-11-wireless.ssid connection show "$uuid" 2>/dev/null || true
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
