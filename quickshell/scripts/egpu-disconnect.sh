#!/usr/bin/env bash

set -euo pipefail

# Find external GPU PCI devices connected via Thunderbolt/USB4.
# Only targets GPUs whose PCI topology passes through a Thunderbolt bridge,
# avoiding accidental removal of built-in iGPU/dGPU.

is_thunderbolt_device() {
    local dev_path="$1"
    local real_path
    real_path="$(readlink -f "$dev_path")"

    # Walk the sysfs path upward looking for a Thunderbolt domain indicator.
    local current="$real_path"
    while [[ "$current" != "/" && "$current" != "/sys" ]]; do
        if [[ -d "$current/domain0" ]] || [[ "$(basename "$(dirname "$current")")" == "thunderbolt" ]]; then
            return 0
        fi

        if [[ -f "$current/label" ]]; then
            local label
            label="$(cat "$current/label" 2>/dev/null || true)"
            if [[ "$label" == *[Tt]hunderbolt* ]]; then
                return 0
            fi
        fi

        current="$(dirname "$current")"
    done

    # Fallback: external GPUs normally enumerate on a later bus than the
    # platform-integrated graphics devices.
    if [[ "$real_path" == */pci*/*:*:*/* ]]; then
        local bus_id
        local bus_num
        local bus_dec

        bus_id="$(basename "$dev_path")"
        bus_num="${bus_id#*:}"
        bus_num="${bus_num%%:*}"
        bus_dec=$((16#$bus_num))
        if (( bus_dec >= 3 )); then
            return 0
        fi
    fi

    return 1
}

is_pci_bdf() {
    [[ "$1" =~ ^[[:xdigit:]]{4}:[[:xdigit:]]{2}:[[:xdigit:]]{2}\.[[:xdigit:]]$ ]]
}

remove_pci_device() {
    local bdf="$1"
    local dev_path="/sys/bus/pci/devices/$bdf"
    local remove_file="$dev_path/remove"

    [[ -e "$dev_path" ]] || return 0
    [[ -f "$remove_file" ]] || return 0

    printf 'device=%s\n' "$bdf"
    if pkexec sh -c "echo 1 > '$remove_file'" 2>/dev/null; then
        printf 'status=removed\n'
        return 0
    fi

    printf 'status=error\n'
    return 1
}

collect_slot_functions() {
    local dev_path="$1"
    local bdf
    local slot_prefix
    local candidate

    bdf="$(basename "$dev_path")"
    slot_prefix="${bdf%.*}"

    for candidate in /sys/bus/pci/devices/"$slot_prefix".*; do
        [[ -e "$candidate" ]] || continue
        basename "$candidate"
    done | sort -rV
}

collect_parent_bridges() {
    local dev_path="$1"
    local current
    local current_name
    local parent_dir
    local class_file
    local class_value

    current="$(dirname "$(readlink -f "$dev_path")")"
    while [[ "$current" != "/sys" && "$current" != "/sys/devices" ]]; do
        current_name="$(basename "$current")"
        parent_dir="$(dirname "$current")"

        if is_pci_bdf "$current_name"; then
            class_file="$current/class"
            class_value="$(cat "$class_file" 2>/dev/null || true)"

            # Skip the platform root port directly attached to the host bus.
            if [[ "$class_value" == "0x060400" || "$class_value" == "0x060401" ]]; then
                if [[ "$(basename "$parent_dir")" != pci0000:* ]]; then
                    printf '%s\n' "$current_name"
                fi
            fi
        fi

        current="$parent_dir"
    done
}

remove_gpu_chain() {
    local dev_path="$1"
    local -A seen=()
    local bdf

    while IFS= read -r bdf; do
        [[ -n "$bdf" ]] || continue
        [[ -n "${seen[$bdf]:-}" ]] && continue
        seen["$bdf"]=1
        remove_pci_device "$bdf"
    done < <(
        collect_slot_functions "$dev_path"
        collect_parent_bridges "$dev_path"
    )
}

found=false
for dev in /sys/bus/pci/devices/*/; do
    class_file="$dev/class"
    [[ -f "$class_file" ]] || continue

    class="$(cat "$class_file" 2>/dev/null || true)"
    # VGA compatible controller (0x030000) or 3D controller (0x030200)
    if [[ "$class" == "0x030000" || "$class" == "0x030200" ]]; then
        if ! is_thunderbolt_device "$dev"; then
            continue
        fi

        remove_gpu_chain "$dev"
        found=true
        break
    fi
done

if [[ "$found" != "true" ]]; then
    printf 'status=not_found\n'
fi
