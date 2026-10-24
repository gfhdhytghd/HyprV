#!/usr/bin/env bash
# Explicit surfaceless EGL regressions. Never connects to a Wayland/X11 display.
# Vendor selection can be supplied via __EGL_VENDOR_LIBRARY_FILENAMES (Mesa/NVIDIA).
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
bin=$(cargo test -p hyprv-ui --lib --no-run --message-format=json \
  | python3 -c 'import json,sys; [print(x["executable"]) for x in map(json.loads,sys.stdin) if x.get("executable") and x.get("profile",{}).get("test")]')
env -u WAYLAND_DISPLAY -u DISPLAY "$bin" renderer_regressions:: --ignored --nocapture --test-threads=1
# Exercise the public coordinated flush API with a fresh, private Wayland EGL display.
python3 - "$bin" <<'PY'
import os,pathlib,subprocess,sys,tempfile,time
with tempfile.TemporaryDirectory(prefix='hyprv-ui-egl-') as runtime:
    env=dict(os.environ,XDG_RUNTIME_DIR=runtime,WAYLAND_DISPLAY='hyprv-ui-test-flush',HYPRV_TEST_PRIVATE_WAYLAND='1')
    env.pop('DISPLAY',None);env.pop('EGL_PLATFORM',None)
    with tempfile.TemporaryFile() as log:
        weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=kiosk','--no-config','--idle-time=0','--socket='+env['WAYLAND_DISPLAY']],env=env,stdout=log,stderr=log)
        try:
            for _ in range(100):
                if pathlib.Path(runtime,env['WAYLAND_DISPLAY']).exists():break
                if weston.poll() is not None:break
                time.sleep(.05)
            if not pathlib.Path(runtime,env['WAYLAND_DISPLAY']).exists():
                log.seek(0);raise RuntimeError(log.read().decode())
            subprocess.run([sys.argv[1],'offscreen::partial_tests::','--ignored','--nocapture','--test-threads=1'],env=env,check=True)
            subprocess.run([sys.argv[1],'egl::cache_flush_tests::','--ignored','--nocapture','--test-threads=1'],env=env,check=True)
            subprocess.run([sys.argv[1],'egl::frame_audit::tests::','--ignored','--nocapture','--test-threads=1'],env=env,check=True)
        finally:
            weston.terminate();weston.wait(timeout=10)
PY
