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

install_restore_rules() {
  hyprctl eval '
    if _G.hyprv_session_restore_rules then
      for _, rule in ipairs(_G.hyprv_session_restore_rules) do
        rule:set_enabled(false)
      end
    end

    local function slot(name, match, workspace, width)
      return hl.window_rule({
        name = "session-restore-" .. name,
        match = match,
        workspace = workspace .. " silent",
        no_initial_focus = true,
        tile = true,
        scrolling_width = width,
      })
    end

    _G.hyprv_session_restore_rules = {
      slot("thunderbird", { class = "(?i)^(org\\.mozilla\\.thunderbird|thunderbird)$" }, "3", 0.85),
      slot("feishu-class", { class = "^(feishu|bytedance-feishu-stable)$" }, "3", 0.75),
      slot("feishu-title", { title = "^(飞书)$" }, "3", 0.75),
      slot("zen", { class = "^(zen)$" }, "4", 0.85),
      slot("chatgpt", { class = "(?i)^(chatgpt)$" }, "5", 0.85),
      slot("code-oss", { class = "^(code-oss)$" }, "5", 0.90),
      slot("cider", { class = "^(Cider)$" }, "6", 0.85),
    }
  ' >/dev/null 2>&1
}

remove_restore_rules() {
  hyprctl eval '
    if _G.hyprv_session_restore_rules then
      for _, rule in ipairs(_G.hyprv_session_restore_rules) do
        rule:set_enabled(false)
      end
      _G.hyprv_session_restore_rules = nil
    end
  ' >/dev/null 2>&1 || true
}

# Avoid creating headless single-instance applications while the login
# services they depend on are still racing to initialize.
wait_for_desktop_services

if ! install_restore_rules; then
  log "could not install temporary session restore rules"
fi
trap remove_restore_rules EXIT

# Start and verify every application in parallel. Window rules place them
# silently, and each missing window gets one independent retry.
# Laptop: QQ, WeChat, Telegram and Discord are launched manually.

# Workspace 3: Thunderbird on the left, Feishu on the right.
# Sequence window creation so scrolling columns follow the current layout.
(ensure_window thunderbird 'any(.[]; .class | ascii_downcase | IN("thunderbird", "org.mozilla.thunderbird"))' thunderbird
 sleep 10
 ensure_window feishu \
   'any(.[]; .class == "feishu" or .class == "bytedance-feishu-stable" or .title == "飞书")' feishu) &

# Workspaces 4-6.
ensure_window zen 'any(.[]; .class == "zen")' "$HOME/.config/HyprV/hypr/scripts/browser-wl-relabel.sh" zen-browser &
# ChatGPT starts after a short delay, then ChatGPT and Code remain sequenced
# because they share the Codex state database and can race during SQLite init.
(sleep 3
 ensure_window chatgpt 'any(.[]; (.class | ascii_downcase) == "chatgpt")' chatgpt
 ensure_window code-oss 'any(.[]; .class == "code-oss")' code-oss) &
ensure_window cider 'any(.[]; .class == "Cider")' cider \
  --enable-features=UseOzonePlatform \
  --ozone-platform=wayland \
  --enable-wayland-ime &

# Wait until all application groups have finished checking their windows.
wait || true

# Ordered launches above establish the scrolling column order without
# focusing or rearranging the user's active desktop.

log "desktop application restore finished"
