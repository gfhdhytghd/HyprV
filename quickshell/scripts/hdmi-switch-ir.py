#!/usr/bin/env python3
"""Learn, inspect, and transmit HDMI-switch IR commands for Quickshell."""

from __future__ import annotations

import argparse
import datetime as dt
import glob
import json
import os
from pathlib import Path
import select
import sys
import termios
import time
import tty


DEFAULT_DATA = Path.home() / ".local/share/hyprv-ir/hdmi-switch.json"
SERIAL_PATTERNS = (
    "/dev/serial/by-id/*MicroPython*",
    "/dev/serial/by-id/*Raspberry_Pi_Pico*",
)


class Device:
    def __init__(self, path: str):
        self.path = path
        self.fd = os.open(path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        attrs = termios.tcgetattr(self.fd)
        tty.setraw(self.fd)
        attrs = termios.tcgetattr(self.fd)
        attrs[4] = termios.B115200
        attrs[5] = termios.B115200
        termios.tcsetattr(self.fd, termios.TCSANOW, attrs)
        self.buffer = b""

    def close(self) -> None:
        os.close(self.fd)

    def drain(self) -> None:
        while select.select([self.fd], [], [], 0)[0]:
            os.read(self.fd, 4096)

    def command(self, command: str, final_events: set[str], timeout: float):
        self.drain()
        os.write(self.fd, command.encode() + b"\n")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], min(0.25, deadline - time.monotonic()))
            if not ready:
                continue
            chunk = os.read(self.fd, 4096)
            if not chunk:
                continue
            self.buffer += chunk
            while b"\n" in self.buffer:
                raw, self.buffer = self.buffer.split(b"\n", 1)
                raw = raw.strip()
                if not raw:
                    continue
                try:
                    message = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                event = message.get("event")
                if event == "error":
                    raise RuntimeError(message.get("message", "device error"))
                if event in final_events:
                    return message
        raise TimeoutError(f"Pico did not answer within {timeout:.0f}s")


def find_device(explicit: str | None) -> str:
    if explicit:
        return explicit
    matches = []
    for pattern in SERIAL_PATTERNS:
        matches.extend(glob.glob(pattern))
    matches = sorted(set(matches))
    if len(matches) == 1:
        return matches[0]
    if not matches:
        raise RuntimeError("MicroPython Pico serial device not found")
    raise RuntimeError("multiple Pico serial devices found; pass --device")


def near(actual: int, expected: int, tolerance: float = 0.35) -> bool:
    return expected * (1 - tolerance) <= actual <= expected * (1 + tolerance)


def single_frame(durations: list[int]) -> list[int]:
    """Drop an inter-frame gap and any repeat frame that follows it."""
    for index in range(1, len(durations), 2):
        if durations[index] > 10000:
            return durations[:index]
    return durations


def decode_nec(durations: list[int]):
    if len(durations) < 66 or not near(durations[0], 9000) or not near(durations[1], 4500):
        return None
    bits = []
    for offset in range(2, 66, 2):
        mark, space = durations[offset : offset + 2]
        if not near(mark, 560):
            return None
        if near(space, 560):
            bits.append(0)
        elif near(space, 1690):
            bits.append(1)
        else:
            return None
    octets = []
    for start in range(0, 32, 8):
        value = sum(bits[start + bit] << bit for bit in range(8))
        octets.append(value)
    address, address_inverse, command, command_inverse = octets
    return {
        "protocol": "NEC",
        "bytes_hex": [f"0x{value:02X}" for value in octets],
        "address": address,
        "command": command,
        "address_inverse_valid": address ^ address_inverse == 0xFF,
        "command_inverse_valid": command ^ command_inverse == 0xFF,
    }


def load_database(path: Path):
    if not path.exists():
        return {"schema": 1, "carrier_hz": 38000, "duty_cycle": 0.33, "buttons": {}}
    return json.loads(path.read_text())


def save_database(path: Path, database) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(database, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def print_json(value) -> None:
    print(json.dumps(value, ensure_ascii=False, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device", help="Pico serial path (auto-detected by default)")
    parser.add_argument("--data", type=Path, default=DEFAULT_DATA)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("status")
    scan_parser = subparsers.add_parser("scan", help="find which GPIO has the receiver output")
    scan_parser.add_argument("--seconds", type=float, default=5)
    pin_parser = subparsers.add_parser("set-pin")
    pin_parser.add_argument("pin", type=int)
    learn_parser = subparsers.add_parser("learn", help="capture and name one remote button")
    learn_parser.add_argument("name")
    learn_parser.add_argument("--force", action="store_true")
    subparsers.add_parser("list")
    show_parser = subparsers.add_parser("show")
    show_parser.add_argument("name")
    send_parser = subparsers.add_parser("send", help="transmit one learned button once")
    send_parser.add_argument("name")
    args = parser.parse_args()

    if args.command in {"list", "show"}:
        database = load_database(args.data)
        if args.command == "list":
            for name in sorted(database["buttons"]):
                decoded = database["buttons"][name].get("decoded")
                suffix = f"  {decoded['protocol']} command={decoded['command']}" if decoded else "  raw"
                print(name + suffix)
            return 0
        try:
            print_json(database["buttons"][args.name])
        except KeyError:
            raise RuntimeError(f"button not found: {args.name}")
        return 0

    device = Device(find_device(args.device))
    try:
        if args.command == "status":
            print_json(device.command("status", {"status"}, 2))
        elif args.command == "scan":
            milliseconds = max(1000, int(args.seconds * 1000))
            print(f"在接收头前持续按遥控器按键 {milliseconds / 1000:g} 秒…", file=sys.stderr)
            result = device.command(f"scan {milliseconds}", {"scan"}, args.seconds + 3)
            print_json(result)
        elif args.command == "set-pin":
            print_json(device.command(f"pin {args.pin}", {"pin"}, 2))
        elif args.command == "learn":
            database = load_database(args.data)
            if args.name in database["buttons"] and not args.force:
                raise RuntimeError(f"{args.name!r} already exists; use --force to replace it")
            print(f"请对准接收头，按一次遥控器的 {args.name!r} 键…", file=sys.stderr)
            capture = device.command("capture 10000", {"capture", "capture_timeout"}, 13)
            if capture["event"] == "capture_timeout":
                raise RuntimeError("10 秒内没有收到红外信号")
            durations = single_frame(capture["durations_us"])
            entry = {
                "captured_at": dt.datetime.now(dt.timezone.utc).astimezone().isoformat(),
                "source_pin": capture["pin"],
                "polarity": capture["polarity"],
                "durations_us": durations,
                "decoded": decode_nec(durations),
            }
            database["buttons"][args.name] = entry
            save_database(args.data, database)
            print_json(entry)
            print(f"已保存到 {args.data}", file=sys.stderr)
        elif args.command == "send":
            database = load_database(args.data)
            try:
                entry = database["buttons"][args.name]
            except KeyError:
                raise RuntimeError(f"button not found: {args.name}")
            carrier_hz = int(database.get("carrier_hz", 38000))
            duty_x10000 = round(float(database.get("duty_cycle", 0.33)) * 10000)
            durations = ",".join(str(value) for value in entry["durations_us"])
            result = device.command(
                f"transmit {carrier_hz} {duty_x10000} {durations}",
                {"transmit"},
                5,
            )
            result["button"] = args.name
            print_json(result)
    finally:
        device.close()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, TimeoutError, ValueError) as error:
        print(f"irctl: {error}", file=sys.stderr)
        raise SystemExit(1)
