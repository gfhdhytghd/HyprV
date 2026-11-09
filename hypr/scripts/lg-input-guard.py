#!/usr/bin/env python3
"""Turn the desktop LG UltraFine 6K off while it is not showing this laptop.

After the LG is attached, if its input is not this machine within 15 s, DPMS it off.
Once it has shown another input for more than 5 min, disable the output (windows move
to the internal panel). As soon as the LG is switched back to this machine's input,
re-enable it and turn DPMS on. DDC/CI keeps answering while the output is off or
disabled, so the input is polled over DDC every few seconds while the LG is attached.
"""
import fcntl
import glob
import json
import os
import subprocess
import time

SERIAL = b"601INXJ74394"
MINE = 0x0F  # VCP 0x60 value while the LG shows this laptop (USB-C/TB, reported as DP-1)
POLL = 3
GRACE = 15
DETACH = 300
RECHECK = 10  # polls between Hyprland reconciliations while nothing changes
I2C_SLAVE = 0x0703
MONITORS_LUA = os.path.expanduser("~/.config/HyprV/hypr/hyprland-monitors.lua")


def find_lg():
    """(output name, i2c bus) of the attached LG, from DRM connector EDIDs."""
    for conn in glob.glob("/sys/class/drm/card*-DP-*"):
        try:
            if open(conn + "/status").read().strip() != "connected":
                continue
            if SERIAL not in open(conn + "/edid", "rb").read():
                continue
            bus = os.path.basename(os.path.realpath(conn + "/ddc"))
        except OSError:
            continue
        return conn.split("-", 1)[1], int(bus.removeprefix("i2c-"))
    return None


def input_source(bus):
    """VCP 0x60 over DDC/CI directly (no ddcutil process per poll)."""
    request = bytes([0x51, 0x82, 0x01, 0x60])
    request += bytes([0x6E ^ 0x51 ^ 0x82 ^ 0x01 ^ 0x60])
    try:
        fd = os.open(f"/dev/i2c-{bus}", os.O_RDWR)
    except OSError:
        return None
    try:
        fcntl.ioctl(fd, I2C_SLAVE, 0x37)
        os.write(fd, request)
        time.sleep(0.05)
        reply = os.read(fd, 11)
    except OSError:
        return None
    finally:
        os.close(fd)
    # 6E 88 02 result vcp type maxH maxL curH curL checksum
    if len(reply) < 10 or reply[2] != 0x02 or reply[3] != 0 or reply[4] != 0x60:
        return None
    return reply[9]


def hyprctl(*args):
    return subprocess.run(["hyprctl", *args], capture_output=True, text=True, timeout=5).stdout


def output_state(name):
    """'disabled', 'off' (DPMS off) or 'on', as Hyprland currently has it."""
    try:
        for m in json.loads(hyprctl("monitors", "-j")):
            if m.get("name") == name:
                return "on" if m.get("dpmsStatus", True) else "off"
    except (ValueError, subprocess.TimeoutExpired):
        return None
    return "disabled"


def apply(name, want, have):
    if want == have or have is None:
        return
    if want == "disabled":
        hyprctl("eval", f'hl.monitor({{output="{name}", disabled=true}})')
        return
    if have == "disabled":
        # Re-apply the configured rules; controls.lua resyncs brightness on monitor.added.
        hyprctl("eval", f"dofile({json.dumps(MONITORS_LUA)})")
        time.sleep(1)
    hyprctl("dispatch", f'hl.dsp.dpms({{action = "{"on" if want == "on" else "off"}", monitor = "{name}"}})')


def main():
    attached_at = away_since = None
    seen_mine = False
    applied, polls = None, 0
    while True:
        lg = find_lg()
        if lg is None:
            attached_at = away_since = None
            seen_mine, applied = False, None
            time.sleep(POLL)
            continue
        name, bus = lg
        now = time.monotonic()
        if attached_at is None:
            attached_at = away_since = now
        source = input_source(bus)
        if source == MINE:
            seen_mine, away_since = True, None
            want = "on"
        elif source is None:
            time.sleep(POLL)
            continue
        else:
            away_since = away_since or now
            if now - away_since >= DETACH:
                want = "disabled"
            elif not seen_mine and now - attached_at >= GRACE:
                want = "off"
            else:
                want = None
        polls += 1
        if want and (want != applied or polls % RECHECK == 0):
            apply(name, want, output_state(name))
            applied = want
        time.sleep(POLL)


if __name__ == "__main__":
    main()
