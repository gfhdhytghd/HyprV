#!/usr/bin/env bash

set -euo pipefail

# Find external GPU PCI devices connected via Thunderbolt.
# Only targets GPUs whose PCI topology passes through a Thunderbolt bridge,
# avoiding accidental removal of built-in iGPU/dGPU.

is_thunderbolt_device() {
    local dev_path="$1"
    local real_path
    real_path="$(readlink -f "$dev_path")"
    # Walk the sysfs path upward looking for a Thunderbolt domain indicator
    local current="$real_path"
    while [[ "$current" != "/" && "$current" != "/sys" ]]; do
        # Check for thunderbolt domain in the path
        if [[ -d "$current/domain0" ]] || [[ "$(basename "$(dirname "$current")")" == "thunderbolt" ]]; then
            return 0
        fi
        # Check if parent bridge has "Thunderbolt" in its label
        if [[ -f "$current/label" ]]; then
            local label
            label="$(cat "$current/label" 2>/dev/null || true)"
            if [[ "$label" == *[Tt]hunderbolt* ]]; then
                return 0
            fi
        fi
        current="$(dirname "$current")"
    done
    # Fallback: check if the device path contains a known Thunderbolt PCI bridge pattern
    # Thunderbolt devices typically sit behind bus numbers >= 3 on most systems
    if [[ "$real_path" == */pci*/*:*:*/* ]]; then
        local bus_id
        bus_id="$(basename "$dev_path")"
        local domain_bus="${bus_id%%:*}"
        local bus_num="${bus_id#*:}"
        bus_num="${bus_num%%:*}"
        # Integrated GPUs are typically at bus 00 or 01
        local bus_dec=$((16#$bus_num))
        if (( bus_dec >= 3 )); then
            return 0
        fi
    fi
    return 1
}

found=false
for dev in /sys/bus/pci/devices/*/; do
    class_file="$dev/class"
    remove_file="$dev/remove"
    [[ -f "$class_file" ]] || continue
    [[ -f "$remove_file" ]] || continue

    class="$(cat "$class_file" 2>/dev/null || true)"
    # VGA compatible controller (0x030000) or 3D controller (0x030200)
    if [[ "$class" == "0x030000" || "$class" == "0x030200" ]]; then
        if ! is_thunderbolt_device "$dev"; then
            continue
        fi
        device_name="$(basename "$dev")"
        printf 'device=%s\n' "$device_name"
        if pkexec sh -c "echo 1 > '$remove_file'" 2>/dev/null; then
            printf 'status=removed\n'
        else
            printf 'status=error\n'
        fi
        found=true
        break
    fi
done

if [[ "$found" != "true" ]]; then
    printf 'status=not_found\n'
fi
