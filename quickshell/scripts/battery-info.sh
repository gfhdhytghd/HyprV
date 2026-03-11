#!/bin/sh

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/../../scripts/lib/battery.sh"

window_seconds=1800

battery_dir="$(hyprv_find_battery_dir || true)"
if [ -z "$battery_dir" ]; then
    jq -n '{available: false}'
    exit 0
fi

state_dir="$(hyprv_pick_state_dir || printf '/tmp/hyprv')"
mkdir -p "$state_dir"
state_file="$state_dir/battery-power-history.tsv"

present="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" present || printf '1')")"
status="$(hyprv_read_first_existing "$battery_dir" status || printf 'Unknown')"
capacity="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" capacity || printf '0')")"
voltage_now="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" voltage_now || printf '0')")"
charge_now="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" charge_now || printf '0')")"
charge_full="$(hyprv_safe_int "$(hyprv_read_first_existing "$battery_dir" charge_full || printf '0')")"
energy_now_raw="$(hyprv_read_first_existing "$battery_dir" energy_now || true)"
energy_full_raw="$(hyprv_read_first_existing "$battery_dir" energy_full || true)"
power_now_raw="$(hyprv_read_first_existing "$battery_dir" power_now || true)"
current_now_raw="$(hyprv_read_first_existing "$battery_dir" current_now || true)"

if [ -n "$energy_now_raw" ]; then
    energy_now_uwh="$(hyprv_safe_int "$energy_now_raw")"
elif [ "$charge_now" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
    energy_now_uwh="$(hyprv_mul_div_round "$charge_now" "$voltage_now" 1000000)"
else
    energy_now_uwh=0
fi

if [ -n "$energy_full_raw" ]; then
    energy_full_uwh="$(hyprv_safe_int "$energy_full_raw")"
elif [ "$charge_full" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
    energy_full_uwh="$(hyprv_mul_div_round "$charge_full" "$voltage_now" 1000000)"
else
    energy_full_uwh=0
fi

if [ -n "$power_now_raw" ]; then
    power_now_uw="$(hyprv_abs_int "$power_now_raw")"
else
    current_now_abs="$(hyprv_abs_int "$current_now_raw")"
    if [ "$current_now_abs" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
        power_now_uw="$(hyprv_mul_div_round "$current_now_abs" "$voltage_now" 1000000)"
    else
        power_now_uw=0
    fi
fi

mode="idle"
case "$status" in
    Charging|Pending\ charge)
        mode="charging"
        ;;
    Discharging|Pending\ discharge)
        mode="discharging"
        ;;
    Full)
        mode="full"
        ;;
    Not\ charging)
        mode="plugged"
        ;;
esac

if [ "$mode" = "plugged" ] && [ "$capacity" -ge 99 ]; then
    mode="full"
fi

signed_power_uw=0
case "$mode" in
    charging)
        signed_power_uw="$power_now_uw"
        ;;
    discharging)
        signed_power_uw="-$(hyprv_abs_int "$power_now_uw")"
        ;;
esac

tmp_file="$(mktemp "$state_dir/battery-history.XXXXXX")"
cleanup() {
    rm -f "$tmp_file"
}
trap cleanup EXIT INT TERM

now="$(date +%s)"
cutoff=$((now - window_seconds))

if [ -f "$state_file" ]; then
    awk -v cutoff="$cutoff" 'BEGIN {
        FS = "\t";
        OFS = "\t";
    }
    NF >= 2 && $1 ~ /^[0-9]+$/ && $2 ~ /^-?[0-9]+$/ && ($1 + 0) >= cutoff {
        print $1, $2;
    }' "$state_file" > "$tmp_file"
else
    : > "$tmp_file"
fi

printf '%s\t%s\n' "$now" "$signed_power_uw" >> "$tmp_file"
mv "$tmp_file" "$state_file"

avg_fields="$(awk -v mode="$mode" -F'\t' '
function include_value(value) {
    if (mode == "charging") {
        return value > 0;
    }
    if (mode == "discharging") {
        return value < 0;
    }
    return 1;
}

NF >= 2 {
    timestamp = $1 + 0;
    value = $2 + 0;
    if (!include_value(value)) {
        next;
    }
    if (count == 0) {
        first_timestamp = timestamp;
    }
    sum += value;
    count += 1;
}

END {
    if (count == 0) {
        print "0\t0\t0";
        exit;
    }
    printf "%d\t%.0f\t%d\n", first_timestamp, sum / count, count;
}' "$state_file")"

old_ifs=$IFS
IFS='	'
set -- $avg_fields
IFS=$old_ifs
avg_first_timestamp="${1:-0}"
avg_power_uw="${2:-0}"
sample_count="${3:-0}"

sample_window_seconds=0
if [ "${avg_first_timestamp:-0}" -gt 0 ]; then
    sample_window_seconds=$((now - avg_first_timestamp))
fi

avg_power_abs_uw="$(hyprv_abs_int "$avg_power_uw")"
estimate_seconds_json="null"
estimate_basis="none"
estimate_power_uw=0

case "$mode" in
    charging)
        remaining_energy_uwh=$((energy_full_uwh - energy_now_uwh))
        if [ "$remaining_energy_uwh" -lt 0 ]; then
            remaining_energy_uwh=0
        fi
        if [ "$avg_power_abs_uw" -gt 0 ]; then
            estimate_power_uw="$avg_power_abs_uw"
            estimate_basis="average"
        elif [ "$power_now_uw" -gt 0 ]; then
            estimate_power_uw="$power_now_uw"
            estimate_basis="current"
        fi
        if [ "$remaining_energy_uwh" -le 0 ]; then
            estimate_seconds_json="0"
            estimate_basis="none"
        elif [ "$estimate_power_uw" -gt 0 ]; then
            estimate_seconds_json="$(awk -v e="$remaining_energy_uwh" -v p="$estimate_power_uw" 'BEGIN {
                printf "%.0f", (e + 0) / (p + 0) * 3600;
            }')"
        fi
        ;;
    discharging)
        remaining_energy_uwh="$energy_now_uwh"
        if [ "$avg_power_abs_uw" -gt 0 ]; then
            estimate_power_uw="$avg_power_abs_uw"
            estimate_basis="average"
        elif [ "$power_now_uw" -gt 0 ]; then
            estimate_power_uw="$power_now_uw"
            estimate_basis="current"
        fi
        if [ "$remaining_energy_uwh" -gt 0 ] && [ "$estimate_power_uw" -gt 0 ]; then
            estimate_seconds_json="$(awk -v e="$remaining_energy_uwh" -v p="$estimate_power_uw" 'BEGIN {
                printf "%.0f", (e + 0) / (p + 0) * 3600;
            }')"
        fi
        ;;
    full)
        estimate_seconds_json="0"
        ;;
esac

available_json=false
if [ "$present" -gt 0 ]; then
    available_json=true
fi

jq -n \
    --arg status "$status" \
    --arg mode "$mode" \
    --arg estimateBasis "$estimate_basis" \
    --argjson available "$available_json" \
    --argjson capacity "$capacity" \
    --argjson powerW "$(hyprv_micro_to_decimal_units "$signed_power_uw")" \
    --argjson averagePowerW "$(hyprv_micro_to_decimal_units "$avg_power_uw")" \
    --argjson sampleCount "$sample_count" \
    --argjson sampleWindowSeconds "$sample_window_seconds" \
    --argjson windowComplete "$( [ "$sample_window_seconds" -ge "$window_seconds" ] && printf 'true' || printf 'false' )" \
    --argjson estimateSeconds "$estimate_seconds_json" \
    --argjson energyNowWh "$(hyprv_micro_to_decimal_units "$energy_now_uwh")" \
    --argjson energyFullWh "$(hyprv_micro_to_decimal_units "$energy_full_uwh")" \
    '{
        available: $available,
        status: $status,
        mode: $mode,
        capacity: $capacity,
        powerW: $powerW,
        averagePowerW: $averagePowerW,
        sampleCount: $sampleCount,
        sampleWindowSeconds: $sampleWindowSeconds,
        windowComplete: $windowComplete,
        estimateSeconds: $estimateSeconds,
        estimateBasis: $estimateBasis,
        energyNowWh: $energyNowWh,
        energyFullWh: $energyFullWh
    }'
