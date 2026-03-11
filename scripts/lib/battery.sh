#!/bin/sh

hyprv_pick_state_dir() {
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

hyprv_find_battery_dir() {
    for path in /sys/class/power_supply/BAT*; do
        [ -d "$path" ] || continue
        printf '%s\n' "$path"
        return 0
    done
    return 1
}

hyprv_read_first_existing() {
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

hyprv_safe_int() {
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

hyprv_abs_int() {
    value="$(hyprv_safe_int "${1:-0}")"
    case "$value" in
        -*)
            printf '%s\n' "${value#-}"
            ;;
        *)
            printf '%s\n' "$value"
            ;;
    esac
}

hyprv_mul_div_round() {
    awk -v a="$1" -v b="$2" -v d="$3" 'BEGIN {
        if ((d + 0) == 0) {
            print 0;
            exit;
        }
        printf "%.0f\n", (a + 0) * (b + 0) / d;
    }'
}

hyprv_micro_to_decimal_units() {
    awk -v value="$1" 'BEGIN {
        printf "%.3f\n", (value + 0) / 1000000;
    }'
}
