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

  setsid -f -- "$@" >/dev/null 2>&1
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

ensure_window() {
  local name="$1"
  local jq_filter="$2"
  shift 2

  launch_if_missing "$jq_filter" "$@"
  if wait_for_window "$jq_filter" 30; then
    log "$name window is ready"
    return 0
  fi

  log "$name did not create a window; retrying"
  launch_if_missing "$jq_filter" "$@"
  if wait_for_window "$jq_filter" 30; then
    log "$name window is ready after retry"
    return 0
  fi

  log "$name failed to create a window"
  return 1
}

activate_status_notifier() {
  local wanted_id="$1"
  local item service path item_id

  while read -r item; do
    [[ "$item" == */* ]] || continue
    service="${item%%/*}"
    path="/${item#*/}"
    item_id="$(busctl --user get-property "$service" "$path" \
      org.kde.StatusNotifierItem Id 2>/dev/null || true)"
    if [[ "$item_id" == "s \"$wanted_id\"" ]]; then
      busctl --user call "$service" "$path" \
        org.kde.StatusNotifierItem Activate ii 0 0 >/dev/null 2>&1 || true
      return 0
    fi
  done < <(
    busctl --user get-property org.kde.StatusNotifierWatcher \
      /StatusNotifierWatcher org.kde.StatusNotifierWatcher \
      RegisteredStatusNotifierItems 2>/dev/null | tr ' ' '\n' | tr -d '"'
  )

  return 1
}

activate_status_notifier_until_window() {
  local notifier_id="$1"
  local jq_filter="$2"

  for _ in $(seq 1 30); do
    has_window "$jq_filter" && return 0
    activate_status_notifier "$notifier_id" || true
    sleep 1
  done

  return 1
}

wait_for_desktop_services() {
  for _ in $(seq 1 30); do
    if hyprctl -j monitors >/dev/null 2>&1 &&
        busctl --user status org.freedesktop.portal.Desktop >/dev/null 2>&1 &&
        busctl --user status org.freedesktop.secrets >/dev/null 2>&1 &&
        busctl --user status org.kde.StatusNotifierWatcher >/dev/null 2>&1; then
      # Registration can precede the backends becoming usable by a few seconds.
      sleep 5
      return 0
    fi
    sleep 1
  done

  log "desktop services did not all become ready; continuing cautiously"
}

# Avoid creating headless single-instance applications while the login
# services they depend on are still racing to initialize.
wait_for_desktop_services

# Start and verify every application in parallel. Window rules place them
# silently, and each missing window gets one independent retry.
ensure_window discord 'any(.[]; .class == "discord")' discord &
ensure_window qq 'any(.[]; .class == "QQ")' linuxqq &
ensure_window telegram 'any(.[]; .class == "org.telegram.desktop")' Telegram &

wechat_started=0
if ! has_window 'any(.[]; .class == "wechat")'; then
  wechat_started=1
fi

ensure_window wechat 'any(.[]; .class == "wechat")' wechat &

# Workspace 3.
ensure_window gmail 'any(.[]; .class == "chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default")' \
  gtk-launch chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default &
ensure_window outlook 'any(.[]; .class == "chrome-faolnafnngnfdaknnbpnkhgohbobgegn-Default")' \
  gtk-launch chrome-faolnafnngnfdaknnbpnkhgohbobgegn-Default &

# Feishu is particularly sensitive to the portal/keyring startup burst.
(sleep 10; ensure_window feishu \
  'any(.[]; .class == "feishu" or .class == "bytedance-feishu-stable")' feishu) &

# Workspaces 4-6.
ensure_window zen 'any(.[]; .class == "zen")' zen-browser &
# ChatGPT and Code share the Codex state database. Starting them together can
# make one fail its SQLite initialization, so only this dependent pair is
# sequenced while every other application continues in parallel.
(ensure_window chatgpt 'any(.[]; .class == "Chatgpt")' chatgpt
 ensure_window code-oss 'any(.[]; .class == "code-oss")' code-oss) &
ensure_window cider 'any(.[]; .class == "Cider")' cider \
  --enable-features=UseOzonePlatform \
  --ozone-platform=wayland \
  --enable-wayland-ime &

# Telegram and WeChat can restore directly into their tray-only state. Activate
# their notifier items to request the main windows without changing workspace.
(sleep 5; activate_status_notifier_until_window TelegramDesktop \
  'any(.[]; .class == "org.telegram.desktop")') &
(sleep 5; activate_status_notifier_until_window wechat \
  'any(.[]; .class == "wechat")') &

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

# Wait for the parallel verification jobs before arranging any workspace.
wait || true

# Discord and QQ replace their updater/login surfaces after automatic login.
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
      hl.get_windows({ workspace = ws3, class = "chrome-faolnafnngnfdaknnbpnkhgohbobgegn-Default" })[1],
      hl.get_windows({ workspace = ws3, class = "feishu" })[1] or
        hl.get_windows({ workspace = ws3, class = "bytedance-feishu-stable" })[1],
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

log "desktop application restore finished"
