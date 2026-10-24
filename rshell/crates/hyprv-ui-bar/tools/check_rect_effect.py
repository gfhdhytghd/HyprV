#!/usr/bin/env python3
"""Isolate Qt RHI source rectangles from MultiEffect; never updates shared references.
Requires an existing parity-host and Weston, plus a built rect_effect example.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, tempfile, time
import numpy as np
from PIL import Image
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--host',type=Path,required=True)
p.add_argument('--probe',type=Path,required=True)
p.add_argument('--scene',default='rect_effect.qml')
p.add_argument('--artifact-name',default='rect-effect')
a=p.parse_args();host=a.host.resolve();probe=a.probe.resolve()
crate=Path(__file__).resolve().parents[1];workspace=crate.parents[1]
if Path(a.scene).name!=a.scene or not a.artifact_name.replace('-','').isalnum():raise SystemExit('scene and artifact names must stay local')
out=crate/'artifacts'/a.artifact_name;out.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='rect-effect-',dir=crate/'.build') as tmp:
 runtime=Path(tmp);runtime.chmod(0o700)
 env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime))
 with (out/'weston.log').open('w') as log:
  weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=rect-effect'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(runtime/'rect-effect').exists():break
    time.sleep(.05)
   else:raise RuntimeError('private compositor failed')
   qt_env=dict(env,WAYLAND_DISPLAY='rect-effect',QT_QPA_PLATFORM='wayland',QSG_RENDER_LOOP='basic',QSG_RHI_BACKEND='opengl',QT_WAYLAND_DISABLE_WINDOWDECORATION='1')
   subprocess.run([str(probe),str(out/'rust')],env=qt_env,check=True)
   for effect in [False,True]:
    fixture=runtime/f'effect-{str(effect).lower()}.json'
    props=dict(effect=effect)
    if a.scene=='batch_material.qml':props.update(control=(workspace.parent/'quickshell/assets/bar/control-panel-dark.svg').as_uri(),cover=(workspace/'parity/fixtures/assets/cover-warm.png').as_uri())
    fixture.write_text(json.dumps(dict(component='rect-effect',scene=a.scene,props=props,settle_ms=200,steps=[dict(capture='')])))
    for scale in [1,2]:
     result=subprocess.run([str(host),'--fixture',str(fixture),'--scenes',str(crate/'tools'),'--source',str(workspace.parent/'quickshell'),'--assets',str(workspace/'parity/fixtures/assets'),'--out',str(out/'qt'),'--theme','dark','--scale',str(scale)],env=qt_env,capture_output=True,text=True,timeout=90)
     (out/f'effect-{effect}@{scale}.log').write_text(result.stdout+result.stderr)
     if result.returncode:raise RuntimeError(result.stderr)
  finally:
   weston.terminate();weston.wait(timeout=10)
rows=[]
for source in sorted((out/'qt').glob('*.png')):
 def premul(path):
  v=np.asarray(Image.open(path).convert('RGBA')).astype(np.int32)
  v[:,:,:3]=(v[:,:,:3]*v[:,:,3:4]+127)//255
  return v
 q=premul(source);r=premul(out/'rust'/source.name);d=np.abs(q-r);mask=(q[:,:,3]>0)|(r[:,:,3]>0)
 rows.append(dict(file=source.name,max_channel=int(d.max()),painted_mae=float(d[mask].mean()),over8=int((d.max(2)>8).sum()),q_center=q[q.shape[0]//2,q.shape[1]//2].tolist(),r_center=r[r.shape[0]//2,r.shape[1]//2].tolist()))
report=dict(probe_sha256=hashlib.sha256(probe.read_bytes()).hexdigest(),host_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),pixel_space='premultiplied RGBA',cases=rows)
(out/'metrics.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))

