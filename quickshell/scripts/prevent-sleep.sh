#!/usr/bin/env bash

set -euo pipefail

STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell"
PID_FILE="$STATE_DIR/prevent-sleep.pid"

ensure_state_dir() {
    mkdir -p "$STATE_DIR"
}

read_pid() {
    [[ -f "$PID_FILE" ]] || return 1
    tr -d '[:space:]' < "$PID_FILE"
}

is_pid_active() {
    local pid="$1"
    [[ -n "$pid" ]] || return 1
    kill -0 "$pid" 2>/dev/null || return 1
    ps -o command= -p "$pid" 2>/dev/null | grep -Fq "systemd-inhibit --what=idle:sleep"
}

cleanup_stale_state() {
    local pid
    pid="$(read_pid 2>/dev/null || true)"
    if [[ -n "$pid" ]] && ! is_pid_active "$pid"; then
        rm -f "$PID_FILE"
    fi
}

status() {
    cleanup_stale_state
    local pid
    pid="$(read_pid 2>/dev/null || true)"
    if [[ -n "$pid" ]] && is_pid_active "$pid"; then
        printf 'enabled=true\n'
        printf 'pid=%s\n' "$pid"
        return 0
    fi
    printf 'enabled=false\n'
    printf 'pid=\n'
    return 1
}

enable() {
    ensure_state_dir
    cleanup_stale_state

    local pid
    pid="$(read_pid 2>/dev/null || true)"
    if [[ -n "$pid" ]] && is_pid_active "$pid"; then
        printf 'enabled=true\n'
        printf 'pid=%s\n' "$pid"
        return 0
    fi

    nohup systemd-inhibit \
        --what=idle:sleep \
        --mode=block \
        --why="HyprV prevent sleep" \
        sh -lc 'exec sleep infinity' >/dev/null 2>&1 &
    pid="$!"
    printf '%s\n' "$pid" > "$PID_FILE"
    sleep 0.1

    if ! is_pid_active "$pid"; then
        rm -f "$PID_FILE"
        printf 'failed to enable prevent sleep\n' >&2
        return 1
    fi

    printf 'enabled=true\n'
    printf 'pid=%s\n' "$pid"
}

disable() {
    cleanup_stale_state
    local pid
    pid="$(read_pid 2>/dev/null || true)"
    if [[ -z "$pid" ]]; then
        printf 'enabled=false\n'
        printf 'pid=\n'
        return 0
    fi

    kill "$pid" 2>/dev/null || true
    rm -f "$PID_FILE"
    printf 'enabled=false\n'
    printf 'pid=\n'
}

toggle() {
    if status >/dev/null 2>&1; then
        disable
    else
        enable
    fi
}

case "${1:-status}" in
    status)
        status
        ;;
    enable)
        enable
        ;;
    disable)
        disable
        ;;
    toggle)
        toggle
        ;;
    *)
        printf 'Usage: %s [status|enable|disable|toggle]\n' "$0" >&2
        exit 1
        ;;
esac
