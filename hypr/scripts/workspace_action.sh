#!/usr/bin/env bash
set -u

action=${1:-}
requested=${2:-}

if [[ -z "$action" || ! "$requested" =~ ^-?[0-9]+$ ]]; then
  printf 'usage: %s <workspace|movetoworkspace> <number>\n' "$0" >&2
  exit 1
fi

current=$(hyprctl activeworkspace -j | jq -r .id)
target=$(((((current - 1) / 10) * 10) + requested))

case "$action" in
  workspace)
    lua_dispatch="hl.dsp.focus({ workspace = $target })"
    ;;
  movetoworkspace)
    lua_dispatch="hl.dsp.window.move({ workspace = $target })"
    ;;
  *)
    lua_dispatch=""
    ;;
esac

if [[ -n "$lua_dispatch" ]]; then
  lua_output=$(hyprctl dispatch "$lua_dispatch" 2>&1)
  if [[ $lua_output == "ok" ]]; then
    printf '%s\n' "$lua_output"
    exit 0
  fi
fi

hyprctl dispatch "$action" "$target"
