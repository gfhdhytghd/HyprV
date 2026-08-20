#!/usr/bin/env bash

set -euo pipefail

readonly output_dir="/run/hyprv"
readonly output_file="${output_dir}/cpu-package-power-w"

umask 022
mkdir -p "$output_dir"

while true; do
    sample="$(LC_ALL=C /usr/bin/turbostat --quiet --interval 1 --num_iterations 2 --show PkgWatt 2>/dev/null || true)"
    value="$(awk '/^[0-9]+([.][0-9]+)?$/ { last = $1 } END { if (last != "") print last }' <<<"$sample")"
    if [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
        temporary="${output_file}.$$"
        printf '%s\n' "$value" >"$temporary"
        chmod 0644 "$temporary"
        mv -f "$temporary" "$output_file"
    else
        sleep 1
    fi
done
