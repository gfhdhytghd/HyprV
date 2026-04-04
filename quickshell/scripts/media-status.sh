#!/usr/bin/env bash

set -euo pipefail

if ! command -v playerctl >/dev/null 2>&1; then
    printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\n'
    exit 1
fi

selected_player=""
fallback_player=""

while IFS= read -r player; do
    [[ -z "$player" ]] && continue
    if [[ -z "$fallback_player" ]]; then
        fallback_player="$player"
    fi
    status="$(playerctl -p "$player" status 2>/dev/null || true)"
    if [[ "$status" == "Playing" ]]; then
        selected_player="$player"
        break
    fi
done < <(playerctl -l 2>/dev/null || true)

if [[ -z "$selected_player" ]]; then
    selected_player="$fallback_player"
fi

if [[ -z "$selected_player" ]]; then
    printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\n'
    exit 1
fi

status="$(playerctl -p "$selected_player" status 2>/dev/null || true)"
title="$(playerctl -p "$selected_player" metadata xesam:title 2>/dev/null | head -n 1 || true)"
artist="$(playerctl -p "$selected_player" metadata xesam:artist 2>/dev/null | paste -sd ', ' - || true)"
art_url="$(playerctl -p "$selected_player" metadata mpris:artUrl 2>/dev/null | head -n 1 || true)"

printf 'available=true\n'
printf 'playing=%s\n' "$([[ "$status" == "Playing" ]] && printf true || printf false)"
printf 'title=%s\n' "${title//$'\n'/ }"
printf 'artist=%s\n' "${artist//$'\n'/ }"
printf 'player=%s\n' "${selected_player//$'\n'/ }"
printf 'art_url=%s\n' "${art_url//$'\n'/ }"
