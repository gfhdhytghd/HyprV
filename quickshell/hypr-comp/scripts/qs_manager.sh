#!/usr/bin/env bash
set -euo pipefail

config_dir="$HOME/.config/HyprV/quickshell/hypr-comp"
action="${1:-toggle}"
target="${2:-applauncher}"

if [[ "$action" =~ ^[0-9]+$ ]]; then
    quickshell -p "$config_dir" ipc call hyprComp closeLauncher >/dev/null 2>&1 || true
    if [[ "$target" == "move" ]]; then
        hyprctl dispatch movetoworkspace "$action" >/dev/null 2>&1 || true
    else
        hyprctl dispatch workspace "$action" >/dev/null 2>&1 || true
    fi
    exit 0
fi

start_shell() {
    if ! quickshell list --all 2>/dev/null | grep -Fq "Config path: $config_dir/shell.qml"; then
        quickshell -p "$config_dir" -n -d >/dev/null 2>&1 || true
        sleep 0.2
    fi
}

case "$action:$target" in
    close:*|close:)
        quickshell -p "$config_dir" ipc call hyprComp closeLauncher >/dev/null 2>&1 || true
        ;;
    open:applauncher|open:)
        start_shell
        quickshell -p "$config_dir" ipc call hyprComp openLauncher >/dev/null 2>&1 || true
        ;;
    toggle:applauncher|toggle:)
        start_shell
        quickshell -p "$config_dir" ipc call hyprComp toggleLauncher >/dev/null 2>&1 || true
        ;;
    *)
        :
        ;;
esac
