#!/usr/bin/env python3
"""Finite observation with a private instrumented host; no shared refs are changed.
Requires the observer host in .build/qt-context-host (source retained there).
"""
from pathlib import Path
import tempfile,subprocess,os,time,hashlib,json
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];out=crate/'artifacts'/os.environ.get('HYPRV_QT_TRACE_ARTIFACT','qt-image-trace-final');out.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='qt-image-',dir=crate/'.build') as tmp:
 r=Path(tmp);r.chmod(0o700);env={**os.environ,'XDG_RUNTIME_DIR':str(r),'WAYLAND_DISPLAY':'qt-image','QT_QPA_PLATFORM':'wayland','QSG_RHI_BACKEND':'opengl','QSG_RENDER_LOOP':'basic','HYPRV_QT_TRACE_IMAGES':'1','HYPRV_QT_SHADER_DUMP':str(out),'QT_WAYLAND_DISABLE_WINDOWDECORATION':'1'}
 with (out/'weston.log').open('w')as log:
  proc=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=qt-image'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(r/'qt-image').exists():break
    time.sleep(.05)
   for scale in map(float,os.environ.get('HYPRV_QT_TRACE_SCALES','1,2').split(',')):
    with (out/f'trace@{scale}.log').open('w')as log:
     subprocess.run([str(crate/'.build/qt-context-host/build/parity-host'),'--fixture',str(crate/'artifacts/gate-inputs/reference/fixtures'/os.environ.get('HYPRV_QT_TRACE_FIXTURE','tray-menu-root.json')),'--scenes',str(root/'parity/scenes'),'--source',str(root.parent/'quickshell'),'--assets',str(root/'parity/fixtures/assets'),'--out',str(out),'--theme',os.environ.get('HYPRV_QT_TRACE_THEME','dark'),'--scale',str(scale)],env={**env,**({'LD_PRELOAD':env['HYPRV_QT_GL_PRELOAD']} if 'HYPRV_QT_GL_PRELOAD'in env else{})},stdout=log,stderr=subprocess.STDOUT,check=True)
  finally:proc.terminate();proc.wait(timeout=10)
(out/'provenance.json').write_text(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [crate/'.build/qt-context-host/source/main.cpp',crate/'.build/qt-context-host/build/parity-host',root/'parity/host/main.cpp']},indent=2))
