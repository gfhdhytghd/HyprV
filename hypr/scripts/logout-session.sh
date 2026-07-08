#!/usr/bin/env bash
set -euo pipefail

hyprctl_current() {
    hyprctl "$@" 2>/dev/null
}

hyprctl_fallback() {
    hyprctl -i 0 "$@" 2>/dev/null
}

hyprctl_any() {
    hyprctl_current "$@" || hyprctl_fallback "$@"
}

info="$(hyprctl_any systeminfo || true)"

if grep -q 'configProvider: lua' <<<"$info"; then
    hyprctl_any dispatch 'hl.dsp.exit()'
    exit $?
fi

if grep -q 'configProvider: hyprlang' <<<"$info"; then
    hyprctl_any dispatch exit
    exit $?
fi

hyprctl_any dispatch 'hl.dsp.exit()' || hyprctl_any dispatch exit
