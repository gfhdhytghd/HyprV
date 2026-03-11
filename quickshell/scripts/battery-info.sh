#!/bin/sh

set -eu

window_seconds=1800

pick_state_dir() {
    for candidate in "${XDG_RUNTIME_DIR:-}" /tmp; do
        [ -n "$candidate" ] || continue
        [ -d "$candidate" ] || continue

        dir="$candidate/hyprv"
        if mkdir -p "$dir" 2>/dev/null; then
            probe="$dir/.battery-write-test.$$"
            if touch "$probe" 2>/dev/null; then
                rm -f "$probe"
                printf '%s\n' "$dir"
                return 0
            fi
        fi
    done

    return 1
}

find_battery_dir() {
    for path in /sys/class/power_supply/BAT*; do
        [ -d "$path" ] || continue
        printf '%s\n' "$path"
        return 0
    done
    return 1
}

read_first_existing() {
    dir="$1"
    shift

    for name in "$@"; do
        if [ -r "$dir/$name" ]; then
            tr -d '\n' < "$dir/$name"
            return 0
        fi
    done

    return 1
}

safe_int() {
    value="${1:-0}"
    case "$value" in
        ''|*[!0-9-]*)
            printf '0\n'
            ;;
        *)
            printf '%s\n' "$value"
            ;;
    esac
}

abs_int() {
    value="$(safe_int "${1:-0}")"
    case "$value" in
        -*)
            printf '%s\n' "${value#-}"
            ;;
        *)
            printf '%s\n' "$value"
            ;;
    esac
}

mul_div_round() {
    awk -v a="$1" -v b="$2" -v d="$3" 'BEGIN {
        if ((d + 0) == 0) {
            print 0;
            exit;
        }
        printf "%.0f\n", (a + 0) * (b + 0) / d;
    }'
}

to_decimal_units() {
    awk -v value="$1" 'BEGIN {
        printf "%.3f\n", (value + 0) / 1000000;
    }'
}

battery_dir="$(find_battery_dir || true)"
if [ -z "$battery_dir" ]; then
    jq -n '{available: false}'
    exit 0
fi

state_dir="$(pick_state_dir || printf '/tmp/hyprv')"
state_file="$state_dir/battery-power-history.tsv"

present="$(safe_int "$(read_first_existing "$battery_dir" present || printf '1')")"
status="$(read_first_existing "$battery_dir" status || printf 'Unknown')"
capacity="$(safe_int "$(read_first_existing "$battery_dir" capacity || printf '0')")"
voltage_now="$(safe_int "$(read_first_existing "$battery_dir" voltage_now || printf '0')")"
charge_now="$(safe_int "$(read_first_existing "$battery_dir" charge_now || printf '0')")"
charge_full="$(safe_int "$(read_first_existing "$battery_dir" charge_full || printf '0')")"
energy_now_raw="$(read_first_existing "$battery_dir" energy_now || true)"
energy_full_raw="$(read_first_existing "$battery_dir" energy_full || true)"
power_now_raw="$(read_first_existing "$battery_dir" power_now || true)"
current_now_raw="$(read_first_existing "$battery_dir" current_now || true)"

if [ -n "$energy_now_raw" ]; then
    energy_now_uwh="$(safe_int "$energy_now_raw")"
elif [ "$charge_now" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
    energy_now_uwh="$(mul_div_round "$charge_now" "$voltage_now" 1000000)"
else
    energy_now_uwh=0
fi

if [ -n "$energy_full_raw" ]; then
    energy_full_uwh="$(safe_int "$energy_full_raw")"
elif [ "$charge_full" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
    energy_full_uwh="$(mul_div_round "$charge_full" "$voltage_now" 1000000)"
else
    energy_full_uwh=0
fi

if [ -n "$power_now_raw" ]; then
    power_now_uw="$(abs_int "$power_now_raw")"
else
    current_now_abs="$(abs_int "$current_now_raw")"
    if [ "$current_now_abs" -gt 0 ] && [ "$voltage_now" -gt 0 ]; then
        power_now_uw="$(mul_div_round "$current_now_abs" "$voltage_now" 1000000)"
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
        signed_power_uw="-$(abs_int "$power_now_uw")"
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

avg_first_timestamp="$(printf '%s\n' "$avg_fields" | awk -F'\t' 'NR == 1 { print $1 }')"
avg_power_uw="$(printf '%s\n' "$avg_fields" | awk -F'\t' 'NR == 1 { print $2 }')"
sample_count="$(printf '%s\n' "$avg_fields" | awk -F'\t' 'NR == 1 { print $3 }')"

sample_window_seconds=0
if [ "${avg_first_timestamp:-0}" -gt 0 ]; then
    sample_window_seconds=$((now - avg_first_timestamp))
fi

avg_power_abs_uw="$(abs_int "$avg_power_uw")"
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
    --argjson powerW "$(to_decimal_units "$signed_power_uw")" \
    --argjson averagePowerW "$(to_decimal_units "$avg_power_uw")" \
    --argjson sampleCount "$sample_count" \
    --argjson sampleWindowSeconds "$sample_window_seconds" \
    --argjson windowComplete "$( [ "$sample_window_seconds" -ge "$window_seconds" ] && printf 'true' || printf 'false' )" \
    --argjson estimateSeconds "$estimate_seconds_json" \
    --argjson energyNowWh "$(to_decimal_units "$energy_now_uwh")" \
    --argjson energyFullWh "$(to_decimal_units "$energy_full_uwh")" \
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
