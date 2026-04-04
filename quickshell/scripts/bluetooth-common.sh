#!/usr/bin/env bash

: "${HYPRV_BLUETOOTH_TIMEOUT:=5}"

bt_trim() {
    local value="${1:-}"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s\n' "$value"
}

bt_property_value() {
    local text="${1:-}"
    local key="${2:-}"

    awk -F': ' -v key="$key" '
        $1 ~ ("^[[:space:]]*" key "$") {
            sub(/^[[:space:]]+/, "", $2);
            print $2;
            exit;
        }
    ' <<<"$text"
}

bt_property_bool() {
    local text="${1:-}"
    local key="${2:-}"
    local value

    value="$(bt_property_value "$text" "$key" | tr '[:upper:]' '[:lower:]')"
    if [[ "$value" == "yes" ]]; then
        printf 'true\n'
    else
        printf 'false\n'
    fi
}

bt_output_invalid() {
    local output="${1:-}"
    grep -qiE 'No default controller available|dbus_connection_get_object_path_data|D-Bus not built with -rdynamic' <<<"$output"
}

bt_run() {
    local output

    output="$({
        timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>/dev/null || true
    } 2>/dev/null)"

    if bt_output_invalid "$output"; then
        return 0
    fi

    printf '%s\n' "$output"
}

bt_run_capture() {
    local output

    output="$({
        timeout "${HYPRV_BLUETOOTH_TIMEOUT}s" bluetoothctl --timeout "$HYPRV_BLUETOOTH_TIMEOUT" "$@" 2>&1 || true
    } 2>/dev/null)"

    if bt_output_invalid "$output"; then
        return 0
    fi

    printf '%s\n' "$output"
}

bt_first_controller() {
    local output

    output="$(bt_run_capture list)"
    awk '/^Controller / { print $2; exit }' <<<"$output"
}

bt_show_capture() {
    local output controller

    output="$(bt_run_capture show)"
    if [[ -n "$output" ]]; then
        printf '%s\n' "$output"
        return 0
    fi

    controller="$(bt_first_controller)"
    if [[ -n "$controller" ]]; then
        bt_run_capture show "$controller"
    fi
}

bt_has_controller() {
    [[ -n "$(bt_show_capture)" ]]
}
