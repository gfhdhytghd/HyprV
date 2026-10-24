#!/usr/bin/python3 -I
"""Inner orchestrator of the full-screen capture (started by ../capture.sh, never directly).

It runs INSIDE capture.sh's isolation envelope: a private XDG_RUNTIME_DIR under
/tmp/claude-1000 and a private dbus-run-session bus that also serves as the system bus.
For each fixture × theme it starts, from scratch:

  outer   headless KWin (--virtual) sized to the real output in pixels, with a window
          rule that makes the nested compositor's window fullscreen at 0,0
  nested  Hyprland (wayland backend; no DRM, no logind, no Xwayland) whose only monitor
          WAYLAND-1 has the real output's pixel size and scale
  mocks   D-Bus services (tray/MPRIS/BlueZ/UPower/PowerProfiles), fixture sockets,
          optionally a private PipeWire with one null sink, a transparent titled window
  shell   the ORIGINAL quickshell config (`quickshell -p <source>`), under the vclock
          preload, with every external command answered by stub.py

then replays the fixture steps (IPC calls through `quickshell ipc`, pointer input through
zwlr_virtual_pointer_v1 at Qt-resolved targets, virtual time in ticks) and captures whole
outputs with grim over black and white compositor backgrounds (matte -> RGBA).

Every hyprctl call targets the nested instance explicitly (XDG_RUNTIME_DIR + signature
inline) after asserting that the signature differs from every live one.
"""
import argparse
import datetime
import hashlib
import json
import os
import shutil
import signal
import struct
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image

FS = Path(__file__).resolve().parent.parent
PARITY = FS.parent
ASSETS = PARITY / "fixtures" / "assets"
SAFE_ROOT = "/tmp/claude-1000/"
POINTER_KEYS = ("hover", "press", "click", "rightclick", "drag", "wheel", "release")
# External commands the QML runs by name (PATH lookup); all answered by stub.py.
SHIM_COMMANDS = [
    "sh", "bash", "python3", "hyprctl", "playerctl", "swaync-client", "alacritty", "xdg-open", "wlogout",
    "rofi-wayland", "rofi", "powerprofilesctl", "pavucontrol", "hyprlock", "notify-send", "cava", "busctl",
    "nm-connection-editor", "iwgtk", "nmtui", "blueman-manager", "blueberry", "bluetoothctl", "btop",
    "nmcli", "wpctl", "pactl", "brightnessctl", "ddcutil", "systemctl", "loginctl", "qs", "quickshell",
]
LEVELS = {"0": "background", "1": "bottom", "2": "top", "3": "overlay"}


def die(msg):
    print(f"run.py: {msg}", file=sys.stderr)
    sys.exit(1)


def safe_rm(path, rt):
    p = os.path.realpath(path)
    if not (p.startswith(SAFE_ROOT) and p.startswith(os.path.realpath(rt) + "/")):
        die(f"refusing to remove {p}")
    shutil.rmtree(p, ignore_errors=True)


class Isolation:
    """Facts established once and asserted before every mutation of the nested compositor."""

    def __init__(self):
        self.rt = os.environ.get("XDG_RUNTIME_DIR", "")
        self.bus = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
        self.live_sigs = set(os.environ.get("LIVE_SIGS", "").split())
        self.real_home = os.environ.get("REAL_HOME", "")
        if not self.rt.startswith(SAFE_ROOT):
            die(f"XDG_RUNTIME_DIR {self.rt!r} is not private")
        if not self.bus or SAFE_ROOT not in self.bus or os.environ.get("DBUS_SYSTEM_BUS_ADDRESS") != self.bus:
            die("not inside the private bus (session == system bus under /tmp/claude-1000 required)")
        if not self.live_sigs:
            die("LIVE_SIGS missing: cannot prove the nested signature differs from the live one")
        if not self.real_home:
            die("REAL_HOME missing")


class Nested:
    """KWin (outer) + Hyprland (nested) for one run."""

    def __init__(self, iso, run_dir, output, fixture, logs):
        self.iso, self.run, self.output, self.fixture, self.logs = iso, run_dir, output, fixture, logs
        self.procs = []
        self.sig = None
        self.wl = None

    def env(self, **extra):
        # Shared across runs: warm GL shader caches make the outer window configure within
        # Hyprland's fixed 2 s launch timeout (else it falls back and never adopts WAYLAND-1).
        cache = f"{self.iso.rt}/cache"
        os.makedirs(cache, exist_ok=True)
        e = {"PATH": "/usr/bin", "XDG_RUNTIME_DIR": self.iso.rt, "LANG": "C.UTF-8",
             "DBUS_SESSION_BUS_ADDRESS": self.iso.bus, "DBUS_SYSTEM_BUS_ADDRESS": self.iso.bus,
             "XDG_CACHE_HOME": cache, "__GL_SHADER_DISK_CACHE_PATH": cache, "MESA_SHADER_CACHE_DIR": cache}
        e.update(extra)
        return e

    def spawn(self, name, argv, env, **kw):
        log = open(self.logs / f"{name}.log", "a")
        p = subprocess.Popen(argv, env=env, stdout=kw.pop("stdout", log), stderr=log, start_new_session=True, **kw)
        self.procs.append((name, p))
        return p

    def start(self):
        # The outer window sometimes never configures (no WAYLAND-1 monitor); retry cleanly.
        for attempt in range(1, 4):
            try:
                return self.start_once(attempt)
            except Retry as e:
                print(f"  nested start attempt {attempt}: {e}; retrying", flush=True)
                self.stop()
                self.sig = None
        die("nested compositor failed to start 3 times")

    def start_once(self, attempt):
        o = self.output
        khome = self.run / f"khome{attempt}"
        (khome / ".config").mkdir(parents=True)
        (khome / ".config/kwinrulesrc").write_text(
            "[General]\ncount=1\nrules=parity\n\n[parity]\nDescription=nested compositor fills the output\n"
            f"fullscreen=true\nfullscreenrule=2\nnoborder=true\nnoborderrule=2\nposition=0,0\npositionrule=2\n"
            f"size={o['width_px']},{o['height_px']}\nsizerule=2\nwmclass=.*\nwmclassmatch=3\n")
        before = set(os.listdir(self.iso.rt))
        outer = f"wl-outer-{os.getpid()}"
        self.spawn("kwin", ["kwin_wayland", "--virtual", "--no-lockscreen", "--no-global-shortcuts",
                            "--width", str(o["width_px"]), "--height", str(o["height_px"]), "--socket", outer],
                   self.env(HOME=str(khome), XDG_CONFIG_HOME=str(khome / ".config")))
        wait_for(lambda: os.path.exists(f"{self.iso.rt}/{outer}"), 15, "outer KWin socket")
        time.sleep(1.0)  # let KWin finish its first frame before the 2 s clock starts

        hhome = self.run / f"hhome{attempt}"
        for sub in (".config", ".local/share", ".local/state", ".cache"):
            (hhome / sub).mkdir(parents=True)
        conf = self.run / f"hyprland{attempt}.conf"
        conf.write_text(self.hypr_conf())
        sigs_before = set(os.listdir(f"{self.iso.rt}/hypr")) if os.path.isdir(f"{self.iso.rt}/hypr") else set()
        self.spawn("hyprland", ["Hyprland", "-c", str(conf)], self.env(
            HOME=str(hhome), XDG_CONFIG_HOME=str(hhome / ".config"), XDG_DATA_HOME=str(hhome / ".local/share"),
            XDG_STATE_HOME=str(hhome / ".local/state"),
            WAYLAND_DISPLAY=outer, LIBSEAT_BACKEND="seatd", AQ_DRM_DEVICES="/nonexistent/dri-card",
            HYPRLAND_NO_SD_VARS="1", HYPRLAND_NO_SD_NOTIFY="1", HYPRLAND_NO_CRASHREPORTER="1", HYPRLAND_NO_RT="1"))

        def found():
            if not os.path.isdir(f"{self.iso.rt}/hypr"):
                return False
            new = [s for s in set(os.listdir(f"{self.iso.rt}/hypr")) - sigs_before
                   if os.path.exists(f"{self.iso.rt}/hypr/{s}/.socket.sock")]
            if new:
                self.sig = new[0]
            return bool(new)
        wait_for(found, 20, "nested Hyprland socket")
        if self.sig in self.iso.live_sigs:
            die(f"nested signature {self.sig} equals a live one")
        wl = sorted(n for n in set(os.listdir(self.iso.rt)) - before
                    if n.startswith("wayland-") and not n.endswith(".lock"))
        if not wl:
            die("nested Hyprland did not create a wayland socket")
        self.wl = wl[0]
        def monitors():
            out = self.hyprctl("monitors", "-j", check=False)
            return out.startswith("[") and any(m["name"] != "FALLBACK" and m["width"] > 0 for m in json.loads(out))
        try:
            wait_for(monitors, 12, "nested monitor (outer window configure)")
        except SystemExit:
            raise Retry("no WAYLAND-1 monitor")
        mons = [m for m in json.loads(self.hyprctl("monitors", "-j")) if m["name"] != "FALLBACK"]
        m = mons[0]
        if len(mons) != 1 or (m["width"], m["height"]) != (o["width_px"], o["height_px"]) or abs(m["scale"] - o["scale"]) > 1e-6:
            die(f"nested output {[(x['name'], x['width'], x['height'], x['scale']) for x in mons]} != real {o}")
        self.monitor = m["name"]

    def hypr_conf(self):
        o = self.output
        hy = self.fixture["services"].get("hyprland", {})
        ws = "\n".join(f"workspace = {w['id']}, monitor:WAYLAND-1, persistent:true"
                       + (f", defaultName:{w['name']}" if str(w.get("name", w["id"])) != str(w["id"]) else "")
                       for w in hy.get("workspaces", []))
        return f"""# generated by parity/fullscreen/tools/run.py — nested capture compositor
monitor = WAYLAND-1, {o['width_px']}x{o['height_px']}@60, 0x0, {o['scale']:g}
{ws}
general {{
    border_size = 0
    gaps_in = 0
    gaps_out = 0
}}
decoration {{
    rounding = 0
    dim_inactive = false
    blur {{
        enabled = false
    }}
    shadow {{
        enabled = false
    }}
}}
animations {{
    enabled = false
}}
misc {{
    disable_hyprland_logo = true
    disable_splash_rendering = true
    force_default_wallpaper = 0
    background_color = 0xff000000
    disable_autoreload = true
}}
ecosystem {{
    no_update_news = true
    no_donation_nag = true
}}
xwayland {{
    enabled = false
}}
cursor {{
    invisible = true
}}
debug {{
    disable_logs = false
    suppress_errors = true
}}
"""

    def hyprctl(self, *args, check=True):
        if not self.sig or self.sig in self.iso.live_sigs:
            die("hyprctl without a verified nested signature")
        if not os.path.exists(f"{self.iso.rt}/hypr/{self.sig}/.socket.sock"):
            die("nested Hyprland socket vanished")
        r = subprocess.run(["hyprctl", *args], env={"PATH": "/usr/bin", "XDG_RUNTIME_DIR": self.iso.rt,
                                                   "HYPRLAND_INSTANCE_SIGNATURE": self.sig},
                           capture_output=True, text=True, timeout=10)
        if check and r.returncode != 0:
            die(f"hyprctl {args} failed: {r.stdout}{r.stderr}")
        return r.stdout

    def grim(self):
        r = subprocess.run(["grim", "-t", "ppm", "-o", self.monitor, "-"],
                           env={"PATH": "/usr/bin", "XDG_RUNTIME_DIR": self.iso.rt, "WAYLAND_DISPLAY": self.wl},
                           capture_output=True, timeout=30)
        if r.returncode != 0:
            die(f"grim failed: {r.stderr.decode()}")
        return np.asarray(Image.open(__import__("io").BytesIO(r.stdout)).convert("RGB"))

    def stop(self):
        for name, p in reversed(self.procs):
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 5
        for name, p in reversed(self.procs):
            try:
                p.wait(max(0.1, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                p.wait()
        if self.sig:
            log = Path(f"{self.iso.rt}/hypr/{self.sig}/hyprland.log")
            if log.exists():
                shutil.copy(log, self.logs / "hyprland-internal.log")
        self.procs = []


class Retry(Exception):
    pass


def wait_for(pred, seconds, what):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if pred():
            return
        time.sleep(0.05)
    die(f"timed out waiting for {what}")


class Clock:
    def __init__(self, path):
        self.path = path
        self.now = 0.0  # virtual ms
        path.write_bytes(struct.pack("<q", 0))
        self.fd = os.open(path, os.O_WRONLY)

    def set(self, ms):
        self.now = ms
        os.pwrite(self.fd, struct.pack("<q", int(round(ms * 1000))), 0)


class Run:
    def __init__(self, iso, args, fixture_path, theme, index, build):
        self.iso, self.args, self.theme, self.build = iso, args, theme, build
        self.fixture_path = fixture_path
        self.fx = json.loads(fixture_path.read_text())
        self.state = fixture_path.stem
        if len(self.fx["outputs"]) != 1:
            die("one output per fixture is supported (KWin --virtual gives one)")
        self.output = self.fx["outputs"][0]
        self.dir = Path(iso.rt) / f"r{index}"
        self.dir.mkdir()
        self.logs = Path(args.out) / "logs" / f"{self.state}--{theme}"
        if self.logs.exists():
            shutil.rmtree(self.logs)
        self.logs.mkdir(parents=True)
        self.tick = float(self.fx.get("tick_ms", 1000.0 / 60.0))
        self.entries = []
        self.vp = None

    # ---------------------------------------------------------------- fake home + stubs
    def make_home(self):
        home = self.dir / "home"
        real = Path(self.iso.real_home)
        for rel in (".config/fontconfig", ".local/share/fonts", ".local/share/icons", ".icons", ".fonts"):
            if (real / rel).exists():
                (home / rel).parent.mkdir(parents=True, exist_ok=True)
                (home / rel).symlink_to(real / rel)
        qt = home / ".config/qt6ct"
        qt.mkdir(parents=True)
        scheme, icons = ("darker", "Fluent-dark") if self.theme == "dark" else ("simple", "Fluent-light")
        src_conf = real / ".config/qt6ct/qt6ct.conf"
        lines = src_conf.read_text().splitlines() if src_conf.exists() else []
        out = []
        for line in lines:
            if line.startswith("color_scheme_path="):
                line = f"color_scheme_path=/usr/share/qt6ct/colors/{scheme}.conf"
            elif line.startswith("icon_theme="):
                line = f"icon_theme={icons}"
            out.append(line)
        (qt / "qt6ct.conf").write_text("\n".join(out) + "\n")
        for sub in ("colors", "qss"):
            if (real / ".config/qt6ct" / sub).exists():
                (qt / sub).symlink_to(real / ".config/qt6ct" / sub)
        state = home / ".local/state/hyprv/quickshell"
        state.mkdir(parents=True)
        (state / "ui-state.env").write_text(f"theme={self.theme}\nvariant=v2\n")
        stub = FS / "tools/stub.py"
        scripts = home / ".config/HyprV/quickshell/scripts"
        scripts.mkdir(parents=True)
        names = [p.name for p in (Path(self.args.source) / "scripts").iterdir() if p.is_file()]
        bins = home / ".local/bin"
        bins.mkdir(parents=True)
        # Read-only assets the QML loads by configDir path.
        for entry in Path(self.args.source).iterdir():
            if entry.name != "scripts":
                (home / ".config/HyprV/quickshell" / entry.name).symlink_to(entry)
        hypr_scripts = home / ".config/hypr/scripts"
        hypr_scripts.mkdir(parents=True)
        for path in [scripts / n for n in names] + [bins / "dcal", bins / "viewflow-windows",
                                                    hypr_scripts / "record-script.sh"]:
            if path.suffix == ".py":
                path.write_text(f"#!/usr/bin/python3 -I\nimport os, sys\n"
                                f"os.execv('/usr/bin/python3', ['python3', '-I', {str(stub)!r}, '--cmd', "
                                f"'/usr/bin/python3', sys.argv[0], *sys.argv[1:]])\n")
            else:
                path.write_text(f"#!/usr/bin/sh\nexec /usr/bin/python3 -I {stub} --cmd \"$0\" \"$@\"\n")
            path.chmod(0o755)
        shims = self.dir / "shims"
        shims.mkdir()
        for name in SHIM_COMMANDS:
            (shims / name).symlink_to(stub)
        # Fixture processes with this run's paths.
        procs = json.dumps(self.fx.get("processes", []))
        procs = procs.replace("@HOME@", str(home)).replace("@ASSETS@", str(ASSETS))
        procs = procs.replace("/run/user/1000/dcal/", f"{self.dir}/dcal/")
        (self.dir / "processes.json").write_text(procs)
        self.home, self.shims = home, shims

    def shell_env(self, preload=True):
        e = {
            "HOME": str(self.home), "PATH": str(self.shims), "LANG": "zh_CN.UTF-8",
            "XDG_RUNTIME_DIR": self.iso.rt, "DBUS_SESSION_BUS_ADDRESS": self.iso.bus,
            "DBUS_SYSTEM_BUS_ADDRESS": self.iso.bus, "WAYLAND_DISPLAY": self.nested.wl,
            "HYPRLAND_INSTANCE_SIGNATURE": self.nested.sig, "QT_QPA_PLATFORM": "wayland",
            "QT_QPA_PLATFORMTHEME": "qt6ct", "QT_WAYLAND_DISABLE_WINDOWDECORATION": "1",
            "QT_AUTO_SCREEN_SCALE_FACTOR": "1", "QT_IM_MODULE": "fcitx", "QSG_RENDER_LOOP": "basic",
            "XCURSOR_SIZE": "24", "XDG_CURRENT_DESKTOP": "Hyprland", "XDG_SESSION_TYPE": "wayland",
            "FS_PROCESSES": str(self.dir / "processes.json"), "FS_STUB_LOG": str(self.logs / "commands.jsonl"),
            "VCLOCK_FILE": str(self.dir / "vclock"),
        }
        if os.environ.get("TZ"):
            e["TZ"] = os.environ["TZ"]
        if preload:
            e["LD_PRELOAD"] = str(self.build / "vclock.so")
            e["VCLOCK_REALTIME_MS"] = str(self.realtime_ms)
        return e

    # ---------------------------------------------------------------- services
    def start_pipewire(self):
        audio = self.fx["services"].get("audio") or {}
        if not audio.get("available"):
            return
        pw = self.dir / "pipewire"
        pw.mkdir()
        (pw / "pipewire.conf").write_text(PIPEWIRE_CONF)
        env = self.nested.env(PIPEWIRE_CONFIG_DIR=str(pw), PIPEWIRE_RUNTIME_DIR=self.iso.rt, HOME=str(self.home),
                              SPA_PLUGIN_DIR="/usr/lib/spa-0.2", PIPEWIRE_MODULE_DIR="/usr/lib/pipewire-0.3")
        self.nested.spawn("pipewire", ["pipewire", "-c", str(pw / "pipewire.conf")], env)
        wait_for(lambda: os.path.exists(f"{self.iso.rt}/pipewire-0"), 10, "private pipewire socket")
        time.sleep(0.3)
        cli_env = {k: env[k] for k in ("PATH", "XDG_RUNTIME_DIR", "PIPEWIRE_RUNTIME_DIR")}
        dump = json.loads(subprocess.run(["pw-dump"], env=cli_env, capture_output=True, text=True, timeout=10).stdout)
        sink = next(o for o in dump if o.get("info", {}).get("props", {}).get("node.name") == "parity-sink")
        vol = float(audio.get("volume", 0))
        # Quickshell reports channelVolumes cube-rooted (PipeWire stores cubic volume).
        cubic = vol ** 3
        props = "{ channelVolumes: [ %f, %f ], mute: %s }" % (cubic, cubic, "true" if audio.get("muted") else "false")
        for cmd in (["pw-metadata", "-n", "default", "0", "default.audio.sink", '{ "name": "parity-sink" }', "Spa:String:JSON"],
                    ["pw-metadata", "-n", "default", "0", "default.configured.audio.sink", '{ "name": "parity-sink" }', "Spa:String:JSON"],
                    ["pw-cli", "set-param", str(sink["id"]), "Props", props]):
            r = subprocess.run(cmd, env=cli_env, capture_output=True, text=True, timeout=10)
            if r.returncode != 0:
                die(f"{cmd[0]} failed: {r.stderr}")

    def start_mocks(self):
        services = self.dir / "services.json"
        services.write_text(json.dumps(self.fx["services"]))
        self.services_ready = self.dir / "services.ready"
        self.nested.spawn("services", ["/usr/bin/python3", "-I", str(FS / "mocks/services.py"), str(services),
                                       str(ASSETS), str(self.services_ready)],
                          self.nested.env(HOME=str(self.home)))
        if self.fx.get("sockets"):
            spec = self.dir / "sockets.json"
            spec.write_text(json.dumps(self.fx["sockets"]))
            ready = self.dir / "sockets.ready"
            self.nested.spawn("sockets", ["/usr/bin/python3", "-I", str(FS / "mocks/sockets.py"), str(spec),
                                          str(self.dir / "dcal"), str(ready)], self.nested.env())
            wait_for(ready.exists, 10, "socket mocks")

    def start_titlewin(self):
        top = self.fx["services"].get("hyprland", {}).get("activeToplevel")
        if not top:
            return
        p = self.nested.spawn("titlewin", [str(self.build / "titlewin"), top["title"]],
                              self.nested.env(WAYLAND_DISPLAY=self.nested.wl), stdout=subprocess.PIPE)
        if p.stdout.readline().strip() != b"mapped":
            die("titlewin did not map")
        wait_for(lambda: json.loads(self.nested.hyprctl("activewindow", "-j") or "{}").get("title") == top["title"],
                 5, "active window title")

    # ---------------------------------------------------------------- run
    def execute(self):
        self.make_home()
        self.nested = Nested(self.iso, self.dir, self.output, self.fx, self.logs)
        try:
            self.nested.start()
            self.nested.hyprctl("dismissnotify", "-1", check=False)
            hy = self.fx["services"].get("hyprland", {})
            if hy.get("focusedWorkspace"):
                self.nested.hyprctl("dispatch", "workspace", str(hy["focusedWorkspace"]))
            if hy.get("fullscreen"):
                die("fullscreen state needs a fullscreen client; not supported yet")
            self.start_mocks()
            self.start_pipewire()
            self.clock = Clock(self.dir / "vclock")
            now = datetime.datetime.fromisoformat(self.fx["now"])
            self.realtime_ms = int(time.mktime(now.timetuple()) * 1000)
            self.nested.spawn("quickshell", ["/usr/bin/quickshell", "-p", self.args.source, "-n"], self.shell_env())
            wait_for(lambda: "hyprv-quickshell" in self.nested.hyprctl("layers", "-j"), 60, "the shell's bar layer")
            # Mapped after the shell is up, so Quickshell.Hyprland sees the activewindow event.
            self.start_titlewin()
            if self.fx["services"].get("tray"):
                wait_for(self.services_ready.exists, 20, "tray items registered")
            self.vp = subprocess.Popen([str(self.build / "vpointer")], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       env=self.nested.env(WAYLAND_DISPLAY=self.nested.wl), text=True, bufsize=1)
            self.advance(float(self.fx.get("settle_ms", 1500)))
            # As parity-host: pointer parks in the output's far corner, then one tick.
            self.pointer("move", self.output["width"] - 1, self.output["height"] - 1)
            self.advance(self.tick)
            self.steps()
        finally:
            if self.vp:
                self.vp.stdin.close()
                self.vp.wait(5)
            self.nested.stop()
            safe_rm(self.dir, self.iso.rt)
        return self.entries

    def advance(self, ms):
        # One virtual tick per compositor frame (~60 Hz real), like parity-host's one rendered
        # frame per tick: frame-driven QML (callLater chains, polish/layout passes) then sees
        # the same number of frames per virtual interval as the live shell.
        target = self.clock.now + ms
        while self.clock.now + 1e-9 < target:
            self.clock.set(min(target, self.clock.now + self.tick))
            time.sleep(self.args.tick_real)
        time.sleep(0.05)

    def pointer(self, *cmd):
        if cmd[0] == "move":
            line = f"move {cmd[1]} {cmd[2]} {self.output['width']} {self.output['height']}"
            self.mouse = (cmd[1], cmd[2])
        else:
            line = " ".join(str(c) for c in cmd)
        self.vp.stdin.write(line + "\n")
        reply = self.vp.stdout.readline().strip()
        if reply != "ok":
            die(f"vpointer: {line!r} -> {reply!r}")

    def target(self, i, key, value):
        if isinstance(value, list):
            return value
        by_theme = self.fx.get("targets_by_theme", {}).get(self.theme, {})
        t = by_theme.get(f"{i}/{key}") or self.fx.get("targets", {}).get(f"{i}/{key}")
        if not t:
            die(f"step {i}/{key}: no resolved target (run tools/resolve-targets.py)")
        return t

    def quiesce(self):
        """At frozen virtual time, let the compositor round-trips of an input/IPC step finish
        (new popup surfaces created, configured and first frame drawn) before time moves —
        parity-host creates windows instantly, a real compositor does not."""
        time.sleep(0.1)
        self.settled_grab()

    def steps(self):
        for i, step in enumerate(self.fx["steps"]):
            acted = any(k in step for k in ("set",) + POINTER_KEYS)
            if "set" in step:
                s = step["set"]
                if set(s) - {"ipc"}:
                    die(f"step {i}: set keys {sorted(set(s) - {'ipc'})} are not replayable in the real shell")
                self.ipc(s["ipc"])
            if "invoke" in step:
                die(f"step {i}: invoke is not replayable in the real shell")
            if "hover" in step:
                self.pointer("move", *self.target(i, "hover", step["hover"]))
            if "press" in step:
                self.pointer("move", *self.target(i, "press", step["press"]))
                self.pointer("down", "left")
            if "click" in step:
                self.pointer("move", *self.target(i, "click", step["click"]))
                self.pointer("down", "left")
                self.pointer("up", "left")
            if "rightclick" in step:
                self.pointer("move", *self.target(i, "rightclick", step["rightclick"]))
                self.pointer("down", "right")
                self.pointer("up", "right")
            if "drag" in step:
                to = self.target(i, "drag", step["drag"])
                frm = self.mouse
                for k in range(1, 13):
                    self.pointer("move", frm[0] + (to[0] - frm[0]) * k / 12, frm[1] + (to[1] - frm[1]) * k / 12)
                    self.advance(self.tick)
            if "wheel" in step:
                self.pointer("move", *self.target(i, "wheel", step["wheel"]))
                self.pointer("axis", step["wheel"].get("dy", 0))
            if "release" in step:
                if step["release"] is not True:
                    self.pointer("move", *self.target(i, "release", step["release"]))
                self.pointer("up", "left")
            if acted:
                self.quiesce()
            if "advance" in step:
                self.advance(float(step["advance"]))
            if "capture" in step:
                self.capture(step["capture"], self.clock.now)
            if "frames" in step:
                f = step["frames"]
                start = self.clock.now
                for at in f["at"]:
                    if start + at > self.clock.now:
                        self.advance(start + at - self.clock.now)
                    self.capture(f"{f['name']}-t{int(at):04d}", self.clock.now - start)

    def ipc(self, call):
        target, fn, *args = call
        r = subprocess.run(["/usr/bin/quickshell", "ipc", "-p", self.args.source, "call", target, fn, *map(str, args)],
                           env=self.shell_env(preload=False) | {"PATH": "/usr/bin"}, capture_output=True, text=True, timeout=20)
        if r.returncode != 0:
            die(f"ipc {call} failed: {r.stdout}{r.stderr}")

    # ---------------------------------------------------------------- capture
    def settled_grab(self):
        prev = self.nested.grim()
        for _ in range(30):
            time.sleep(0.1)
            cur = self.nested.grim()
            if np.array_equal(cur, prev):
                return cur
            prev = cur
        die("output never became stable at a frozen virtual time")

    def capture(self, frame, virtual_ms):
        time.sleep(0.15)
        layers = self.layers()
        for attempt in range(3):
            black = self.settled_grab().astype(np.int16)
            self.nested.hyprctl("keyword", "misc:background_color", "0xffffffff")
            white = self.settled_grab().astype(np.int16)
            self.nested.hyprctl("keyword", "misc:background_color", "0xff000000")
            again = self.settled_grab().astype(np.int16)
            if np.array_equal(black, again):
                break
            print(f"  {frame}: output changed during the matte, retrying", flush=True)
        else:
            die(f"{frame}: output changed during the matte 3 times (not a settled state)")
        d = white - black
        if d.min() < 0:
            die(f"{frame}: white-background capture darker than black at {int((d < 0).any(axis=2).sum())} px")
        # Black background: out = premultiplied colour exactly. White: out = colour + (255 - alpha).
        # Hyprland's blend rounds a few edge pixels in one channel; take the median
        # channel as alpha and record those pixels as uncertain.
        alpha_c = 255 - d
        spread = alpha_c.max(axis=2) - alpha_c.min(axis=2)
        alpha = np.median(alpha_c, axis=2).astype(np.int16)
        premul = black
        excess = premul - alpha[..., None]
        bad = (spread > 2) | (excess.max(axis=2) > 2)
        if bad.sum() > 64 or spread.max() > 8:
            for n, img in (("black", black), ("white", white)):
                Image.fromarray(img.astype(np.uint8), "RGB").save(self.logs / f"matte-{frame or 'capture'}-{n}.png")
            ys, xs = np.nonzero(bad)
            die(f"{frame}: inconsistent matte at {len(xs)} px (alpha spread max {int(spread.max())}, colour > alpha by "
                f"{int(excess.max())}), bbox x {xs.min()}-{xs.max()} y {ys.min()}-{ys.max()}; images in {self.logs}")
        alpha = np.maximum(alpha, premul.max(axis=2))
        uncertain = np.argwhere((spread > 0) | (excess.max(axis=2) > 0))[:, ::-1]
        straight = np.where(alpha[..., None] > 0,
                            np.clip(np.round(premul * 255.0 / np.maximum(alpha, 1)[..., None]), 0, 255), 0)
        rgba = np.dstack([straight, alpha]).astype(np.uint8)
        back = np.round(straight * alpha[..., None] / 255.0)
        roundtrip = int(np.abs(back - premul).max()) if alpha.any() else 0
        name = f"{self.state}{'--' + frame if frame else ''}--{self.theme}@{self.output['scale']:g}.png"
        Image.fromarray(rgba, "RGBA").save(Path(self.args.out) / name, compress_level=6)
        o = self.output
        self.entries.append({
            "file": name, "fixture": f"fixtures/{self.fixture_path.name}", "state": self.state, "frame": frame,
            "theme": self.theme, "scale": o["scale"], "virtual_ms": virtual_ms,
            "width_logical": o["width"], "height_logical": o["height"],
            "width_px": int(rgba.shape[1]), "height_px": int(rgba.shape[0]),
            "output": {"name": o["name"], "make": o.get("make", ""), "model": o.get("model", ""),
                       "x": o["x"], "y": o["y"], "refresh": o.get("refresh"), "nested_name": self.nested.monitor},
            "distance_field_text": None,  # filled from parity-host's run of the same fixture (merge step)
            "layers": layers,
            "matte": {"alpha_channel_spread_max": int(spread.max()), "uncertain_px": len(uncertain),
                      "uncertain_px_coords": uncertain.tolist() if len(uncertain) <= 4096 else "too many to list",
                      "uncertain_meaning": "physical px [x, y] where the black/white matte channels disagree (alpha may be off by up to half the spread)",
                      "premultiplied_roundtrip_max_delta": roundtrip},
        })
        print(f"  {name}  (matte: {len(uncertain)} uncertain px, roundtrip {roundtrip})", flush=True)

    def layers(self):
        data = json.loads(self.nested.hyprctl("layers", "-j"))
        out = []
        for mon, v in data.items():
            for lv in sorted(v["levels"], key=int):
                for z, l in enumerate(v["levels"][lv]):
                    out.append({"level": LEVELS.get(lv, lv), "order": z, "namespace": l["namespace"],
                                "x": l["x"], "y": l["y"], "w": l["w"], "h": l["h"]})
        return out


PIPEWIRE_CONF = """# private PipeWire for the nested capture: no devices, no D-Bus, one null sink
context.properties = {
    core.daemon = true
    core.name = pipewire-0
    support.dbus = false
    module.x11.bell = false
    mem.allow-mlock = false
    log.level = 2
}
context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-access args = { } }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-spa-node-factory }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
]
context.objects = [
    { factory = metadata args = { metadata.name = default } }
    { factory = spa-node-factory
      args = { factory.name = support.node.driver node.name = Dummy-Driver node.group = pipewire.dummy
               priority.driver = 20000 } }
    { factory = adapter
      args = { factory.name = support.null-audio-sink node.name = parity-sink node.description = "Parity Sink"
               media.class = Audio/Sink audio.position = [ FL FR ] monitor.channel-volumes = true
               node.group = pipewire.dummy } }
]
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--build", required=True)
    ap.add_argument("--themes", default="light dark")
    ap.add_argument("--tick-real", type=float, default=float(os.environ.get("FS_TICK_REAL", "0.02")),
                    help="real seconds per virtual tick (default 0.02: about one compositor frame)")
    ap.add_argument("--probe", type=int, default=0, help="only start/stop the nested compositor N times")
    ap.add_argument("states", nargs="*")
    args = ap.parse_args()
    # SIGTERM (timeout, stopped task) -> SystemExit, so every Run's finally tears down.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    iso = Isolation()
    fixtures = sorted(p for p in (FS / "fixtures").glob("*.json") if p.stem != "outputs")
    if args.probe:
        Path(args.out).mkdir(parents=True, exist_ok=True)
        fx = json.loads(fixtures[0].read_text())
        for k in range(args.probe):
            d = Path(iso.rt) / f"probe{k}"
            d.mkdir()
            logs = Path(args.out) / "logs" / f"probe{k}"
            logs.mkdir(parents=True, exist_ok=True)
            n = Nested(iso, d, fx["outputs"][0], fx, logs)
            t = time.monotonic()
            try:
                n.start()
                print(f"probe {k}: ok in {time.monotonic() - t:.1f}s", flush=True)
            finally:
                n.stop()
                safe_rm(d, iso.rt)
        return
    if args.states:
        fixtures = [p for p in fixtures if p.stem in args.states]
        if len(fixtures) != len(args.states):
            die(f"unknown state in {args.states}")
    Path(args.out).mkdir(parents=True, exist_ok=True)
    frag_dir = Path(args.out) / "logs"
    frag_dir.mkdir(exist_ok=True)
    failures = []
    n = 0
    for fx in fixtures:
        for theme in args.themes.split():
            n += 1
            print(f"fullscreen/{fx.stem} [{theme}]", flush=True)
            try:
                entries = Run(iso, args, fx, theme, n, Path(args.build)).execute()
                (frag_dir / f"{fx.stem}--{theme}.entries.json").write_text(json.dumps(entries, indent=1))
            except SystemExit as e:
                failures.append(f"{fx.stem} {theme}: {e}")
                print(f"  FAILED (see logs/{fx.stem}--{theme}/)", flush=True)
    if failures:
        print("failures:\n  " + "\n  ".join(failures), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
