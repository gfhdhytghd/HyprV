#!/usr/bin/env bash
set -euo pipefail

TOGGLE=/dev/shm/sidechat
LOCK_FILE=/dev/shm/sidechat.lock
LAST_TRIGGER_FILE=/dev/shm/sidechat.last_trigger_ms
PREV_FOCUS=/dev/shm/sidechat.prev_focus
PREV_WS=/dev/shm/sidechat.prev_ws
HIDDEN_WS='special:sidechat_hidden'
DEBOUNCE_MS=300
TRACE_FILE=/tmp/sidechat.trace

SIDECHAT_WIDTH=360

APP_CLASS_REGEX='^ai-hub$'

log_trace() {
  local msg=$1
  printf '%s pid=%s %s\n' "$(date '+%F %T.%3N')" "$$" "$msg" >> "$TRACE_FILE"
}

acquire_lock() {
  local timeout=5
  local count=0
  while ! mkdir "$LOCK_FILE" 2>/dev/null; do
    sleep 0.1
    ((count++))
    if [[ $count -gt $((timeout * 10)) ]]; then
      echo "获取锁超时" >&2
      return 1
    fi
  done
  trap 'release_lock' EXIT
}

release_lock() {
  rmdir "$LOCK_FILE" 2>/dev/null || true
}

get_now_ms() {
  local now
  now="$(date +%s%3N 2>/dev/null || true)"
  if [[ ! "$now" =~ ^[0-9]+$ ]]; then
    now="$(( $(date +%s) * 1000 ))"
  fi
  printf '%s\n' "$now"
}

is_recent_trigger() {
  local now
  local last
  local delta

  now="$(get_now_ms)"
  if [[ -f "$LAST_TRIGGER_FILE" ]]; then
    last="$(cat "$LAST_TRIGGER_FILE" 2>/dev/null || true)"
    if [[ "$last" =~ ^[0-9]+$ ]]; then
      delta=$((now - last))
      if [[ $delta -ge 0 && $delta -lt $DEBOUNCE_MS ]]; then
        return 0
      fi
    fi
  fi

  printf '%s\n' "$now" > "$LAST_TRIGGER_FILE"
  return 1
}

get_sidechat_status() {
  hyprctl clients -j 2>/dev/null | \
    jq --arg re "$APP_CLASS_REGEX" 'any(.[]; .class | test($re))' 2>/dev/null | \
    grep -q true 2>/dev/null
}

get_sidechat_address() {
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg re "$APP_CLASS_REGEX" --arg hidden "$HIDDEN_WS" \
      '(first(.[] | select((.class | test($re)) and .workspace.name != $hidden) | .address) //
        first(.[] | select(.class | test($re)) | .address)) // empty' 2>/dev/null
}

cleanup_duplicate_sidechat_windows() {
  local keep_addr
  local addr
  keep_addr="$(get_sidechat_address)"
  [[ -n "$keep_addr" ]] || return 0

  while IFS= read -r addr; do
    [[ -n "$addr" ]] || continue
    safe_hyprctl dispatch closewindow "address:${addr}" || true
  done < <(
    hyprctl clients -j 2>/dev/null | \
      jq -r --arg re "$APP_CLASS_REGEX" --arg keep "$keep_addr" \
        '.[] | select((.class | test($re)) and .address != $keep) | .address' 2>/dev/null
  )
}

is_hidden_in_special() {
  local addr
  addr="$(get_sidechat_address)"
  [[ -n "$addr" ]] || return 1

  is_hidden_in_special_by_address "$addr"
}

is_hidden_in_special_by_address() {
  local addr=$1
  [[ -n "$addr" ]] || return 1

  hyprctl clients -j 2>/dev/null | \
    jq -e --arg addr "$addr" --arg ws "$HIDDEN_WS" \
      'any(.[]; .address == $addr and .workspace.name == $ws)' >/dev/null 2>&1
}

is_marked_visible() {
  [[ -f "$TOGGLE" ]]
}

sync_visibility_state() {
  local addr=$1

  [[ -n "$addr" ]] || {
    rm -f "$TOGGLE"
    return 0
  }

  if is_hidden_in_special_by_address "$addr"; then
    rm -f "$TOGGLE"
  else
    touch "$TOGGLE"
  fi
}

wait_for_window() {
  local expected_exists=$1
  local max_attempts=40
  local attempt=0

  while [[ $attempt -lt $max_attempts ]]; do
    if [[ $expected_exists == "true" ]]; then
      if get_sidechat_status; then
        return 0
      fi
    else
      if ! get_sidechat_status; then
        return 0
      fi
    fi
    sleep 0.1
    ((attempt++))
  done
  return 1
}

safe_hyprctl() {
  local max_retries=3
  local retry=0

  while [[ $retry -lt $max_retries ]]; do
    if hyprctl "$@" 2>/dev/null; then
      return 0
    fi
    ((retry++))
    sleep 0.1
  done

  echo "hyprctl命令执行失败: $*" >&2
  return 1
}

is_integer() {
  [[ "${1:-}" =~ ^-?[0-9]+$ ]]
}

get_window_monitor_id() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" \
      'first(.[] | select(.address == $addr) | .monitor) // empty' 2>/dev/null
}

get_window_x() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" \
      'first(.[] | select(.address == $addr) | .at[0]) // empty' 2>/dev/null
}

get_window_y() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" \
      'first(.[] | select(.address == $addr) | .at[1]) // empty' 2>/dev/null
}

get_window_width() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" \
      'first(.[] | select(.address == $addr) | .size[0]) // empty' 2>/dev/null
}

get_window_height() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" \
      'first(.[] | select(.address == $addr) | .size[1]) // empty' 2>/dev/null
}

is_window_pinned_by_address() {
  local addr=$1
  [[ -n "$addr" ]] || return 1

  hyprctl clients -j 2>/dev/null | \
    jq -e --arg addr "$addr" \
      'any(.[]; .address == $addr and .pinned == true)' >/dev/null 2>&1
}

ensure_window_pinned_state() {
  local addr=$1
  local want_pinned=$2

  [[ -n "$addr" ]] || return 1

  if [[ "$want_pinned" == "on" ]]; then
    if ! is_window_pinned_by_address "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  else
    if is_window_pinned_by_address "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  fi
}

get_monitor_x_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .x) // empty' 2>/dev/null
}

get_monitor_y_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .y) // empty' 2>/dev/null
}

get_monitor_width_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .width) // empty' 2>/dev/null
}

get_monitor_height_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .height) // empty' 2>/dev/null
}

get_monitor_id_by_name() {
  local mon_name=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_name "$mon_name" \
      'first(.[] | select(.name == $mon_name) | .id) // empty' 2>/dev/null
}

get_monitor_active_workspace_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .activeWorkspace.id) // empty' 2>/dev/null
}

get_active_workspace_id() {
  hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty' 2>/dev/null
}

get_active_workspace_monitor_name() {
  hyprctl activeworkspace -j 2>/dev/null | jq -r '.monitor // empty' 2>/dev/null
}

get_focused_monitor_id() {
  hyprctl monitors -j 2>/dev/null | \
    jq -r 'first(.[] | select(.focused == true) | .id) // empty' 2>/dev/null
}

get_focused_monitor_x() {
  hyprctl monitors -j 2>/dev/null | \
    jq -r 'first(.[] | select(.focused == true) | .x) // empty' 2>/dev/null
}

get_focused_monitor_y() {
  hyprctl monitors -j 2>/dev/null | \
    jq -r 'first(.[] | select(.focused == true) | .y) // empty' 2>/dev/null
}

get_focused_monitor_width() {
  hyprctl monitors -j 2>/dev/null | \
    jq -r 'first(.[] | select(.focused == true) | .width) // empty' 2>/dev/null
}

get_focused_monitor_height() {
  hyprctl monitors -j 2>/dev/null | \
    jq -r 'first(.[] | select(.focused == true) | .height) // empty' 2>/dev/null
}

get_monitor_id_by_cursor() {
  local cursor_x
  local cursor_y

  cursor_x="$(hyprctl cursorpos -j 2>/dev/null | jq -r '(.x | floor) // empty' 2>/dev/null)"
  cursor_y="$(hyprctl cursorpos -j 2>/dev/null | jq -r '(.y | floor) // empty' 2>/dev/null)"
  if ! is_integer "$cursor_x" || ! is_integer "$cursor_y"; then
    return 1
  fi

  hyprctl monitors -j 2>/dev/null | \
    jq -r --argjson x "$cursor_x" --argjson y "$cursor_y" \
      'first(.[] | select($x >= .x and $x < (.x + .width) and $y >= .y and $y < (.y + .height)) | .id) // empty' 2>/dev/null
}

get_target_monitor_id() {
  local mon_id
  local mon_name

  mon_name="$(get_active_workspace_monitor_name || true)"
  mon_id="$(get_monitor_id_by_name "$mon_name" || true)"
  if is_integer "$mon_id"; then
    printf '%s\n' "$mon_id"
    return 0
  fi

  mon_id="$(get_focused_monitor_id || true)"
  if is_integer "$mon_id"; then
    printf '%s\n' "$mon_id"
    return 0
  fi
  return 1
}

move_window_exact() {
  local x=$1
  local y=$2
  local addr=$3
  safe_hyprctl dispatch -- movewindowpixel "exact ${x} ${y},address:${addr}" || return 1
}

resize_window_exact() {
  local w=$1
  local h=$2
  local addr=$3
  safe_hyprctl dispatch -- resizewindowpixel "exact ${w} ${h},address:${addr}" || return 1
}

apply_sidechat_geometry() {
  local addr=$1
  local mon_x=$2
  local mon_y=$3
  local mon_w=$4
  local mon_h=$5
  local target_h
  local target_x
  local target_y

  if ! is_integer "$mon_x" || ! is_integer "$mon_y" || ! is_integer "$mon_w" || ! is_integer "$mon_h"; then
    return 1
  fi

  target_h=$((mon_h / 2 - 71))
  if [[ $target_h -lt 1 ]]; then
    target_h=1
  fi

  target_x=$((mon_x + (mon_w / 2) - 370))
  target_y=$((mon_y + 59))

  resize_window_exact "$SIDECHAT_WIDTH" "$target_h" "$addr" || return 1
  move_window_exact "$target_x" "$target_y" "$addr"
}

apply_sidechat_geometry_on_focused_monitor() {
  local addr=$1
  local mon_id
  mon_id="$(get_focused_monitor_id)"
  is_integer "$mon_id" || return 1
  apply_sidechat_geometry_on_monitor_id "$addr" "$mon_id"
}

apply_sidechat_geometry_on_monitor_id() {
  local addr=$1
  local mon_id=$2
  local mon_x
  local mon_y
  local mon_w
  local mon_h

  mon_x="$(get_monitor_x_by_id "$mon_id")"
  mon_y="$(get_monitor_y_by_id "$mon_id")"
  mon_w="$(get_monitor_width_by_id "$mon_id")"
  mon_h="$(get_monitor_height_by_id "$mon_id")"
  if ! is_integer "$mon_x" || ! is_integer "$mon_y" || ! is_integer "$mon_w" || ! is_integer "$mon_h"; then
    return 1
  fi
  apply_sidechat_geometry "$addr" "$mon_x" "$mon_y" "$mon_w" "$mon_h"
}

apply_sidechat_geometry_on_own_monitor() {
  local addr=$1
  local mon_id
  local mon_x
  local mon_y
  local mon_w
  local mon_h

  mon_id="$(get_window_monitor_id "$addr")"
  [[ -n "$mon_id" ]] || return 1

  mon_x="$(get_monitor_x_by_id "$mon_id")"
  mon_y="$(get_monitor_y_by_id "$mon_id")"
  mon_w="$(get_monitor_width_by_id "$mon_id")"
  mon_h="$(get_monitor_height_by_id "$mon_id")"
  if ! is_integer "$mon_x" || ! is_integer "$mon_y" || ! is_integer "$mon_w" || ! is_integer "$mon_h"; then
    return 1
  fi
  apply_sidechat_geometry "$addr" "$mon_x" "$mon_y" "$mon_w" "$mon_h"
}

save_prev_focus() {
  local current_focus
  local current_ws
  current_focus="$(hyprctl activewindow -j 2>/dev/null | jq -r '.address // empty' 2>/dev/null)"
  current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty' 2>/dev/null)"
  if [[ -n "$current_focus" ]]; then
    printf '%s\n' "$current_focus" > "$PREV_FOCUS"
  else
    rm -f "$PREV_FOCUS"
  fi
  if [[ -n "$current_ws" ]]; then
    printf '%s\n' "$current_ws" > "$PREV_WS"
  else
    rm -f "$PREV_WS"
  fi
}

restore_prev_focus() {
  local prev_focus
  local prev_ws
  local current_ws

  if [[ ! -f "$PREV_FOCUS" ]]; then
    rm -f "$PREV_WS"
    return 0
  fi

  prev_focus="$(cat "$PREV_FOCUS")"
  if [[ -z "$prev_focus" ]]; then
    return 0
  fi

  if [[ -f "$PREV_WS" ]]; then
    prev_ws="$(cat "$PREV_WS")"
    current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty' 2>/dev/null)"
    if [[ -n "$prev_ws" && -n "$current_ws" && "$prev_ws" != "$current_ws" ]]; then
      return 0
    fi
  fi

  if hyprctl clients -j 2>/dev/null | jq -e --arg addr "$prev_focus" 'any(.[]; .address == $addr)' >/dev/null 2>&1; then
    safe_hyprctl dispatch focuswindow "address:$prev_focus" || true
  fi
}

clear_prev_focus_state() {
  rm -f "$PREV_FOCUS" "$PREV_WS"
}

showchat() {
  local addr
  local current_ws
  local target_mon
  local target_ws
  local should_show=0

  if ! get_sidechat_status; then
    echo "聊天窗口不存在，无法切换状态" >&2
    return 1
  fi

  addr="$(get_sidechat_address)"
  if [[ -z "$addr" ]]; then
    echo "无法获取聊天窗口地址" >&2
    return 1
  fi

  sync_visibility_state "$addr"

  if ! is_marked_visible || is_hidden_in_special_by_address "$addr"; then
    should_show=1
  fi

  if [[ $should_show -eq 1 ]]; then
    log_trace "showchat: hidden->show addr=${addr}"
    save_prev_focus
    current_ws="$(get_active_workspace_id || true)"
    target_mon="$(get_target_monitor_id || true)"
    if ! is_integer "$current_ws"; then
      target_ws="$(get_monitor_active_workspace_by_id "$target_mon" || true)"
    else
      target_ws="$current_ws"
    fi
    if is_integer "$target_ws"; then
      current_ws="$target_ws"
    fi
    if [[ -z "$current_ws" ]]; then
      echo "无法获取当前工作区" >&2
      return 1
    fi

    ensure_window_pinned_state "$addr" "off" || true
    if safe_hyprctl dispatch movetoworkspacesilent "${current_ws},address:${addr}" && \
       safe_hyprctl dispatch focuswindow "address:${addr}"; then
      if is_integer "$target_mon"; then
        if ! apply_sidechat_geometry_on_monitor_id "$addr" "$target_mon"; then
          apply_sidechat_geometry_on_own_monitor "$addr" || true
        fi
      else
        if ! apply_sidechat_geometry_on_focused_monitor "$addr"; then
          apply_sidechat_geometry_on_own_monitor "$addr" || true
        fi
      fi
      ensure_window_pinned_state "$addr" "on" || true
      safe_hyprctl dispatch alterzorder "top,address:${addr}" || true
      touch "$TOGGLE"
      echo "聊天窗口已显示"
      log_trace "showchat: shown addr=${addr} ws=${current_ws}"
    else
      echo "显示聊天窗口失败" >&2
      log_trace "showchat: show_failed addr=${addr}"
      return 1
    fi
  else
    log_trace "showchat: visible->hide addr=${addr}"
    ensure_window_pinned_state "$addr" "off" || true
    restore_prev_focus
    if safe_hyprctl dispatch movetoworkspacesilent "${HIDDEN_WS},address:${addr}"; then
      rm -f "$TOGGLE"
      clear_prev_focus_state
      echo "聊天窗口已隐藏"
      log_trace "showchat: hidden addr=${addr}"
    else
      echo "隐藏聊天窗口失败" >&2
      log_trace "showchat: hide_failed addr=${addr}"
      return 1
    fi
  fi
}

main() {
  log_trace "main:start"
  if ! acquire_lock; then
    echo "无法获取锁，可能有其他实例在运行" >&2
    log_trace "main:lock_failed"
    exit 1
  fi
  if is_recent_trigger; then
    log_trace "main:debounced"
    exit 0
  fi

  cleanup_duplicate_sidechat_windows

  if get_sidechat_status; then
    log_trace "main:status=exists"
    sync_visibility_state "$(get_sidechat_address)"
    showchat
  else
    local addr
    local target_mon
    local target_ws
    local current_ws
    echo "启动新的 sidechat 实例..."
    log_trace "main:status=missing launch"
    save_prev_focus
    rm -f "$TOGGLE"
    target_mon="$(get_target_monitor_id || true)"
    current_ws="$(get_active_workspace_id || true)"
    if is_integer "$current_ws"; then
      target_ws="$current_ws"
    else
      target_ws="$(get_monitor_active_workspace_by_id "$target_mon" || true)"
    fi
    nohup ai-hub \
      >/dev/null 2>&1 &
    local chrome_pid=$!

    if wait_for_window "true"; then
      addr="$(get_sidechat_address)"
      if [[ -n "$addr" ]]; then
        ensure_window_pinned_state "$addr" "off" || true
        if is_integer "$target_ws"; then
          safe_hyprctl dispatch movetoworkspacesilent "${target_ws},address:${addr}" || true
        fi
        safe_hyprctl dispatch focuswindow "address:${addr}" || true
        if is_integer "$target_mon"; then
          if ! apply_sidechat_geometry_on_monitor_id "$addr" "$target_mon"; then
            apply_sidechat_geometry_on_own_monitor "$addr" || true
          fi
        else
          if ! apply_sidechat_geometry_on_focused_monitor "$addr"; then
            apply_sidechat_geometry_on_own_monitor "$addr" || true
          fi
        fi
        ensure_window_pinned_state "$addr" "on" || true
        safe_hyprctl dispatch alterzorder "top,address:${addr}" || true
      fi
      touch "$TOGGLE"
      echo "sidechat 启动成功"
      log_trace "main:launch_success addr=${addr}"
    else
      echo "等待 sidechat 窗口出现超时" >&2
      kill "$chrome_pid" 2>/dev/null || true
      log_trace "main:launch_timeout"
      exit 1
    fi
  fi
}

main "$@"
