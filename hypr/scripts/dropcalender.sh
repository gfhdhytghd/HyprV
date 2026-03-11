#!/usr/bin/env bash
set -euo pipefail    # 出错立即退出，调试更方便

TOGGLE=/dev/shm/calendertoggle
LOCK_FILE=/dev/shm/calendertoggle.lock
PREV_FOCUS=/dev/shm/calendertoggle.prev_focus
PREV_WS=/dev/shm/calendertoggle.prev_ws
VISIBLE_OFFSET_FILE=/dev/shm/calendertoggle.visible_offset
HIDDEN_WS='special:calendar_hidden'
ANIM_OFFSET_FALLBACK=1000
DEFAULT_VISIBLE_OFFSET=60
PIN_WHEN_VISIBLE=off
# Hyprctl 用到的窗口匹配正则，整个用单引号包住即可
APP_REGEX='class:^(chrome-calendar\.google\.com.*Default)$'

# 获取文件锁，避免并发操作
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

# 释放文件锁
release_lock() {
  rmdir "$LOCK_FILE" 2>/dev/null || true
}

# 判断窗口是否已存在：有 → 返回 0，无 → 返回 1
getcalendarstatus() {
  hyprctl clients -j 2>/dev/null | \
    jq 'any(.[]; .class | test("chrome-calendar\\.google\\.com.*Default"))' 2>/dev/null | \
    grep -q true 2>/dev/null
}

# 获取日历窗口地址
get_calendar_address() {
  hyprctl clients -j 2>/dev/null | \
    jq -r 'first(.[] | select(.class | test("chrome-calendar\\.google\\.com.*Default")) | .address) // empty' 2>/dev/null
}

# 判断是否在隐藏专用工作区
is_hidden_in_special() {
  local addr
  addr="$(get_calendar_address)"
  [[ -n "$addr" ]] || return 1

  hyprctl clients -j 2>/dev/null | \
    jq -e --arg addr "$addr" --arg ws "$HIDDEN_WS" \
      'any(.[]; .address == $addr and .workspace.name == $ws)' >/dev/null 2>&1
}

# 等待窗口状态变化
wait_for_window() {
  local expected_exists=$1
  local max_attempts=30  # 增加等待时间，Chrome启动较慢
  local attempt=0
  
  while [[ $attempt -lt $max_attempts ]]; do
    if [[ $expected_exists == "true" ]]; then
      if getcalendarstatus; then
        return 0
      fi
    else
      if ! getcalendarstatus; then
        return 0
      fi
    fi
    sleep 0.2
    ((attempt++))
  done
  return 1
}

# 安全执行hyprctl命令
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

# 单次垂直位移动画（高效版）
animate_vertical() {
  local dy=$1
  local addr=$2
  safe_hyprctl dispatch -- movewindowpixel "0 ${dy}","address:${addr}" || true
}

is_integer() {
  [[ "${1:-}" =~ ^-?[0-9]+$ ]]
}

get_window_y() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" 'first(.[] | select(.address == $addr) | .at[1]) // empty' 2>/dev/null
}

get_window_monitor_id() {
  local addr=$1
  hyprctl clients -j 2>/dev/null | \
    jq -r --arg addr "$addr" 'first(.[] | select(.address == $addr) | .monitor) // empty' 2>/dev/null
}

get_monitor_y_by_id() {
  local mon_id=$1
  hyprctl monitors -j 2>/dev/null | \
    jq -r --arg mon_id "$mon_id" \
      'first(.[] | select((.id | tostring) == $mon_id) | .y) // empty' 2>/dev/null
}

get_window_monitor_y() {
  local addr=$1
  local mon_id
  local mon_y

  mon_id="$(get_window_monitor_id "$addr")"
  [[ -n "$mon_id" ]] || return 1

  mon_y="$(get_monitor_y_by_id "$mon_id")"
  is_integer "$mon_y" || return 1
  printf '%s\n' "$mon_y"
}

is_window_pinned() {
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
    if ! is_window_pinned "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  else
    if is_window_pinned "$addr"; then
      safe_hyprctl dispatch pin "address:${addr}" || return 1
    fi
  fi
}

save_visible_offset() {
  local addr=$1
  local win_y
  local mon_y
  local offset

  win_y="$(get_window_y "$addr")"
  mon_y="$(get_window_monitor_y "$addr")"
  if ! is_integer "$win_y" || ! is_integer "$mon_y"; then
    return 1
  fi

  offset=$((win_y - mon_y))
  if [[ $offset -lt 0 ]]; then
    offset=0
  fi
  printf '%s\n' "$offset" > "$VISIBLE_OFFSET_FILE"
}

get_visible_offset() {
  local offset="$DEFAULT_VISIBLE_OFFSET"
  if [[ -f "$VISIBLE_OFFSET_FILE" ]]; then
    offset="$(cat "$VISIBLE_OFFSET_FILE")"
  fi

  if ! is_integer "$offset" || [[ $offset -lt 0 ]]; then
    offset="$DEFAULT_VISIBLE_OFFSET"
  fi
  printf '%s\n' "$offset"
}

move_to_monitor_top() {
  local addr=$1
  local win_y
  local mon_y
  local dy

  win_y="$(get_window_y "$addr")"
  mon_y="$(get_window_monitor_y "$addr")"
  if ! is_integer "$win_y" || ! is_integer "$mon_y"; then
    return 1
  fi

  dy=$((mon_y - win_y))
  if [[ $dy -ne 0 ]]; then
    animate_vertical "$dy" "$addr"
  fi
}

restore_visible_position() {
  local addr=$1
  local offset
  local win_y
  local mon_y
  local target_y
  local dy

  offset="$(get_visible_offset)"
  win_y="$(get_window_y "$addr")"
  mon_y="$(get_window_monitor_y "$addr")"
  if ! is_integer "$offset" || ! is_integer "$win_y" || ! is_integer "$mon_y"; then
    return 1
  fi

  target_y=$((mon_y + offset))
  dy=$((target_y - win_y))
  if [[ $dy -ne 0 ]]; then
    animate_vertical "$dy" "$addr"
  fi
}

# 记录当前焦点窗口，便于收起时恢复
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

# 收起后恢复到展开前的焦点窗口
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
    rm -f "$PREV_FOCUS"
    rm -f "$PREV_WS"
    return 0
  fi

  if [[ -f "$PREV_WS" ]]; then
    prev_ws="$(cat "$PREV_WS")"
    current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty' 2>/dev/null)"
    if [[ -n "$prev_ws" && -n "$current_ws" && "$prev_ws" != "$current_ws" ]]; then
      rm -f "$PREV_FOCUS" "$PREV_WS"
      return 0
    fi
  fi

  if hyprctl clients -j 2>/dev/null | jq -e --arg addr "$prev_focus" 'any(.[]; .address == $addr)' >/dev/null 2>&1; then
    safe_hyprctl dispatch focuswindow "address:$prev_focus" || true
  fi

  rm -f "$PREV_FOCUS" "$PREV_WS"
}

# 切换 / 隐藏日历窗口
showcalendar() {
  local addr
  local current_ws
  
  # 检查当前窗口是否存在
  if ! getcalendarstatus; then
    echo "日历窗口不存在，无法切换状态" >&2
    return 1
  fi

  addr="$(get_calendar_address)"
  if [[ -z "$addr" ]]; then
    echo "无法获取日历窗口地址" >&2
    return 1
  fi

  if is_hidden_in_special; then
    # 已隐藏 → 显示到当前工作区，再下移回可见位置
    save_prev_focus
    current_ws="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.id // empty' 2>/dev/null)"
    if [[ -z "$current_ws" ]]; then
      echo "无法获取当前工作区" >&2
      return 1
    fi

    ensure_window_pinned_state "$addr" "off" || true
    if safe_hyprctl dispatch movetoworkspacesilent "${current_ws},address:${addr}" && \
       safe_hyprctl dispatch focuswindow "address:${addr}"; then
      if ! restore_visible_position "$addr"; then
        animate_vertical "${ANIM_OFFSET_FALLBACK}" "$addr"
      fi
      ensure_window_pinned_state "$addr" "$PIN_WHEN_VISIBLE" || true
      touch "$TOGGLE"
      echo "日历窗口已显示"
    else
      echo "显示日历窗口失败" >&2
      return 1
    fi
  else
    # 已显示 → 先上移，再隐藏到专用 special workspace，并恢复之前焦点
    save_visible_offset "$addr" || true
    if ! move_to_monitor_top "$addr"; then
      animate_vertical "$((-ANIM_OFFSET_FALLBACK))" "$addr"
    fi
    ensure_window_pinned_state "$addr" "off" || true
    if safe_hyprctl dispatch movetoworkspacesilent "${HIDDEN_WS},address:${addr}"; then
      rm -f "$TOGGLE"
      restore_prev_focus
      echo "日历窗口已隐藏"
    else
      echo "隐藏日历窗口失败" >&2
      return 1
    fi
  fi
}

# 主执行逻辑
main() {
  # 获取文件锁，避免并发执行
  if ! acquire_lock; then
    echo "无法获取锁，可能有其他实例在运行" >&2
    exit 1
  fi
  
  if getcalendarstatus; then
    # 已经有窗口 → 只切换显隐
    showcalendar
  else
    # 没有窗口 → 启动新实例
    echo "启动新的日历实例..."
    save_prev_focus
    
    # 确保启动前状态为隐藏
    rm -f "$TOGGLE"
    
    # 启动新实例
    nohup google-chrome-stable --app="https://calendar.google.com/calendar/u/0/r" \
            --user-data-dir="$HOME/.local/share/chrome-calendar" \
            --ignore-gpu-blocklist --enable-zero-copy \
            --enable-features=VaapiVideoDecodeLinuxGL \
            --ozone-platform-hint=auto --ozone-platform=wayland \
            --enable-features=TouchpadOverscrollHistoryNavigation \
            --enable-wayland-ime \
            >/dev/null 2>&1 &
    local chrome_pid=$!
    
    # 等待窗口出现
    echo "等待日历窗口启动..."
    if wait_for_window "true"; then
      echo "日历实例启动成功"
      # 新建窗口已经在显示位置，不做下移动画，只标记为可见
      local addr
      addr="$(get_calendar_address)"
      if [[ -n "$addr" ]]; then
        ensure_window_pinned_state "$addr" "off" || true
        ensure_window_pinned_state "$addr" "$PIN_WHEN_VISIBLE" || true
      fi
      touch "$TOGGLE"
    else
      echo "等待日历窗口出现超时" >&2
      # 清理失败的进程
      kill $chrome_pid 2>/dev/null || true
      exit 1
    fi
  fi
}

# 执行主逻辑
main "$@"
