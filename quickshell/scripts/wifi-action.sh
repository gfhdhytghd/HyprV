#!/bin/sh

set -eu

iface="$(nmcli -t -f DEVICE,TYPE dev status 2>/dev/null | awk -F: '$2 == "wifi" { print $1; exit }')"
command="${1:-}"

run_nmcli_connect() {
    ssid="$1"
    password="$2"
    output_file="$(mktemp)"
    error_file="$(mktemp)"

    cleanup_connect() {
        rm -f "$output_file" "$error_file"
    }

    trap cleanup_connect EXIT INT TERM

    if [ -n "$password" ]; then
        nmcli dev wifi connect "$ssid" password "$password" ifname "$iface" >"$output_file" 2>"$error_file"
    else
        nmcli dev wifi connect "$ssid" ifname "$iface" >"$output_file" 2>"$error_file"
    fi

    printf 'Connection requested for %s\n' "$ssid"
}

case "$command" in
    toggle)
        state="${2:-}"
        if [ "$state" != "on" ] && [ "$state" != "off" ]; then
            printf 'toggle expects "on" or "off"\n' >&2
            exit 2
        fi
        nmcli radio wifi "$state" >/dev/null
        if [ "$state" = "on" ]; then
            printf 'Wi-Fi enabled\n'
        else
            printf 'Wi-Fi disabled\n'
        fi
        ;;
    rescan)
        if [ -n "$iface" ]; then
            nmcli dev wifi rescan ifname "$iface" >/dev/null
        else
            nmcli dev wifi rescan >/dev/null
        fi
        printf 'Scan started\n'
        ;;
    disconnect)
        if [ -z "$iface" ]; then
            printf 'No Wi-Fi device found\n' >&2
            exit 1
        fi
        nmcli device disconnect "$iface" >/dev/null
        printf 'Disconnected\n'
        ;;
    connect)
        if [ -z "$iface" ]; then
            printf 'No Wi-Fi device found\n' >&2
            exit 1
        fi

        ssid="${2:-}"
        password="${3:-}"
        security="${4:-}"

        if [ -z "$ssid" ]; then
            printf 'Missing SSID\n' >&2
            exit 2
        fi

        if [ -n "$password" ]; then
            run_nmcli_connect "$ssid" "$password"
            exit 0
        fi

        if run_nmcli_connect "$ssid" ""; then
            exit 0
        fi

        if printf '%s' "$security" | grep -Eq '802\.1X|EAP'; then
            printf 'This network needs an 802.1X profile. Open the editor for first-time setup.\n' >&2
        else
            printf 'Failed to connect to %s\n' "$ssid" >&2
        fi
        exit 1
        ;;
    *)
        printf 'Unknown Wi-Fi command\n' >&2
        exit 2
        ;;
esac
