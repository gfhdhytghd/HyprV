#!/usr/bin/env bash
# Launch-only helper. Window state and waiting belong to hypr/panels.lua.
set -euo pipefail
APP_LAUNCH_LOG=/tmp/sidechat-ai-hub.log

find_ai_hub_checkout() {
  local candidate
  for candidate in \
    "${AI_HUB_REPO:-}" \
    "$HOME/data/AIHub" \
    "$HOME/AIHub" \
    "$HOME/src/AIHub"; do
    [[ -n "$candidate" ]] || continue
    if [[ -f "$candidate/scripts/aihub-firefox.js" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

start_sidechat_app() {
  local checkout

  if [[ -n "${AI_HUB_COMMAND:-}" ]]; then
    nohup bash -lc "$AI_HUB_COMMAND" >>"$APP_LAUNCH_LOG" 2>&1 &
    printf '%s\n' "$!"
    return 0
  fi

  checkout="$(find_ai_hub_checkout || true)"
  if [[ -n "$checkout" ]]; then
    nohup node "$checkout/scripts/aihub-firefox.js" >>"$APP_LAUNCH_LOG" 2>&1 &
    printf '%s\n' "$!"
    return 0
  fi

  if command -v ai-hub >/dev/null 2>&1; then
    nohup ai-hub >>"$APP_LAUNCH_LOG" 2>&1 &
    printf '%s\n' "$!"
    return 0
  fi

  echo "Cannot find AI Hub launcher. Set AI_HUB_COMMAND or install ai-hub." >&2
  return 1
}

case "${1:-}" in
  dropdown)
    nohup kitty --class kitty-dropdown >/dev/null 2>&1 &
    pid=$!
    ;;
  sidechat) pid="$(start_sidechat_app)" ;;
  *) exit 2 ;;
esac
printf '%s\n' "$pid" > "$2"
