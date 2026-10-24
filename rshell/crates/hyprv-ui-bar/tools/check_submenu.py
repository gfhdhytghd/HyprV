#!/usr/bin/env python3
"""Independent Qt submenu replays, with canonical capture environment.
Writes only crate-local diagnostics. Never edits fixtures or shared references.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, tempfile, time
import numpy as np
from PIL import Image
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--host',type=Path,required=True)
p.add_argument('--repeats',type=int,default=2)
p.add_argument('--explicit-hover',action='store_true',help='diagnostic only: move over DND after submenu creation')
a=p.parse_args();host=a.host.resolve()
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
out=crate/('artifacts/submenu-explicit-hover' if a.explicit_hover else 'artifacts/submenu-input-check');out.mkdir(exist_ok=True)
reference=crate/'artifacts/gate-inputs/reference'
def premul(path):
 v=np.asarray(Image.open(path).convert('RGBA')).astype(np.int32)
 v[:,:,:3]=(v[:,:,:3]*v[:,:,3:4]+127)//255
 return v
frozen=json.loads((reference/'manifest.json').read_text())
source_hashes={name:hashlib.sha256((root.parent/'quickshell'/name).read_bytes()).hexdigest() for name in frozen['source_sha256']}
if source_hashes!=frozen['source_sha256']:raise SystemExit('QML source hashes differ from frozen reference; replay cannot establish input-only difference')
results=[]
with tempfile.TemporaryDirectory(prefix='submenu-input-',dir=crate/'.build') as tmp:
 runtime=Path(tmp);runtime.chmod(0o700)
 env={k:os.environ[k] for k in ['HOME','PATH','LANG','TZ'] if k in os.environ}
 env['XDG_RUNTIME_DIR']=str(runtime)
 fixture=reference/'fixtures/tray-menu-submenu.json'
 if a.explicit_hover:
  data=json.loads(fixture.read_text());data['steps'].insert(-1,dict(hover=dict(text='Do Not Disturb')))
  fixture=runtime/'tray-menu-submenu.json';fixture.write_text(json.dumps(data))
 with (out/'weston.log').open('w') as log:
  weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=submenu-check'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(runtime/'submenu-check').exists():break
    time.sleep(.05)
   else:raise RuntimeError('private compositor failed')
   for theme in ['dark','light']:
    cfg=runtime/f'config-{theme}';(cfg/'qt6ct').mkdir(parents=True)
    for name in ['fontconfig']:
     source=Path.home()/'.config'/name
     if source.exists():(cfg/name).symlink_to(source)
    source=Path.home()/'.config/qt6ct/qt6ct.conf'
    if source.exists():
     lines=source.read_text().splitlines();scheme='darker' if theme=='dark' else 'simple'
     (cfg/'qt6ct/qt6ct.conf').write_text('\n'.join(f'color_scheme_path=/usr/share/qt6ct/colors/{scheme}.conf' if line.startswith('color_scheme_path=') else f'icon_theme=Fluent-{theme}' if line.startswith('icon_theme=') else line for line in lines)+'\n')
    qt_env=dict(env,XDG_CONFIG_HOME=str(cfg),WAYLAND_DISPLAY='submenu-check',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='qt6ct',QSG_RENDER_LOOP='basic',QSG_RHI_BACKEND='opengl',QT_WAYLAND_DISABLE_WINDOWDECORATION='1',QT_FORCE_STDERR_LOGGING='1')
    for repeat in range(a.repeats):
     destination=out/f'run-{repeat}';destination.mkdir(exist_ok=True)
     for scale in [1,2]:
      cmd=[str(host),'--fixture',str(fixture),'--scenes',str(root/'parity/scenes'),'--source',str(root.parent/'quickshell'),'--assets',str(root/'parity/fixtures/assets'),'--out',str(destination),'--theme',theme,'--scale',str(scale)]
      run=subprocess.run(cmd,env=qt_env,capture_output=True,text=True,timeout=120)
      name=f'tray-menu-submenu--{theme}@{scale}.png'
      (destination/(name+'.log')).write_text(run.stdout+run.stderr)
      if run.returncode:raise RuntimeError(run.stderr)
      q=premul(destination/name);f=premul(reference/name);d=np.abs(q-f)
      mask=(q[:,:,3]>0)|(f[:,:,3]>0)
      results.append(dict(file=name,repeat=repeat,max=int(d.max()),painted_mae=float(d[mask].mean()),sha256=hashlib.sha256((destination/name).read_bytes()).hexdigest()))
  finally:
   weston.terminate();weston.wait(timeout=10)
report=dict(source_sha256=source_hashes,fixture_sha256=hashlib.sha256((reference/'fixtures/tray-menu-submenu.json').read_bytes()).hexdigest(),explicit_hover=a.explicit_hover,host_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),reference=str(reference),pixel_space='premultiplied RGBA',cases=results)
(out/'metrics.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
