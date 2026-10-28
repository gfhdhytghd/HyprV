#!/usr/bin/env python3
"""Corner-overlay GPU memory probe in a private compositor (docs/verify/corner-overlay-memory.md).

  python3 docs/verify/corner-overlay-memory.py BIN OUT.json --size 6144x3456 --scale 2 \
      --profile desktop [--reconfigure] [--toggle]

Private runtime dir, private D-Bus (session and system), `kwin_wayland --virtual` parent and
a headless nested Hyprland, as `--nested-test` does. The shell runs `--read-only` against the
private buses only. The live session's WAYLAND_DISPLAY/HYPRLAND_INSTANCE_SIGNATURE/D-Bus are
removed from every child environment; hyprctl requests go to the private instance socket.

GPU memory is sampled per process every ~2 ms: NVML graphics-process usedGpuMemory on NVIDIA,
and the drm-* keys of /proc/PID/fdinfo where the kernel driver publishes them (xe, i915, amdgpu).
"""
import argparse, ctypes, json, os, shutil, signal, socket, subprocess, tempfile, threading, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class Nvml:
    class Info(ctypes.Structure):
        _fields_ = [('pid', ctypes.c_uint), ('used', ctypes.c_ulonglong), ('gi', ctypes.c_uint), ('ci', ctypes.c_uint)]

    def __init__(self):
        try:
            self.lib = ctypes.CDLL('libnvidia-ml.so.1')
            if self.lib.nvmlInit_v2():
                raise OSError('nvmlInit')
            count = ctypes.c_uint()
            self.lib.nvmlDeviceGetCount_v2(ctypes.byref(count))
            self.handles = []
            for i in range(count.value):
                h = ctypes.c_void_p()
                self.lib.nvmlDeviceGetHandleByIndex_v2(i, ctypes.byref(h))
                self.handles.append(h)
        except OSError:
            self.lib = None

    def used(self, pid):
        if not self.lib:
            return None
        total, seen, rows = 0, False, (self.Info * 256)()
        for h in self.handles:
            n = ctypes.c_uint(256)
            if self.lib.nvmlDeviceGetGraphicsRunningProcesses_v3(h, ctypes.byref(n), rows):
                continue
            for r in rows[:n.value]:
                if r.pid == pid:
                    total += r.used
                    seen = True
        return total if seen else 0


def fdinfo(pid):
    """Sum drm-* memory keys over distinct DRM clients of PID (KiB -> bytes)."""
    clients, keys = {}, {}
    try:
        names = os.listdir(f'/proc/{pid}/fdinfo')
    except OSError:
        return {}
    for name in names:
        try:
            text = Path(f'/proc/{pid}/fdinfo/{name}').read_text()
        except OSError:
            continue
        rows = dict(l.split(':', 1) for l in text.splitlines() if ':' in l)
        client = rows.get('drm-client-id')
        if client is None or client in clients:
            continue
        clients[client] = True
        for k, v in rows.items():
            if k.startswith(('drm-total-', 'drm-resident-')):
                value, *unit = v.split()
                scale = {'KiB': 1024, 'MiB': 1 << 20}.get(unit[0] if unit else '', 1)
                keys[k] = keys.get(k, 0) + int(value) * scale
    return keys


def rss(pid):
    try:
        for line in Path(f'/proc/{pid}/status').read_text().splitlines():
            if line.startswith('VmRSS:'):
                return int(line.split()[1]) * 1024
    except OSError:
        return None


class Sampler(threading.Thread):
    def __init__(self, pid, nvml):
        super().__init__(daemon=True)
        self.pid, self.nvml, self.stop, self.rows, self.mark = pid, nvml, threading.Event(), [], 'start'

    def run(self):
        drm = True
        while not self.stop.is_set():
            keys = fdinfo(self.pid) if drm else {}
            # Drivers without drm-* fdinfo (NVIDIA): stop paying for the fd walk.
            drm = bool(keys) or len(self.rows) < 200
            self.rows.append((time.monotonic(), self.mark, self.nvml.used(self.pid), keys))
            time.sleep(0.002)

    def window(self, mark):
        rows = [r for r in self.rows if r[1] == mark]
        gpu = [r[2] for r in rows if r[2] is not None]
        drm = {}
        for r in rows:
            for k, v in r[3].items():
                drm.setdefault(k, []).append(v)
        mib = lambda v: round(v / 1048576, 2)
        return {'samples': len(rows),
                'nvml_peak_mib': mib(max(gpu)) if gpu else None,
                'nvml_final_mib': mib(gpu[-1]) if gpu else None,
                'fdinfo_peak_mib': {k: mib(max(v)) for k, v in drm.items()},
                'fdinfo_final_mib': {k: mib(v[-1]) for k, v in drm.items()}}


def request(path, text):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(5)
        s.connect(str(path))
        s.sendall(text.encode())
        data = b''
        while chunk := s.recv(65536):
            data += chunk
    return data.decode()


def wait(test, what, seconds, child=None):
    deadline = time.monotonic() + seconds
    while not (result := test()):
        if child is not None and child.poll() is not None:
            raise RuntimeError(f'{what}: child exited {child.returncode}')
        if time.monotonic() > deadline:
            raise RuntimeError(f'{what}: timeout')
        time.sleep(0.05)
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('binary', type=Path)
    ap.add_argument('out', type=Path)
    ap.add_argument('--size', default='6144x3456')
    ap.add_argument('--scale', type=float, default=2.)
    ap.add_argument('--profile', default='desktop', choices=['desktop', 'laptop'])
    ap.add_argument('--settle', type=float, default=8.)
    ap.add_argument('--reconfigure', action='store_true', help='scale/transform/mode changes on the private output')
    ap.add_argument('--toggle', action='store_true', help='laptop bar toggle round trip (strip and corner motion)')
    ap.add_argument('--grab', type=Path, help='also write private grim screencopies (black/white backgrounds) here')
    ap.add_argument('--fake-proc', action='store_true',
                    help='static /proc/stat and /proc/meminfo (user+mount namespace) so strip text is reproducible')
    args = ap.parse_args()
    width, height = map(int, args.size.split('x'))
    nvml = Nvml()
    runtime = Path(tempfile.mkdtemp(prefix='hv-cornermem-'))
    os.chmod(runtime, 0o700)
    home = runtime / 'home'
    (home / '.config').mkdir(parents=True)
    (home / '.config/kwinrulesrc').write_text(
        f'[General]\ncount=1\n\n[1]\nDescription=Private corner probe\nwmclass=aquamarine\nwmclassmatch=1\n'
        f'size={width},{height}\nsizerule=2\nposition=0,0\npositionrule=2\nnoborder=true\nnoborderrule=2\n')
    children, logs = [], []
    base = {k: v for k, v in os.environ.items() if not k.startswith(('WAYLAND_', 'HYPRLAND_', 'DBUS_', 'XDG_'))
            and k not in ('DISPLAY', 'LIBGL_ALWAYS_SOFTWARE', 'LD_PRELOAD')}
    base.update(HOME=str(home), XDG_RUNTIME_DIR=str(runtime), XDG_CONFIG_HOME=str(home / '.config'),
                XDG_CACHE_HOME=str(home / '.cache'), XDG_DATA_HOME=str(home / '.local/share'),
                XDG_STATE_HOME=str(home / '.local/state'))

    def launch(cmd, env, name):
        log = (runtime / f'{name}.log').open('w')
        logs.append(log)
        child = subprocess.Popen(cmd, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        children.append(child)
        return child

    result = {'binary': str(args.binary), 'size': [width, height], 'scale': args.scale, 'profile': args.profile}
    try:
        bus = subprocess.Popen(['dbus-daemon', '--session', '--nofork', '--print-address=1'], env=base,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True)
        children.append(bus)
        address = bus.stdout.readline().strip()
        base.update(DBUS_SESSION_BUS_ADDRESS=address, DBUS_SYSTEM_BUS_ADDRESS=address)
        kwin = launch(['kwin_wayland', '--virtual', '--socket', 'parent', '--width', str(width), '--height', str(height),
                       '--no-lockscreen', '--no-global-shortcuts', '--no-kactivities'],
                      {**base, 'QT_QPA_PLATFORM': 'offscreen'}, 'kwin')
        wait(lambda: (runtime / 'parent').exists(), 'virtual KWin', 20, kwin)
        shim = runtime / 'nested-flush.so'
        subprocess.run(['cc', '-shared', '-fPIC', str(ROOT / 'crates/hyprv-shell/src/nested-flush.c'), '-o', str(shim), '-ldl'], check=True)
        config = runtime / 'hyprland.lua'
        config.write_text(
            f'hl.monitor({{output="",mode="{width}x{height}@60",position="auto",scale={args.scale}}})\n'
            'hl.config({general={gaps_in=0,gaps_out=0,border_size=0},animations={enabled=false},decoration={rounding=0,'
            'dim_inactive=false,shadow={enabled=false},blur={enabled=false}},misc={disable_hyprland_logo=true,disable_watchdog_warning=true,'
            'disable_splash_rendering=true,force_default_wallpaper=0,background_color=0xff000000},cursor={invisible=true},'
            'xwayland={enabled=false}})\n')
        hypr = launch(['Hyprland', '--config', str(config)],
                      {**base, 'AQ_BACKEND': 'headless', 'LD_PRELOAD': str(shim), 'WAYLAND_DISPLAY': str(runtime / 'parent'),
                       'HYPRLAND_NO_SD_VARS': '1'}, 'hyprland')
        command = wait(lambda: next(iter(runtime.glob('hypr/*/.socket.sock')), None), 'Hyprland IPC', 30, hypr)
        live = {p.name for p in Path(f'/run/user/{os.getuid()}/hypr').glob('*')} if Path(f'/run/user/{os.getuid()}/hypr').exists() else set()
        if command.parent.name in live:
            raise RuntimeError('private signature collides with a live instance')

        def monitors():
            try:
                return json.loads(request(command, 'j/monitors'))
            except (OSError, ValueError):
                return []
        mon = wait(lambda: [m for m in monitors() if m['width'] == width and m['height'] == height
                            and abs(m['scale'] - args.scale) < 1e-3], 'private output size', 30, hypr)[0]
        output = mon['name']
        wl = wait(lambda: next((p for p in runtime.glob('wayland-*') if p.is_socket()), None), 'Wayland socket', 10, hypr)
        env = {**base, 'WAYLAND_DISPLAY': str(wl), 'HYPRLAND_INSTANCE_SIGNATURE': command.parent.name,
               'HYPRV_PROFILE': args.profile, 'HYPRV_ASSET_ROOT': os.environ.get('HYPRV_ASSET_ROOT', str(Path.home() / '.local/share/hyprv-shell')),
               'HYPRV_CONFIG_DIR': str(home / '.config/HyprV')}
        cmd = [str(args.binary.resolve()), '--read-only', '--no-exclusive']
        if args.fake_proc:
            (runtime / 'stat').write_text('cpu 100 0 0 100 0 0 0 0 0 0\ncpu0 100 0 0 100 0 0 0 0 0 0\n')
            # A frozen copy keeps every field drivers and allocators read.
            (runtime / 'meminfo').write_text(Path('/proc/meminfo').read_text())
            # Bind-mount as root of a user+mount namespace, then re-enter the real uid in a
            # nested user namespace (D-Bus EXTERNAL auth). unshare/execv never fork, so the
            # sampled PID stays the shell's.
            bind = ('import ctypes,os,sys;libc=ctypes.CDLL(None,use_errno=True)\n'
                    'for s,t in ((sys.argv[1],"/proc/stat"),(sys.argv[2],"/proc/meminfo")):\n'
                    ' assert libc.mount(s.encode(),t.encode(),None,4096,None)==0,ctypes.get_errno()\n'
                    'os.execvp("unshare",["unshare","--user",f"--map-user={sys.argv[3]}",f"--map-group={sys.argv[4]}",*sys.argv[5:]])')
            cmd = ['unshare', '-Urm', 'python3', '-I', '-c', bind, str(runtime / 'stat'), str(runtime / 'meminfo'),
                   str(os.getuid()), str(os.getgid()), *cmd]
        shell = launch(cmd, env, 'shell')
        sampler = Sampler(shell.pid, nvml)
        sampler.start()
        namespace = f'hyprv-screen-corner-overlay-{output}'

        def layers():
            try:
                data = json.loads(request(command, 'j/layers'))
            except (OSError, ValueError):
                return []
            return [l for m in data.values() for level in m['levels'].values() for l in level if l['namespace'] == namespace]
        wait(layers, 'corner overlay mapped', 30, shell)
        time.sleep(args.settle)
        sampler.mark = 'idle'
        time.sleep(3)
        result['output'] = output
        result['corner_layers'] = layers()
        result['rss_mib'] = round((rss(shell.pid) or 0) / 1048576, 2)

        def grab(tag):
            if not args.grab:
                return
            args.grab.mkdir(parents=True, exist_ok=True)
            for i, color in enumerate(['0xff000000', '0xffffffff', '0xff000000', '0xff000000']):
                if request(command, f'eval hl.config({{misc={{background_color={color}}}}})').strip() != 'ok':
                    raise RuntimeError('private background change failed')
                time.sleep(0.5)
                name = f'grab-{i}.png' if tag == 'idle' else f'{tag}-{i}.png'
                subprocess.run(['grim', '-t', 'png', '-o', output, str(args.grab / name)], env=env, check=True)
        grab('idle')
        steps = []
        if args.reconfigure:
            mode = f'{width}x{height}@60'
            steps = [('scale 1.5', mode, 1.5, 0), ('scale 2 back', mode, args.scale, 0), ('transform 90', mode, args.scale, 1),
                     ('transform 0', mode, args.scale, 0), ('mode 3840x2160', '3840x2160@60', args.scale, 0), ('mode back', mode, args.scale, 0)]
        for label, mode, scale, transform in steps:
            sampler.mark = label
            reply = request(command, f'eval hl.monitor({{output="{output}",mode="{mode}",position="0x0",scale={scale},transform={transform}}})')
            if reply.strip() != 'ok':
                raise RuntimeError(f'{label}: {reply}')
            time.sleep(4)
            if shell.poll() is not None:
                raise RuntimeError(f'shell exited during {label}')
        if steps:
            grab('reconfigured')
        if args.toggle:
            for label in ('toggle expand', 'toggle compact'):
                sampler.mark = label
                with socket.socket(socket.AF_UNIX) as s:
                    s.settimeout(5)
                    s.connect(str(runtime / 'hyprv/shell.sock'))
                    s.sendall(json.dumps({'target': 'bar', 'function': 'toggle', 'args': [output]}).encode() + b'\n')
                    s.recv(65536)
                time.sleep(3)
                if label == 'toggle expand':
                    grab('expanded')
        sampler.mark = 'final'
        time.sleep(3)
        sampler.stop.set()
        sampler.join()
        windows = ['start', 'idle'] + [s[0] for s in steps] + (['toggle expand', 'toggle compact'] if args.toggle else []) + ['final']
        result['windows'] = {w: sampler.window(w) for w in windows}
        gpu = [r[2] for r in sampler.rows if r[2] is not None]
        result['nvml_overall_peak_mib'] = round(max(gpu) / 1048576, 2) if gpu else None
        result['fdinfo_available'] = any(r[3] for r in sampler.rows)
        result['shell_alive'] = shell.poll() is None
    finally:
        for p in reversed(children):
            if p.poll() is None:
                os.killpg(p.pid, signal.SIGTERM)
                try:
                    p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait()
        for log in logs:
            log.close()
        args.out.parent.mkdir(parents=True, exist_ok=True)
        for name in ('shell.log', 'hyprland.log'):
            if (runtime / name).exists():
                shutil.copy2(runtime / name, args.out.with_suffix(f'.{name}'))
        shutil.rmtree(runtime, ignore_errors=True)
        args.out.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
