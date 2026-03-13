#!/usr/bin/env bash

set -euo pipefail

if command -v pamixer >/dev/null 2>&1; then
    read -r muted volume < <(pamixer --get-mute --get-volume)
    printf 'available=true\nmuted=%s\nvolume=%s\n' "$muted" "$volume"
    exit 0
fi

if command -v wpctl >/dev/null 2>&1; then
    output="$(wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null || true)"
    if [[ -n "$output" ]]; then
        muted=false
        if [[ "$output" == *"[MUTED]"* ]]; then
            muted=true
        fi

        volume="$(awk 'NR == 1 { printf "%d", ($2 * 100) + 0.5 }' <<<"$output")"
        printf 'available=true\nmuted=%s\nvolume=%s\n' "$muted" "$volume"
        exit 0
    fi
fi

printf 'available=false\nmuted=false\nvolume=0\n'
exit 1
