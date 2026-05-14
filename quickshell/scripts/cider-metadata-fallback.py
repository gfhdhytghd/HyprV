#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


CIDER_API_BASE = os.environ.get("HYPRV_CIDER_API_BASE", "http://127.0.0.1:10767")
CIDER_API_TIMEOUT = 0.7


def encode_varint(value):
    output = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            output.append(byte | 0x80)
        else:
            output.append(byte)
            return bytes(output)


def read_varint(data, offset):
    value = 0
    shift = 0
    for index in range(10):
        if offset + index >= len(data):
            return None, offset + index
        byte = data[offset + index]
        value |= (byte & 0x7F) << shift
        if byte & 0x80 == 0:
            return value, offset + index + 1
        shift += 7
    return None, offset + 10


def key_pattern(key, value_tag):
    encoded = key.encode("utf-8")
    return b'"' + encode_varint(len(encoded)) + encoded + value_tag


def read_string_field(window, key):
    pattern = key_pattern(key, b'"')
    index = window.find(pattern)
    if index < 0:
        return ""

    length, value_offset = read_varint(window, index + len(pattern))
    if length is None or length < 0 or length > 4096 or value_offset + length > len(window):
        return ""

    try:
        return window[value_offset:value_offset + length].decode("utf-8")
    except UnicodeDecodeError:
        return ""


def read_integer_field(window, key):
    pattern = key_pattern(key, b"I")
    index = window.find(pattern)
    if index < 0:
        return None

    raw_value, _ = read_varint(window, index + len(pattern))
    if raw_value is None:
        return None

    return raw_value >> 1


def single_line(value):
    return " ".join(str(value or "").replace("\r", " ").replace("\n", " ").split())


def normalized(value):
    return " ".join((value or "").casefold().split())


def artist_matches(candidate, requested):
    candidate = normalized(candidate)
    requested = normalized(requested)
    if not requested:
        return True
    return candidate == requested or candidate in requested or requested in candidate


def is_generic_title(value):
    return normalized(value) in ("", "cider", "chromium")


def format_artwork_url(value):
    if not value:
        return ""
    url = str(value)
    replacements = {
        "{w}": "512",
        "{h}": "512",
        "{f}": "jpg",
        "{c}": "bb",
    }
    for key, replacement in replacements.items():
        url = url.replace(key, replacement)
    if url.startswith("//"):
        url = f"https:{url}"
    elif url.startswith("/"):
        url = f"{CIDER_API_BASE.rstrip('/')}{url}"
    return url


def maybe_number(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def track_from_mapping(data):
    if not isinstance(data, dict):
        return {}

    attributes = data.get("attributes") if isinstance(data.get("attributes"), dict) else {}
    source = attributes if attributes else data

    artwork = source.get("artwork")
    if isinstance(artwork, dict):
        artwork = artwork.get("url")
    elif not artwork and isinstance(data.get("artwork"), dict):
        artwork = data["artwork"].get("url")

    duration = (
        source.get("durationInMillis")
        or source.get("duration_ms")
        or source.get("durationMs")
        or source.get("duration")
    )
    duration_number = maybe_number(duration)
    if duration_number is not None and 0 < duration_number < 10000:
        duration_number *= 1000

    return {
        "title": single_line(
            source.get("name")
            or source.get("title")
            or data.get("name")
            or data.get("title")
        ),
        "artist": single_line(
            source.get("artistName")
            or source.get("artist")
            or data.get("artistName")
            or data.get("artist")
        ),
        "album": single_line(
            source.get("albumName")
            or source.get("album")
            or data.get("albumName")
            or data.get("album")
        ),
        "art_url": format_artwork_url(
            artwork
            or source.get("artworkUrl")
            or source.get("artworkURL")
            or data.get("artworkUrl")
            or data.get("artworkURL")
        ),
        "duration_ms": round(duration_number) if duration_number is not None else None,
    }


def iter_candidate_mappings(value):
    if isinstance(value, dict):
        yield value
        for item in value.values():
            yield from iter_candidate_mappings(item)
    elif isinstance(value, list):
        for item in value:
            yield from iter_candidate_mappings(item)


def usable_track(track):
    return bool(track.get("title") or track.get("artist") or track.get("album") or track.get("art_url"))


def requested_track_score(track, artist, album, target_ms, current_title, source_rank):
    score = source_rank
    duration_ms = track.get("duration_ms")
    if target_ms > 0 and duration_ms:
        delta = abs(int(duration_ms) - target_ms)
        if delta > 2500:
            return None
        score += delta
    elif target_ms > 0:
        score += 5000

    if artist and not artist_matches(track.get("artist") or "", artist):
        return None
    if album and normalized(track.get("album")) != normalized(album):
        return None

    if current_title and not is_generic_title(current_title):
        if normalized(track.get("title")) == normalized(current_title):
            score -= 3000
        else:
            score += 1200

    if track.get("title"):
        score -= 100
    if track.get("artist"):
        score -= 80
    if track.get("art_url"):
        score -= 60
    return score


def read_token_file(path):
    try:
        text = Path(path).expanduser().read_text(encoding="utf-8").strip()
    except OSError:
        return ""
    return text.splitlines()[0].strip()


def cider_api_tokens():
    tokens = []
    for key in ("HYPRV_CIDER_APP_TOKEN", "CIDER_APP_TOKEN"):
        value = os.environ.get(key, "").strip()
        if value:
            tokens.append(value)
    for path in (
        "~/.config/hyprv/cider-app-token",
        "~/.config/HyprV/.cider-app-token",
    ):
        value = read_token_file(path)
        if value:
            tokens.append(value)
    unique = []
    for token in tokens:
        if token not in unique:
            unique.append(token)
    return unique


def fetch_json(url, token):
    headers = {"User-Agent": "HyprV"}
    if token:
        headers["apptoken"] = token
    request = Request(url, headers=headers)
    with urlopen(request, timeout=CIDER_API_TIMEOUT) as response:
        return json.loads(response.read().decode("utf-8", errors="replace"))


def find_api_track(artist, album, target_ms, current_title):
    endpoints = (
        "/api/v2/playback/now-playing",
        "/api/v1/playback/now-playing",
    )
    attempts = [""]
    attempts.extend(cider_api_tokens())
    matches = []

    for endpoint in endpoints:
        url = f"{CIDER_API_BASE.rstrip('/')}{endpoint}"
        for token in attempts:
            try:
                payload = fetch_json(url, token)
            except (HTTPError, URLError, TimeoutError, OSError, ValueError):
                continue
            if isinstance(payload, dict) and isinstance(payload.get("data"), dict):
                track = track_from_mapping(payload["data"])
                if usable_track(track):
                    track["source"] = "api"
                    return track
            for index, mapping in enumerate(iter_candidate_mappings(payload)):
                track = track_from_mapping(mapping)
                if not usable_track(track):
                    continue
                score = requested_track_score(track, "", "", target_ms, current_title, index)
                if score is not None:
                    track["source"] = "api"
                    matches.append((score, track))
            if matches:
                break
        if matches:
            break

    if not matches:
        return {}
    matches.sort(key=lambda item: item[0])
    return matches[0][1]


def iter_indexeddb_files():
    base = Path.home() / ".config" / "sh.cider.genten" / "IndexedDB"
    for name in (
        "http_127.0.0.1_10767.indexeddb.leveldb",
        "http_localhost_10767.indexeddb.leveldb",
    ):
        root = base / name
        if not root.exists():
            continue
        for path in root.iterdir():
            if path.is_file() and path.suffix in (".log", ".ldb"):
                yield path


def find_cache_track(artist, album, target_ms, current_title):
    duration_pattern = key_pattern("durationInMillis", b"I")
    matches = []

    for path in iter_indexeddb_files():
        try:
            data = path.read_bytes()
        except OSError:
            continue

        offset = 0
        while True:
            index = data.find(duration_pattern, offset)
            if index < 0:
                break

            window = data[max(0, index - 1400):min(len(data), index + 1800)]
            duration_ms = read_integer_field(window, "durationInMillis")
            if duration_ms is None or abs(duration_ms - target_ms) > 1500:
                offset = index + 1
                continue

            title = read_string_field(window, "name")
            candidate_artist = read_string_field(window, "artistName")
            candidate_album = read_string_field(window, "albumName")
            art_url = (
                read_string_field(window, "artwork")
                or read_string_field(window, "artworkUrl")
                or read_string_field(window, "artworkURL")
            )

            track = {
                "title": single_line(title),
                "artist": single_line(candidate_artist),
                "album": single_line(candidate_album),
                "art_url": format_artwork_url(art_url),
                "duration_ms": duration_ms,
                "source": "cache",
            }
            if not usable_track(track):
                offset = index + 1
                continue

            score = requested_track_score(track, artist, album, target_ms, current_title, 0)
            if score is not None:
                try:
                    mtime = path.stat().st_mtime
                except OSError:
                    mtime = 0
                matches.append((score, -mtime, -index, track))
            offset = index + 1

    if not matches:
        return {}

    matches.sort(key=lambda item: (item[0], item[1], item[2]))
    return matches[0][3]


def find_matching_track(artist, album, target_ms, current_title):
    api_track = find_api_track(artist, album, target_ms, current_title)
    if api_track:
        return api_track
    return find_cache_track(artist, album, target_ms, current_title)


def find_matching_title(artist, album, target_ms, current_title):
    track = find_matching_track(artist, album, target_ms, current_title)
    title = track.get("title") if track else ""
    if title and normalized(title) != normalized(current_title):
        return title
    return ""


def print_shell(track):
    for key in ("title", "artist", "album", "art_url", "source"):
        value = track.get(key) or ""
        print(f"{key}={single_line(value)}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artist", default="")
    parser.add_argument("--album", default="")
    parser.add_argument("--length", default="0")
    parser.add_argument("--current-title", default="")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--shell", action="store_true")
    args = parser.parse_args()

    try:
        target_ms = round(float(args.length) * 1000)
    except ValueError:
        return 1

    if target_ms <= 0:
        return 1

    track = find_matching_track(args.artist, args.album, target_ms, args.current_title)
    if not track:
        return 1

    if args.json:
        print(json.dumps(track, ensure_ascii=False, separators=(",", ":")))
        return 0
    if args.shell:
        print_shell(track)
        return 0

    title = track.get("title") or ""
    if title and normalized(title) != normalized(args.current_title):
        print(title)
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
