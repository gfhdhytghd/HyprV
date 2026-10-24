#!/usr/bin/env python3
from pathlib import Path
import hashlib,importlib.util,json,os,subprocess,tempfile
import numpy as np
from PIL import Image
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];out=crate/'artifacts/scale-probe';out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('v',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
with v.PrivateWeston() as socket, tempfile.TemporaryDirectory(prefix='island-probe-') as t:
 env={**os.environ,'WAYLAND_DISPLAY':socket,'HYPRV_PARITY_BACKEND':'gl','QT_QPA_PLATFORM':'wayland','QSG_RHI_BACKEND':'opengl','QSG_RENDER_LOOP':'basic','QT_WAYLAND_DISABLE_WINDOWDECORATION':'1'}
 with(out/'rust.log').open('w')as log:subprocess.run([str(root/'target/debug/examples/native_scale'),str(out/'rust')],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 fixture=Path(t)/'scale-probe.json';fixture.write_text(json.dumps(dict(component='probe',scene='native_scale.qml',props={},settle_ms=200,steps=[dict(capture='')])))
 for dark in [True,False]:
  for scale in [1,2]:
   source_dir=out/f'sources-{dark}-{scale}';source_dir.mkdir(exist_ok=True);env['HYPRV_CANVAS_SOURCE_DIR']=str(source_dir)
   with(out/f'qt-{dark}-{scale}.log').open('w')as log:subprocess.run([os.environ.get('HYPRV_QT_HOST',str(root/'parity/build/parity-host')),'--fixture',str(fixture),'--scenes',str(crate/'tools'),'--source',str(root.parent/'quickshell'),'--assets',str(root/'parity/fixtures/assets'),'--out',str(out/'qt'),'--theme','dark'if dark else'light','--scale',str(scale)],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
rows=[]
for q in sorted((out/'qt').glob('*.png')):
 def pm(p):
  x=np.array(Image.open(p).convert('RGBA'),dtype=int);x[:,:,:3]=(x[:,:,:3]*x[:,:,3:4]+127)//255;return x
 d=abs(pm(q)-pm(out/'rust'/q.name));rows.append(dict(file=q.name,max=int(d.max()),error=int(d.sum()),pixels=int(np.any(d,axis=2).sum())))
(out/'metrics.json').write_text(json.dumps(dict(cases=rows,source_sha256=hashlib.sha256((crate/'tools/native_scale.qml').read_bytes()).hexdigest()),indent=2)+'\n');print(rows)
