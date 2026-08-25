#!/usr/bin/env bash

set -u

log_tag="hyprv-session-restore"

log() {
  systemd-cat -t "$log_tag" echo "$*"
}

has_window() {
  local jq_filter="$1"
  hyprctl -j clients 2>/dev/null | jq -e "$jq_filter" >/dev/null 2>&1
}

launch_if_missing() {
  local jq_filter="$1"
  shift

  if has_window "$jq_filter"; then
    return 0
  fi

  "$@" >/dev/null 2>&1 &
}

wait_for_window() {
  local jq_filter="$1"
  local timeout_seconds="${2:-30}"

  for _ in $(seq 1 "$timeout_seconds"); do
    if has_window "$jq_filter"; then
      return 0
    fi
    sleep 1
  done

  return 1
}

# Let the portal, keyring, status notifier, and desktop shell settle first.
sleep 3

# Workspace 2. Start each native scrolling column in deterministic order.
launch_if_missing 'any(.[]; .class == "discord")' discord
wait_for_window 'any(.[]; .class == "discord")' 30 || true
# Discord replaces its updater/login surface with the automatically logged-in window.
sleep 12

launch_if_missing 'any(.[]; .class == "QQ")' linuxqq
wait_for_window 'any(.[]; .class == "QQ")' 30 || true
# QQ likewise creates its main window only after automatic login completes.
sleep 12

launch_if_missing 'any(.[]; .class == "org.telegram.desktop")' Telegram
wait_for_window 'any(.[]; .class == "org.telegram.desktop")' 30 || true

wechat_started=0
if ! has_window 'any(.[]; .class == "wechat")'; then
  wechat >/dev/null 2>&1 &
  wechat_started=1
fi
wait_for_window 'any(.[]; .class == "wechat")' 30 || true

# WeChat's remembered-account screen has the login button focused by default.
# XSendEvent targets the XWayland client directly without activating workspace 2.
if ((wechat_started)); then
  for _ in $(seq 1 20); do
    window_id="$(xdotool search --onlyvisible --class '^wechat$' 2>/dev/null | head -n 1 || true)"
    if [[ -n "$window_id" ]]; then
      sleep 1
      xdotool key --window "$window_id" Return >/dev/null 2>&1 || true
      break
    fi
    sleep 1
  done
  # Give WeChat time to replace the remembered-account surface if necessary.
  sleep 5
fi

# Merge QQ below Discord and WeChat below Telegram. Explicit window targets let
# us update a hidden scrolling workspace; restoring the original workspace in
# the same Lua evaluation prevents an intermediate workspace from being drawn.
hyprctl eval '
  local original = hl.get_active_workspace()
  local ws = hl.get_workspace("2")
  if not ws then return end

  local discord = hl.get_windows({ workspace = ws, class = "discord" })[1]
  local qq = hl.get_windows({ workspace = ws, class = "QQ" })[1]
  local telegram = hl.get_windows({ workspace = ws, class = "org.telegram.desktop" })[1]
  local wechat = hl.get_windows({ workspace = ws, class = "wechat" })[1]

  if discord and qq and discord.layout and qq.layout and
      discord.layout.column.index ~= qq.layout.column.index then
    hl.dispatch(hl.dsp.window.move({ direction = "left", window = qq }))
  end

  if telegram and wechat and telegram.layout and wechat.layout and
      telegram.layout.column.index ~= wechat.layout.column.index then
    hl.dispatch(hl.dsp.window.move({ direction = "left", window = wechat }))
  end

  if original then
    hl.dispatch(hl.dsp.focus({ workspace = original }))
  end
' >/dev/null 2>&1 || log "could not assemble workspace 2 scrolling columns"

# Workspace 3.
launch_if_missing 'any(.[]; .class == "chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default")' \
  gtk-launch chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default
launch_if_missing 'any(.[]; .class == "msedge-_faolnafnngnfdaknnbpnkhgohbobgegn-Profile_1")' \
  gtk-launch msedge-faolnafnngnfdaknnbpnkhgohbobgegn-Profile_1
launch_if_missing 'any(.[]; .title == "飞书")' feishu

# Workspaces 4-6.
launch_if_missing 'any(.[]; .class == "zen")' zen-browser
launch_if_missing 'any(.[]; .class == "Chatgpt")' chatgpt
launch_if_missing 'any(.[]; .class == "code-oss")' code-oss
launch_if_missing 'any(.[]; .class == "Cider")' cider

log "desktop applications launched with silent workspace rules"
