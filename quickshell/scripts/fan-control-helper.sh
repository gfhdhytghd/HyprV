#!/usr/bin/env bash

set -euo pipefail

readonly state_dir="/home/wilf/.local/state/hyprv"
readonly state_file="${state_dir}/fan-percent"

find_hwmon() {
    local candidate
    for candidate in /sys/class/hwmon/hwmon*; do
        [[ -r "${candidate}/name" ]] || continue
        if [[ "$(<"${candidate}/name")" == "nct6799" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

channel_index() {
    case "$1" in
        chassis) printf '1\n' ;;
        pump) printf '7\n' ;;
        *) return 1 ;;
    esac
}

read_number() {
    local path="$1"
    if [[ -r "$path" ]]; then
        local value
        value="$(<"$path")"
        [[ "$value" =~ ^[0-9]+$ ]] && printf '%s\n' "$value" && return 0
    fi
    printf '0\n'
}

save_state() {
    local hwmon="$1"
    local chassis_pwm pump_pwm temporary
    umask 077
    mkdir -p "$state_dir"
    exec 9>"${state_dir}/fan-percent.lock"
    flock 9
    chassis_pwm="$(read_number "${hwmon}/pwm1")"
    pump_pwm="$(read_number "${hwmon}/pwm7")"
    temporary="${state_file}.$$"
    printf 'chassis=%s\npump=%s\n' \
        "$(((chassis_pwm * 100 + 127) / 255))" \
        "$(((pump_pwm * 100 + 127) / 255))" >"$temporary"
    mv -f "$temporary" "$state_file"
}

print_status() {
    local hwmon
    if ! hwmon="$(find_hwmon)"; then
        printf 'fan_available=false\n'
        return 0
    fi

    local chassis_pwm pump_pwm
    chassis_pwm="$(read_number "${hwmon}/pwm1")"
    pump_pwm="$(read_number "${hwmon}/pwm7")"

    printf 'fan_available=true\n'
    printf 'fan_chassis_percent=%s\n' "$(((chassis_pwm * 100 + 127) / 255))"
    printf 'fan_chassis_rpm=%s\n' "$(read_number "${hwmon}/fan1_input")"
    printf 'fan_pump_percent=%s\n' "$(((pump_pwm * 100 + 127) / 255))"
    printf 'fan_pump_rpm=%s\n' "$(read_number "${hwmon}/fan7_input")"
}

set_percent() {
    [[ $# -eq 2 ]] || return 2

    local channel="$1"
    local percent="$2"
    local index hwmon pwm

    index="$(channel_index "$channel")" || {
        printf 'unsupported fan channel\n' >&2
        return 2
    }
    if [[ ! "$percent" =~ ^[0-9]+$ ]] || ((percent < 0 || percent > 100)); then
        printf 'fan percentage must be an integer from 0 to 100\n' >&2
        return 2
    fi
    hwmon="$(find_hwmon)" || {
        printf 'nct6799 fan controller is unavailable\n' >&2
        return 1
    }
    [[ -w "${hwmon}/pwm${index}_enable" && -w "${hwmon}/pwm${index}" ]] || {
        printf 'fan control requires root privileges\n' >&2
        return 1
    }

    pwm=$(((percent * 255 + 50) / 100))
    printf '1' >"${hwmon}/pwm${index}_enable"
    printf '%s' "$pwm" >"${hwmon}/pwm${index}"
    save_state "$hwmon"
}


restore_state() {
    [[ -r "$state_file" ]] || return 0

    local key value chassis="" pump=""
    while IFS='=' read -r key value; do
        [[ "$value" =~ ^[0-9]+$ ]] || continue
        ((value >= 0 && value <= 100)) || continue
        case "$key" in
            chassis) chassis="$value" ;;
            pump) pump="$value" ;;
        esac
    done <"$state_file"
    [[ -z "$chassis" ]] || set_percent chassis "$chassis"
    [[ -z "$pump" ]] || set_percent pump "$pump"
}

grant_permissions() {
    [[ $EUID -eq 0 ]] || {
        printf 'grant requires root privileges\n' >&2
        return 1
    }

    local hwmon index path
    hwmon="$(find_hwmon)" || {
        printf 'nct6799 fan controller is unavailable\n' >&2
        return 1
    }
    for index in 1 7; do
        for path in "${hwmon}/pwm${index}_enable" "${hwmon}/pwm${index}"; do
            [[ -e "$path" ]] || {
                printf 'missing fan control attribute: %s\n' "$path" >&2
                return 1
            }
            chown wilf "$path"
            chmod 0600 "$path"
        done
    done
}

case "${1:-}" in
    status)
        [[ $# -eq 1 ]] || exit 2
        print_status
        ;;
    set)
        [[ $# -eq 3 ]] || exit 2
        set_percent "$2" "$3"
        ;;
    grant)
        [[ $# -eq 1 ]] || exit 2
        grant_permissions
        ;;
    restore)
        [[ $# -eq 1 ]] || exit 2
        restore_state
        ;;
    *)
        printf 'usage: %s status | set {chassis|pump} {0..100} | grant | restore\n' "${0##*/}" >&2
        exit 2
        ;;
esac
