#!/usr/bin/env python3
"""Canonical current GPU replay, compared with retained pre-island-work receipts."""
from pathlib import Path
import importlib.util,json,os,hashlib,sys
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];out=crate/'artifacts/panels-cross'
spec=importlib.util.spec_from_file_location('verify',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
sources={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for name in ['hyprv-ui','hyprv-ui-island','hyprv-ui-panels']for p in(root/'crates'/name/'src').rglob('*.rs')};v.OUT=out;v.REPORT=out/'report.md';os.environ['CARGO_TARGET_DIR']=str(root/'target');os.environ['CARGO_INCREMENTAL']='0'
if '--no-render' in sys.argv: render=json.loads((out/'render.json').read_text()) if (out/'render.json').exists() else {'status':'reused'}
else:
 with v.PrivateWeston() as socket: render=v.render_panels('panels-gl',{'WAYLAND_DISPLAY':socket,v.GPU_ENV:'gl'})
 (out/'render.json').write_text(json.dumps(render,indent=2))
manifest=json.loads((root/'parity/reference/panels/manifest.json').read_text());rows=[v.compare('panels',e,out/'panels-gl/actual',{})for e in manifest['images']];v.relabel_animation(rows)
old=json.loads((root/'crates/hyprv-ui-bar/artifacts/exact2-final-cross/after/out/results.json').read_text());before={r['file']:r for r in old['results']['panels-gl']}
unchanged=[];changed=[]
for r in rows:
 a=root/'crates/hyprv-ui-bar/artifacts/exact2-final-cross/after/out/panels-gl/actual'/r['file'];b=out/'panels-gl/actual'/r['file']
 (unchanged if a.read_bytes()==b.read_bytes() else changed).append(r['file'])
result=dict(render=render,images=rows,before_passed=sum(r['result']=='pass'for r in before.values()),passed=sum(r['result']=='pass'for r in rows),lost_passes=[r['file']for r in rows if before[r['file']]['result']=='pass'and r['result']!='pass'],unchanged=unchanged,changed=changed)
(out/'source-hashes.json').write_text(json.dumps(dict(before=sources,changed=[n for n,h in sources.items()if hashlib.sha256((root/n).read_bytes()).hexdigest()!=h]),indent=1)+'\n')
(out/'results.json').write_text(json.dumps(result,indent=1)+'\n');print({k:v for k,v in result.items()if k not in ['images','unchanged','changed']});print('PNG unchanged',len(unchanged),'changed',len(changed))
for d in ['tri','heat']:
 for p in(out/'panels-gl'/d).glob('*.png'):p.unlink()
