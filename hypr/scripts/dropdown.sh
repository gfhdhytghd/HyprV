#!/usr/bin/env bash
set -euo pipefail

STATE_DIR=/dev/shm
TOGGLE_FILE="$STATE_DIR/dropdown"
LOCK_DIR="$STATE_DIR/dropdown.lock"
PREV_FOCUS_FILE="$STATE_DIR/dropdown.prev_focus"
PREV_WS_FILE="$STATE_DIR/dropdown.prev_ws"

APP_CLASS='alacritty-dropdown'
HIDDEN_WORKSPACE='special:dropdown_hidden'
DEFAULT_VISIBLE_OFFSET=59
LOCK_TIMEOUT_SECONDS=5
WINDOW_WIDTH=1080
PIN_WHEN_VISIBLE=on

cleanup() {
  rmdir "$LOCK_DIR" 2>/dev/null || true
}

acquire_lock() {
  local attempts=0
  local max_attempts=$((LOCK_TIMEOUT_SECONDS * 10))

  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    ((attempts += 1))
    if (( attempts >= max_attempts )); then
      echo "获取锁超时" >&2
      return 1
    fi
    sleep 0.1
  done

  trap cleanup EXIT
}

write_state() {
  local path=$1
  local value=${2:-}

  if [[ -n "$value" ]]; then
    printf '%s\n' "$value" > "$path"
  else
    rm -f "$path"
  fi
}

read_state() {
  local path=$1
  [[ -f "$path" ]] || return 1
  cat "$path"
}

is_integer() {
  [[ ${1:-} =~ ^-?[0-9]+$ ]]
}

safe_hyprctl() {
  local attempt

  for attempt in 1 2 3; do
    if hyprctl "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done

  echo "hyprctl 命令执行失败: $*" >&2
  return 1
}

dropdown_address() {
  hyprctl clients -j 2>/dev/null | jq -r \
    --arg class "$APP_CLASS" \
    'first(.[] | select(.class == $class) | .address) // empty'
}

dropdown_exists() {
  [[ -n "$(dropdown_address)" ]]
}

window_workspace_name() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | jq -r \
    --arg addr "$addr" \
    'first(.[] | select(.address == $addr) | .workspace.name) // empty'
}

window_prop() {
  local addr=$1
  local jq_expr=$2

  hyprctl clients -j 2>/dev/null | jq -r \
    --arg addr "$addr" \
    "first(.[] | select(.address == \$addr) | $jq_expr) // empty"
}

monitor_prop_by_id() {
  local mon_id=$1
  local jq_expr=$2

  hyprctl monitors -j 2>/dev/null | jq -r \
    --arg mon_id "$mon_id" \
    "first(.[] | select((.id | tostring) == \$mon_id) | $jq_expr) // empty"
}

focused_monitor_prop() {
  local jq_expr=$1

  hyprctl monitors -j 2>/dev/null | jq -r \
    "first(.[] | select(.focused == true) | $jq_expr) // empty"
}

window_monitor_id() {
  window_prop "$1" '.monitor'
}

window_monitor_y() {
  local mon_id
  mon_id="$(window_monitor_id "$1")"
  [[ -n "$mon_id" ]] || return 1
  monitor_prop_by_id "$mon_id" '.y'
}

window_y() {
  window_prop "$1" '.at[1]'
}

window_x() {
  window_prop "$1" '.at[0]'
}

is_window_pinned() {
  local addr=$1
  [[ -n "$addr" ]] || return 1

  hyprctl clients -j 2>/dev/null | jq -e \
    --arg addr "$addr" \
    'any(.[]; .address == $addr and .pinned == true)' >/dev/null 2>&1
}

ensure_window_pinned_state() {
  local addr=$1
  local want_pinned=$2

  [[ -n "$addr" ]] || return 1

  if [[ "$want_pinned" == "on" ]]; then
    if ! is_window_pinned "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  else
    if is_window_pinned "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  fi
}

is_hidden() {
  local addr
  addr="$(dropdown_address)"
  [[ -n "$addr" ]] || return 1
  [[ "$(window_workspace_name "$addr")" == "$HIDDEN_WORKSPACE" ]]
}

move_vertical() {
  local delta=$1
  local addr=$2

  (( delta == 0 )) && return 0
  safe_hyprctl dispatch -- movewindowpixel "0 ${delta},address:${addr}"
}

move_horizontal() {
  local delta=$1
  local addr=$2

  (( delta == 0 )) && return 0
  safe_hyprctl dispatch -- movewindowpixel "${delta} 0,address:${addr}"
}

move_to_target_position() {
  local addr=$1
  local mon_x=$2
  local mon_y=$3
  local mon_w=$4
  local current_x
  local current_y
  local target_x
  local target_y

  current_x="$(window_x "$addr")"
  current_y="$(window_y "$addr")"
  if ! is_integer "$current_x" || ! is_integer "$current_y" || ! is_integer "$mon_x" || ! is_integer "$mon_y" || ! is_integer "$mon_w"; then
    return 1
  fi

  target_x="$(awk -v x="$mon_x" -v w="$mon_w" -v window_width="$WINDOW_WIDTH" 'BEGIN {
    printf "%d\n", x + (w / 4) - (window_width / 2)
  }')"
  if ! is_integer "$target_x"; then
    return 1
  fi

  target_y=$((mon_y + DEFAULT_VISIBLE_OFFSET))
  move_horizontal $((target_x - current_x)) "$addr"
  move_vertical $((target_y - current_y)) "$addr"
}

focus_and_move_to_target() {
  local addr=$1
  local mon_x=$2
  local mon_y=$3
  local mon_w=$4

  safe_hyprctl dispatch focuswindow "address:${addr}" || return 1
  move_to_target_position "$addr" "$mon_x" "$mon_y" "$mon_w"
}

save_previous_focus() {
  local focus_addr
  local workspace_id

  focus_addr="$(hyprctl activewindow -j 2>/dev/null | jq -r '.address // empty')"
  workspace_id="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty')"

  write_state "$PREV_FOCUS_FILE" "$focus_addr"
  write_state "$PREV_WS_FILE" "$workspace_id"
}

restore_previous_focus() {
  local previous_focus
  local previous_ws
  local current_ws

  previous_focus="$(read_state "$PREV_FOCUS_FILE" 2>/dev/null || true)"
  previous_ws="$(read_state "$PREV_WS_FILE" 2>/dev/null || true)"
  current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty')"

  rm -f "$PREV_FOCUS_FILE" "$PREV_WS_FILE"

  [[ -n "$previous_focus" ]] || return 0
  if [[ -n "$previous_ws" && -n "$current_ws" && "$previous_ws" != "$current_ws" ]]; then
    return 0
  fi

  hyprctl clients -j 2>/dev/null | jq -e \
    --arg addr "$previous_focus" \
    'any(.[]; .address == $addr)' >/dev/null || return 0

  safe_hyprctl dispatch focuswindow "address:${previous_focus}" || true
}

wait_for_dropdown() {
  local attempt

  for attempt in {1..20}; do
    if dropdown_exists; then
      return 0
    fi
    sleep 0.1
  done

  return 1
}

show_dropdown() {
  local addr=$1
  local current_ws
  local mon_x
  local mon_y
  local mon_w

  current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty')"
  mon_x="$(focused_monitor_prop '.x')"
  mon_y="$(focused_monitor_prop '.y')"
  mon_w="$(focused_monitor_prop '.width')"

  [[ -n "$current_ws" ]] || {
    echo "无法获取当前工作区" >&2
    return 1
  }

  save_previous_focus
  ensure_window_pinned_state "$addr" "off" || true
  safe_hyprctl dispatch movetoworkspacesilent "${current_ws},address:${addr}"
  focus_and_move_to_target "$addr" "$mon_x" "$mon_y" "$mon_w" || true
  ensure_window_pinned_state "$addr" "$PIN_WHEN_VISIBLE" || true
  touch "$TOGGLE_FILE"
}

hide_dropdown() {
  local addr=$1

  ensure_window_pinned_state "$addr" "off" || true
  safe_hyprctl dispatch movetoworkspacesilent "${HIDDEN_WORKSPACE},address:${addr}"
  rm -f "$TOGGLE_FILE"
  restore_previous_focus
}

spawn_dropdown() {
  local pid
  local addr
  local mon_x
  local mon_y
  local mon_w

  save_previous_focus
  rm -f "$TOGGLE_FILE"

  mon_x="$(focused_monitor_prop '.x')"
  mon_y="$(focused_monitor_prop '.y')"
  mon_w="$(focused_monitor_prop '.width')"

  nohup alacritty --class "${APP_CLASS},${APP_CLASS}" >/dev/null 2>&1 &
  pid=$!

  if ! wait_for_dropdown; then
    kill "$pid" 2>/dev/null || true
    echo "等待 Alacritty 窗口出现超时" >&2
    return 1
  fi

  addr="$(dropdown_address)"
  [[ -n "$addr" ]] || {
    echo "无法获取窗口地址" >&2
    return 1
  }

  ensure_window_pinned_state "$addr" "off" || true
  focus_and_move_to_target "$addr" "$mon_x" "$mon_y" "$mon_w" || true
  ensure_window_pinned_state "$addr" "$PIN_WHEN_VISIBLE" || true
  touch "$TOGGLE_FILE"
}

main() {
  local addr

  acquire_lock

  addr="$(dropdown_address)"
  if [[ -z "$addr" ]]; then
    spawn_dropdown
    return 0
  fi

  if is_hidden; then
    show_dropdown "$addr"
  else
    hide_dropdown "$addr"
  fi
}

main "$@"
