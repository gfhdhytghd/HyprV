#!/usr/bin/env python3

import argparse
from pathlib import Path


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


def normalized(value):
    return " ".join((value or "").casefold().split())


def artist_matches(candidate, requested):
    candidate = normalized(candidate)
    requested = normalized(requested)
    if not requested:
        return True
    return candidate == requested or candidate in requested or requested in candidate


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


def find_matching_title(artist, album, target_ms, current_title):
    duration_pattern = key_pattern("durationInMillis", b"I")
    album_key = normalized(album)
    current_key = normalized(current_title)
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

            if not title or normalized(candidate_album) != album_key:
                offset = index + 1
                continue
            if not artist_matches(candidate_artist, artist):
                offset = index + 1
                continue

            score = abs(duration_ms - target_ms)
            if normalized(title) != current_key:
                score -= 1
            matches.append((score, title))
            offset = index + 1

    if not matches:
        return ""

    if any(normalized(title) == current_key for _, title in matches):
        return ""

    matches.sort(key=lambda item: item[0])
    return matches[0][1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artist", default="")
    parser.add_argument("--album", default="")
    parser.add_argument("--length", default="0")
    parser.add_argument("--current-title", default="")
    args = parser.parse_args()

    try:
        target_ms = round(float(args.length) * 1000)
    except ValueError:
        return 1

    if target_ms <= 0 or not args.album:
        return 1

    title = find_matching_title(args.artist, args.album, target_ms, args.current_title)
    if title and normalized(title) != normalized(args.current_title):
        print(title)
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
