#!/usr/bin/env python3
"""Independent offline QtSvg vs Qt-free decoder probe. Never runtime code.
The finite corpus reports tolerance (8 max/.5 painted mean) and exact separately.
"""
from pathlib import Path
import argparse, hashlib, json, os, shlex, subprocess
import numpy as np
from PIL import Image
options=argparse.ArgumentParser();options.add_argument('--probe',type=Path);options=options.parse_args()
root=Path(__file__).resolve().parents[1]
workspace=root.parents[1]
out=root/'artifacts/svg-decoder';out.mkdir(parents=True,exist_ok=True)
flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Svg','Qt6Gui'],text=True))
binary=root/'.build/svg-bake';binary.parent.mkdir(exist_ok=True)
subprocess.run(['c++','-std=c++17',str(root/'tools/svg_bake.cpp'),'-o',str(binary),*flags],check=True)
if options.probe is None:subprocess.run(['python3',str(root/'scripts/check.py'),'build','--example','svg_probe'],check=True)
probe=options.probe.resolve() if options.probe else workspace/'target/debug/examples/svg_probe'
sources=[root/'assets/control-panel-dark.svg',root/'assets/control-panel-light.svg',root/'assets/network-wired-light.svg']
signal=Path.home()/'.local/share/icons/Fluent-dark/symbolic/status/nm-signal-75-secure-symbolic.svg'
if signal.exists():sources.append(signal)
def pixels(path):
 a=np.array(Image.open(path).convert('RGBA')).astype(np.int32)
 a[:,:,:3]=(a[:,:,:3]*a[:,:,3:4]+127)//255
 return a
rows=[]
for source in sources:
 for scale in [1,2]:
  name=f'{source.stem}@{scale}';reference=out/f'{name}-qt.png'
  subprocess.run([str(binary),str(source),str(reference),'24','24',str(scale)],env=dict(os.environ,QT_QPA_PLATFORM='offscreen'),check=True)
  expected=pixels(reference)
  for backend in ['gray','resvg']:
   actual=out/f'{name}-{backend}.png'
   args=[str(probe),str(source),str(actual),'24',str(scale)]
   if backend=='resvg':args.append('resvg')
   used=subprocess.check_output(args,text=True).strip()
   a=pixels(actual);delta=np.abs(a-expected);painted=(a[:,:,3]>0)|(expected[:,:,3]>0)
   maximum=int(delta.max());mae=float(delta[painted].mean()) if painted.any() else 0.
   rows.append(dict(icon=source.name,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),scale=scale,logical_size=24,backend=used,max_channel=maximum,painted_mae=mae,tolerance_pass=maximum<=8 and mae<=.5,exact=maximum==0))
report=dict(probe_sha256=hashlib.sha256(probe.read_bytes()).hexdigest(),prebuilt_probe=bool(options.probe),qt_version=subprocess.check_output(['pkg-config','--modversion','Qt6Svg'],text=True).strip(),pixel_space='premultiplied RGBA',cases=rows)
(out/'metrics.json').write_text(json.dumps(report,indent=2))
for backend in ['GrayOutline','Resvg']:
 cases=[r for r in rows if r['backend']==backend]
 print(backend,len(cases),'cases, tolerance passed',sum(r['tolerance_pass']for r in cases),'exact',sum(r['exact']for r in cases),'max',max(r['max_channel']for r in cases),'worst painted MAE',max(r['painted_mae']for r in cases))
