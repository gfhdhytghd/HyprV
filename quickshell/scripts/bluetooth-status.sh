#!/usr/bin/env bash

set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/bluetooth-common.sh"

show_output="$(bt_show_capture)"

present=false
enabled=false
discovering=false
pairable=false

tmp_seed="$(mktemp)"
tmp_devices="$(mktemp)"

cleanup() {
    rm -f "$tmp_seed" "$tmp_devices"
}

trap cleanup EXIT INT TERM

if [[ -n "$show_output" ]]; then
    present=true
    [[ "$(bt_property_bool "$show_output" "Powered")" == "true" ]] && enabled=true
    [[ "$(bt_property_bool "$show_output" "Discovering")" == "true" ]] && discovering=true
    [[ "$(bt_property_bool "$show_output" "Pairable")" == "true" ]] && pairable=true

    {
        bt_run devices
        bt_run devices Paired
        bt_run devices Connected
    } | awk '
        /^Device / {
            addr = $2;
            $1 = "";
            $2 = "";
            sub(/^  */, "", $0);
            print addr "\t" $0;
        }
    ' | awk -F'\t' '!seen[$1]++ { print }' > "$tmp_seed"

    while IFS=$'\t' read -r address label; do
        [[ -n "$address" ]] || continue

        info="$(bt_run_capture info "$address")"
        name="$(bt_property_value "$info" "Alias")"
        [[ -n "$name" ]] || name="$(bt_trim "$label")"
        [[ -n "$name" ]] || name="$(bt_property_value "$info" "Name")"
        [[ -n "$name" ]] || name="$address"

        icon="$(bt_property_value "$info" "Icon")"
        [[ -n "$icon" ]] || icon="bluetooth"

        paired="$(bt_property_bool "$info" "Paired")"
        trusted="$(bt_property_bool "$info" "Trusted")"
        connected="$(bt_property_bool "$info" "Connected")"
        blocked="$(bt_property_bool "$info" "Blocked")"
        rssi="$(bt_property_value "$info" "RSSI")"

        if [[ ! "$rssi" =~ ^-?[0-9]+$ ]]; then
            rssi=""
        fi

        name="${name//$'\t'/ }"
        icon="${icon//$'\t'/ }"

        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$address" "$name" "$icon" "$paired" "$trusted" "$connected" "$blocked" "$rssi" >> "$tmp_devices"
    done < "$tmp_seed"
fi

jq -Rn \
    --arg present "$present" \
    --arg enabled "$enabled" \
    --arg discovering "$discovering" \
    --arg pairable "$pairable" \
    --rawfile devices "$tmp_devices" '
    def bool($value):
        $value == "true";

    def device_rows:
        $devices
        | split("\n")
        | map(select(length > 0) | split("\t"))
        | map({
            address: .[0],
            name: .[1],
            icon: .[2],
            paired: (.[3] == "true"),
            trusted: (.[4] == "true"),
            connected: (.[5] == "true"),
            blocked: (.[6] == "true"),
            rssi: (if (.[7] // "") | length > 0 then (.[7] | tonumber) else null end)
        })
        | sort_by([
            if .connected then 0 else 1 end,
            if .paired then 0 else 1 end,
            if .trusted then 0 else 1 end,
            -(.rssi // -999),
            (.name | ascii_downcase)
        ]);

    {
        present: bool($present),
        enabled: bool($enabled),
        discovering: bool($discovering),
        pairable: bool($pairable),
        devices: device_rows
    }
'
