#!/usr/bin/env python3
"""Generate independent engine goldens; does not change canonical parity data."""
from pathlib import Path
import hashlib, importlib.util, json, os, subprocess, tempfile
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
out=crate/'artifacts/core-panels/layer-oracle';out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('verify',root/'parity/verify/verify.py')
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
paths=[crate/'tools/core_layer_probe.qml',root/'parity/build/parity-host']
with v.PrivateWeston()as socket,tempfile.TemporaryDirectory(prefix='panel-layer-')as tmp:
    fixture=Path(tmp)/'layer-projection.json'
    fixture.write_text(json.dumps(dict(component='probe',scene='core_layer_probe.qml',props={},settle_ms=200,steps=[dict(capture='')])))
    for scale in [1,2]:
        env={**os.environ,'WAYLAND_DISPLAY':socket,'QT_QPA_PLATFORM':'wayland','QSG_RHI_BACKEND':'opengl','QSG_RENDER_LOOP':'basic','QT_WAYLAND_DISABLE_WINDOWDECORATION':'1'}
        with(out/f'qt-{scale}.log').open('w')as log:
            subprocess.run([str(root/'parity/build/parity-host'),'--fixture',str(fixture),'--scenes',str(crate/'tools'),'--source',str(root.parent/'quickshell'),'--assets',str(root/'parity/fixtures/assets'),'--out',str(out),'--theme','dark','--scale',str(scale)],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        target=root/f'crates/hyprv-ui/tests/fixtures/native-gl/layer-projection@{scale}.png'
        target.write_bytes((out/f'layer-projection--dark@{scale}.png').read_bytes());paths.append(target)
(out/'oracle.json').write_text(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in paths},indent=2)+'\n')
