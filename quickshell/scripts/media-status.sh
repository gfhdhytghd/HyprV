#!/usr/bin/env bash

set -euo pipefail

if ! command -v playerctl >/dev/null 2>&1; then
    printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\nposition=0\nlength=0\n'
    exit 1
fi

selected_player=""
fallback_player=""
content_player=""
paused_content_player=""
paused_player=""

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
    if [[ -z "$paused_player" && "$status" == "Paused" ]]; then
        paused_player="$player"
    fi
    if [[ -z "$content_player" ]]; then
        candidate_title="$(playerctl -p "$player" metadata xesam:title 2>/dev/null | head -n 1 || true)"
        candidate_artist="$(playerctl -p "$player" metadata xesam:artist 2>/dev/null | paste -sd ', ' - || true)"
        candidate_art_url="$(playerctl -p "$player" metadata mpris:artUrl 2>/dev/null | head -n 1 || true)"
        if [[ -n "$candidate_title" || -n "$candidate_artist" || -n "$candidate_art_url" ]]; then
            content_player="$player"
            if [[ -z "$paused_content_player" && "$status" == "Paused" ]]; then
                paused_content_player="$player"
            fi
        fi
    fi
done < <(playerctl -l 2>/dev/null || true)

if [[ -z "$selected_player" ]]; then
    selected_player="${paused_content_player:-${content_player:-${paused_player:-$fallback_player}}}"
fi

if [[ -z "$selected_player" ]]; then
    printf 'available=false\nplaying=false\ntitle=\nartist=\nplayer=\nart_url=\nposition=0\nlength=0\n'
    exit 1
fi

status="$(playerctl -p "$selected_player" status 2>/dev/null || true)"
title="$(playerctl -p "$selected_player" metadata xesam:title 2>/dev/null | head -n 1 || true)"
artist="$(playerctl -p "$selected_player" metadata xesam:artist 2>/dev/null | paste -sd ', ' - || true)"
album="$(playerctl -p "$selected_player" metadata xesam:album 2>/dev/null | head -n 1 || true)"
art_url="$(playerctl -p "$selected_player" metadata mpris:artUrl 2>/dev/null | head -n 1 || true)"
position="$(playerctl -p "$selected_player" position 2>/dev/null | head -n 1 || true)"
length_us="$(playerctl -p "$selected_player" metadata mpris:length 2>/dev/null | head -n 1 || true)"

if [[ ! "$position" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    position="0"
fi

length="0"
if [[ "$length_us" =~ ^[0-9]+$ ]]; then
    length="$(awk -v value="$length_us" 'BEGIN { printf "%.3f", value / 1000000 }')"
fi

player_comm=""
if [[ "$selected_player" == chromium.instance* ]] && command -v busctl >/dev/null 2>&1; then
    player_comm="$(busctl --user status "org.mpris.MediaPlayer2.${selected_player}" 2>/dev/null | sed -n 's/^Comm=//p' | head -n 1 || true)"
fi

if [[ "$player_comm" == "Cider" && "$length" != "0" && -n "$album" ]]; then
    script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    fallback_title="$("$script_dir/cider-metadata-fallback.py" --artist "$artist" --album "$album" --length "$length" --current-title "$title" 2>/dev/null || true)"
    if [[ -n "$fallback_title" ]]; then
        title="$fallback_title"
    fi
fi

printf 'available=true\n'
printf 'playing=%s\n' "$([[ "$status" == "Playing" ]] && printf true || printf false)"
printf 'title=%s\n' "${title//$'\n'/ }"
printf 'artist=%s\n' "${artist//$'\n'/ }"
printf 'player=%s\n' "${selected_player//$'\n'/ }"
printf 'art_url=%s\n' "${art_url//$'\n'/ }"
printf 'position=%s\n' "$position"
printf 'length=%s\n' "$length"
