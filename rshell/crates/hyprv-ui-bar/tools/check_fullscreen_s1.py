#!/usr/bin/env python3
"""Read-only full-screen S1 probe using the bar adapter and frozen idle fixture.
No compositor from the user's session is contacted. Runtime is Qt-free.
"""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,tempfile,time
from PIL import Image
import numpy as np
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
out=crate/'artifacts'/os.environ.get('HYPRV_S1_PROBE_NAME','fullscreen-s1-current');out.mkdir(exist_ok=True)
snapshot=out/'snapshot';(snapshot/'fixtures').mkdir(parents=True,exist_ok=True)
fixture=root/'parity/fullscreen/fixtures/idle.json';shutil.copyfile(fixture,snapshot/'fixtures/idle.json')
entries=[dict(state='idle',theme=t,scale=2,file=f'idle--{t}@2.png',crop_logical=[0,0,3072,64],distance_field_text=True) for t in ['dark','light']]
(snapshot/'manifest.json').write_text(json.dumps({'images':entries}))
binary=root/'target/debug/examples/reference'
with tempfile.TemporaryDirectory(prefix='s1-',dir=crate/'.build')as d:
 runtime=Path(d);runtime.chmod(0o700);pinned=runtime/'reference';shutil.copy2(binary,pinned);binary_sha=hashlib.sha256(pinned.read_bytes()).hexdigest()
 assets=runtime/'assets';assets.mkdir()
 for name in ['theme-icons','fixture-icons']:shutil.copytree(crate/'artifacts'/name,assets/name)
 env={**os.environ,'XDG_RUNTIME_DIR':str(runtime),'WAYLAND_DISPLAY':'s1-probe','HYPRV_PARITY_BACKEND':'gl','HYPRV_BAR_REAL_OUTPUT_DPR':'1','HYPRV_BAR_FIXTURE_ASSET_ROOT':str(assets),'HYPRV_BAR_REFERENCE_SNAPSHOT':str(snapshot),'HYPRV_BAR_ACTUAL_DIR':str(out/'actual')}
 with (out/'weston.log').open('w')as log:
  weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=s1-probe'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(runtime/'s1-probe').exists():break
    time.sleep(.05)
   with(out/'render.log').open('w')as log:subprocess.run([str(pinned),'idle'],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  finally:weston.terminate();weston.wait(timeout=10)
receipt={'binary_sha256':binary_sha,'fixture_sha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'regions':[]}
def pm(a):
 a=np.asarray(a.convert('RGBA'),dtype=np.int32);a[:,:,:3]=(a[:,:,:3]*a[:,:,3:]+127)//255;return a
for e in entries:
 ref=Image.open(root/'parity/fullscreen/reference'/e['file']);actual=Image.open(out/'actual'/e['file'])
 # Literal regions include both complete glyphs and a small surrounding margin.
 for name,rect in [('control',[3010,16,3032,44]),('bell',[3035,16,3060,44])]:
  box=tuple(x*2 for x in rect);a=pm(actual.crop(box));b=pm(ref.crop(box));delta=np.abs(a-b)
  receipt['regions'].append(dict(theme=e['theme'],region=name,logical_rect=rect,max=int(delta.max()),absolute_error=int(delta.sum()),pixels_over8=int(np.count_nonzero(delta.max(axis=2)>8))))
(out/'metrics.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
