#!/usr/bin/env python3
"""Private original-QML stats observation; never publishes canonical references."""
from pathlib import Path
import importlib.util, os, re, subprocess, sys, tempfile

crate = Path(__file__).resolve().parents[1]
root = crate.parents[1]
state, scale = sys.argv[1:3]
out = crate / 'artifacts/core-panels' / f'qt-{state}-{scale}'
out.mkdir(exist_ok=True)
spec = importlib.util.spec_from_file_location('verify', root / 'parity/verify/verify.py')
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
with v.PrivateWeston() as socket, tempfile.TemporaryDirectory(prefix='panel-qt-env-') as tmp:
    cfg = Path(tmp)
    (cfg / 'qt6ct').mkdir()
    user = Path.home() / '.config'
    (cfg / 'fontconfig').symlink_to(user / 'fontconfig')
    qt = (user / 'qt6ct/qt6ct.conf').read_text()
    qt = re.sub(r'^color_scheme_path=.*', 'color_scheme_path=/usr/share/qt6ct/colors/darker.conf', qt, flags=re.M)
    qt = re.sub(r'^icon_theme=.*', 'icon_theme=Fluent-dark', qt, flags=re.M)
    (cfg / 'qt6ct/qt6ct.conf').write_text(qt)
    env = {k: os.environ[k] for k in ['HOME', 'PATH', 'XDG_RUNTIME_DIR', 'LANG', 'TZ'] if k in os.environ}
    env.update({k:os.environ[k]for k in ['LD_PRELOAD','HYPRV_GL_TRACE_FILE','HYPRV_GL_TRACE_AT','CORE_GL_UNIT','CORE_GL_VIEWPORT']if k in os.environ})
    env.update(XDG_CONFIG_HOME=str(cfg), QT_QPA_PLATFORMTHEME='qt6ct', WAYLAND_DISPLAY=socket,
               QT_QPA_PLATFORM='wayland', QSG_RHI_BACKEND='opengl', QSG_RENDER_LOOP='basic',
               QT_DISABLE_SHADER_DISK_CACHE='1', QT_WAYLAND_DISABLE_WINDOWDECORATION='1',
               HYPRV_CANVAS_SOURCE_DIR=str(out))
    with (out / 'trace.log').open('w') as log:
        subprocess.run([str(crate / 'artifacts/core-panels/qt-observer-build/parity-host'),
                        '--fixture', str(root / 'parity/fixtures/panels' / f'{state}.json'),
                        '--scenes', str(root / 'parity/scenes'), '--source', str(root.parent / 'quickshell'),
                        '--assets', str(root / 'parity/fixtures/assets'), '--out', str(out),
                        '--theme', 'dark', '--scale', scale], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
