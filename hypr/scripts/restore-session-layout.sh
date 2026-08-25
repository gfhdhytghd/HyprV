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

# Start every application first. Window rules place them silently while the
# login-dependent workspace 2 clients settle in parallel.
launch_if_missing 'any(.[]; .class == "discord")' discord
launch_if_missing 'any(.[]; .class == "QQ")' linuxqq
launch_if_missing 'any(.[]; .class == "org.telegram.desktop")' Telegram

wechat_started=0
if ! has_window 'any(.[]; .class == "wechat")'; then
  wechat >/dev/null 2>&1 &
  wechat_started=1
fi

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

# Discord and QQ replace their updater/login surfaces after automatic login.
# These waits happen after every application has already been launched.
wait_for_window '
  any(.[]; .class == "discord") and
  any(.[]; .class == "QQ") and
  any(.[]; .class == "org.telegram.desktop") and
  any(.[]; .class == "wechat")
' 30 || true
sleep 12

# Assemble the hidden scrolling workspaces after every launch has settled.
# Explicit window targets avoid focusing those workspaces while they are sorted.
hyprctl eval '
  local original = hl.get_active_workspace()
  local ws2 = hl.get_workspace("2")

  if ws2 then
    local discord = hl.get_windows({ workspace = ws2, class = "discord" })[1]
    local qq = hl.get_windows({ workspace = ws2, class = "QQ" })[1]
    local telegram = hl.get_windows({ workspace = ws2, class = "org.telegram.desktop" })[1]
    local wechat = hl.get_windows({ workspace = ws2, class = "wechat" })[1]

    if discord and qq and discord.layout and qq.layout and
        discord.layout.column.index ~= qq.layout.column.index then
      hl.dispatch(hl.dsp.window.move({ direction = "left", window = qq }))
    end

    if telegram and wechat and telegram.layout and wechat.layout and
        telegram.layout.column.index ~= wechat.layout.column.index then
      hl.dispatch(hl.dsp.window.move({ direction = "left", window = wechat }))
    end
  end

  local ws3 = hl.get_workspace("3")
  if ws3 then
    local ordered = {
      hl.get_windows({ workspace = ws3, class = "chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default" })[1],
      hl.get_windows({ workspace = ws3, class = "msedge-_faolnafnngnfdaknnbpnkhgohbobgegn-Profile_1" })[1],
      hl.get_windows({ workspace = ws3, title = "飞书" })[1],
    }

    for target = 0, 2 do
      local wanted = ordered[target + 1]
      if wanted and wanted.layout and wanted.layout.column.index ~= target then
        local occupying
        for _, candidate in ipairs(ordered) do
          if candidate and candidate.layout and candidate.layout.column.index == target then
            occupying = candidate
            break
          end
        end
        if occupying then
          hl.dispatch(hl.dsp.window.swap({ window = wanted, other = occupying }))
        end
      end
    end
  end

  if original then
    hl.dispatch(hl.dsp.focus({ workspace = original }))
  end
' >/dev/null 2>&1 || log "could not assemble scrolling workspaces"

log "desktop applications launched with silent workspace rules"
