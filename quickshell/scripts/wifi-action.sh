#!/bin/sh

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/wifi-common.sh"

iface="$(detect_wifi_iface || true)"
command="${1:-}"

run_nmcli_connect() {
    ssid="$1"
    password="$2"

    if [ -n "$password" ]; then
        run_nmcli dev wifi connect "$ssid" password "$password" ifname "$iface" >/dev/null
    else
        run_nmcli dev wifi connect "$ssid" ifname "$iface" >/dev/null
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
        run_nmcli radio wifi "$state" >/dev/null
        if [ "$state" = "on" ]; then
            printf 'Wi-Fi enabled\n'
        else
            printf 'Wi-Fi disabled\n'
        fi
        ;;
    rescan)
        if [ -n "$iface" ]; then
            run_nmcli dev wifi rescan ifname "$iface" >/dev/null
        else
            run_nmcli dev wifi rescan >/dev/null
        fi
        printf 'Scan started\n'
        ;;
    disconnect)
        require_wifi_iface "$iface"
        run_nmcli device disconnect "$iface" >/dev/null
        printf 'Disconnected\n'
        ;;
    connect)
        require_wifi_iface "$iface"

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
