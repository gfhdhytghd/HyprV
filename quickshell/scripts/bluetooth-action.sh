#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/bluetooth-common.sh"

bt_fail() {
    printf '%s\n' "${1:-Bluetooth action failed}" >&2
    exit 1
}

bt_require_controller() {
    bt_has_controller || bt_fail "No Bluetooth controller found"
}

bt_require_address() {
    local address="${1:-}"
    [[ -n "$address" ]] || bt_fail "Missing device address"
}

bt_output_has_failure() {
    local output="${1:-}"
    grep -qiE 'No default controller available|Failed|not available|not ready|invalid|Missing|not connected|Operation not permitted' <<<"$output"
}

bt_exec() {
    local output

    output="$(timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>&1)" || {
        printf '%s\n' "$output"
        return 1
    }

    if bt_output_has_failure "$output"; then
        printf '%s\n' "$output"
        return 1
    fi

    printf '%s\n' "$output"
}

bt_exec_script() {
    local timeout_seconds="$1"
    shift

    local script_file output status=0
    script_file="$(mktemp)"
    printf '%s\n' "$@" > "$script_file"

    output="$(timeout "$((timeout_seconds + 2))s" bluetoothctl --timeout "$timeout_seconds" --init-script "$script_file" 2>&1)" || status=$?
    rm -f "$script_file"

    if (( status != 0 )) || bt_output_has_failure "$output"; then
        printf '%s\n' "$output"
        return 1
    fi

    printf '%s\n' "$output"
}

command="${1:-}"

case "$command" in
    toggle)
        bt_require_controller
        state="${2:-}"
        [[ "$state" == "on" || "$state" == "off" ]] || bt_fail 'toggle expects "on" or "off"'
        bt_exec power "$state" >/dev/null || bt_fail "Failed to change Bluetooth power state"
        if [[ "$state" == "on" ]]; then
            printf 'Bluetooth enabled\n'
        else
            printf 'Bluetooth disabled\n'
        fi
        ;;
    scan)
        bt_require_controller
        if [[ "$(bt_property_bool "$(bt_show_capture)" "Powered")" != "true" ]]; then
            bt_fail "Bluetooth is turned off"
        fi
        bt_exec scan on >/dev/null || bt_fail "Failed to start Bluetooth scan"
        sleep 4
        bt_exec scan off >/dev/null || true
        printf 'Bluetooth scan complete\n'
        ;;
    connect)
        bt_require_controller
        address="${2:-}"
        paired="${3:-false}"
        bt_require_address "$address"

        if [[ "$paired" == "true" ]]; then
            bt_exec connect "$address" >/dev/null || bt_fail "Failed to connect device"
            printf 'Connected to %s\n' "$address"
        else
            bt_exec_script 35 \
                "agent KeyboardDisplay" \
                "default-agent" \
                "pair $address" \
                "trust $address" \
                "connect $address" >/dev/null || bt_fail "Failed to pair device"
            printf 'Paired and connected %s\n' "$address"
        fi
        ;;
    disconnect)
        bt_require_controller
        address="${2:-}"
        bt_require_address "$address"
        bt_exec disconnect "$address" >/dev/null || bt_fail "Failed to disconnect device"
        printf 'Disconnected %s\n' "$address"
        ;;
    remove)
        bt_require_controller
        address="${2:-}"
        bt_require_address "$address"
        bt_exec remove "$address" >/dev/null || bt_fail "Failed to remove device"
        printf 'Removed %s\n' "$address"
        ;;
    *)
        bt_fail "Unknown Bluetooth command"
        ;;
esac
