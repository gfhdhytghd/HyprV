#!/usr/bin/env bash

set -euo pipefail

valid_temperature() {
    [[ "$1" =~ ^-?[0-9]+$ ]] && ((1 <= $1 && $1 <= 125000))
}

read_cpu_temperature() {
    local preferred zone type value hwmon name input base label

    # Thermal-zone names are stable across boot even though their numeric
    # thermal_zone index is not. Prefer the CPU package sensor when available.
    for preferred in x86_pkg_temp TCPU cpu_thermal cpu-thermal soc_thermal; do
        for zone in /sys/class/thermal/thermal_zone*; do
            [[ -r "${zone}/type" && -r "${zone}/temp" ]] || continue
            type="$(<"${zone}/type")"
            [[ "$type" == "$preferred" ]] || continue
            value="$(<"${zone}/temp")"
            if valid_temperature "$value"; then
                printf '%s\n' "$value"
                return 0
            fi
        done
    done

    # hwmon fallback covers Intel coretemp, AMD k10temp, and similarly named
    # CPU package sensors on systems without a useful thermal-zone entry.
    for hwmon in /sys/class/hwmon/hwmon*; do
        [[ -r "${hwmon}/name" ]] || continue
        name="$(<"${hwmon}/name")"
        [[ "$name" == coretemp || "$name" == k10temp || "$name" == zenpower ]] || continue
        for input in "${hwmon}"/temp*_input; do
            [[ -r "$input" ]] || continue
            base="${input%_input}"
            label=""
            [[ ! -r "${base}_label" ]] || label="$(<"${base}_label")"
            case "$label" in
                "Package id 0"|Tctl|"CPU Package"|CPU)
                    value="$(<"$input")"
                    if valid_temperature "$value"; then
                        printf '%s\n' "$value"
                        return 0
                    fi
                    ;;
            esac
        done
    done

    return 1
}

printf '__STAT__\n'
cat /proc/stat
printf '\n__MEM__\n'
cat /proc/meminfo
printf '\n__TEMP__\n'
read_cpu_temperature || true
printf '\n__CPU_POWER__\n'
[[ -r /run/hyprv/cpu-package-power-w ]] && cat /run/hyprv/cpu-package-power-w
printf '\n__ROUTE__\n'
cat /proc/net/route
printf '\n__NET__\n'
cat /proc/net/dev
