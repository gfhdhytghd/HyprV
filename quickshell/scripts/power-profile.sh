#!/bin/sh

current="$(powerprofilesctl get 2>/dev/null || printf 'balanced')"

case "$current" in
    performance)
        next="balanced"
        icon="⚡"
        ;;
    balanced)
        next="power-saver"
        icon="⚖️"
        ;;
    power-saver)
        next="performance"
        icon="🔋"
        ;;
    *)
        next="balanced"
        icon="❓"
        ;;
esac

if [ "${1:-}" = "toggle" ]; then
    powerprofilesctl set "$next"
    exit 0
fi

printf '%s\n' "$icon"
