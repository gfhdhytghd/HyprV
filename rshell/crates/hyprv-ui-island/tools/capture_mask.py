#!/usr/bin/env python3
from pathlib import Path
import importlib.util,json,os,subprocess,tempfile,hashlib
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];probe=os.environ.get('QT_PROBE','mask');out=crate/'artifacts'/f'{probe}-probe';out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('v',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
with v.PrivateWeston()as socket,tempfile.TemporaryDirectory(prefix='island-mask-')as tmp:
 fixture=Path(tmp)/f'{probe}-probe.json';fixture.write_text(json.dumps(dict(component='probe',scene=f'{probe}_probe.qml',props={},settle_ms=200,steps=[dict(capture='')])))
 for scale in [1,2]:
  env={**os.environ,'WAYLAND_DISPLAY':socket,'QT_QPA_PLATFORM':'wayland','QSG_RHI_BACKEND':'opengl','QSG_RENDER_LOOP':'basic','QT_WAYLAND_DISABLE_WINDOWDECORATION':'1'}
  with(out/f'qt-{scale}.log').open('w')as log:subprocess.run([str(root/'parity/build/parity-host'),'--fixture',str(fixture),'--scenes',str(crate/'tools'),'--source',str(root.parent/'quickshell'),'--assets',str(root/'parity/fixtures/assets'),'--out',str(out),'--theme','dark','--scale',str(scale)],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  (root/f'crates/hyprv-ui/tests/fixtures/native-gl/{probe}@{scale}.png').write_bytes((out/f'{probe}-probe--dark@{scale}.png').read_bytes())
(out/'oracle.json').write_text(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in [root/'parity/build/parity-host',crate/f'tools/{probe}_probe.qml',*sorted((root/'crates/hyprv-ui/tests/fixtures/native-gl').glob(f'{probe}@*.png'))]},indent=2)+'\n')
