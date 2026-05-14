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

if [[ "$player_comm" == "Cider" && "$length" != "0" ]]; then
    script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    fallback_output="$("$script_dir/cider-metadata-fallback.py" --artist "$artist" --album "$album" --length "$length" --current-title "$title" --shell 2>/dev/null || true)"
    fallback_title=""
    fallback_artist=""
    fallback_album=""
    fallback_art_url=""
    fallback_source=""

    while IFS= read -r line; do
        [[ "$line" == *=* ]] || continue
        key="${line%%=*}"
        value="${line#*=}"
        case "$key" in
            title) fallback_title="$value" ;;
            artist) fallback_artist="$value" ;;
            album) fallback_album="$value" ;;
            art_url) fallback_art_url="$value" ;;
            source) fallback_source="$value" ;;
        esac
    done <<< "$fallback_output"

    if [[ "$fallback_source" == "api" ]]; then
        [[ -n "$fallback_title" ]] && title="$fallback_title"
        [[ -n "$fallback_artist" ]] && artist="$fallback_artist"
        [[ -n "$fallback_album" ]] && album="$fallback_album"
        [[ -n "$fallback_art_url" ]] && art_url="$fallback_art_url"
    elif [[ -n "$fallback_title" && ( -z "$title" || "$title" == "Cider" || "$title" == "Chromium" ) ]]; then
        title="$fallback_title"
    fi
    if [[ "$fallback_source" != "api" && -n "$fallback_artist" && -z "$artist" ]]; then
        artist="$fallback_artist"
    fi
    if [[ "$fallback_source" != "api" && -n "$fallback_album" && -z "$album" ]]; then
        album="$fallback_album"
    fi
    if [[ "$fallback_source" != "api" && -n "$fallback_art_url" && -z "$art_url" ]]; then
        art_url="$fallback_art_url"
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
