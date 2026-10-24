#!/usr/bin/env python3
"""Canonical GPU component controls in private output; no QA reports/references mutated."""
import hashlib, importlib.util, json, os, sys
from pathlib import Path
root=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('verify',root/'parity/verify/verify.py')
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
out=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else root/'crates/hyprv-shell/artifacts/fullscreen-core/components';out.mkdir(parents=True,exist_ok=True)
v.OUT=out
paths=[p for c in v.WATCHED for d in ['src','tests','examples'] for p in (root/'crates'/c/d).rglob('*.rs')]
paths += [p for c in v.COMPONENTS for p in (root/'parity/reference'/c).glob('*') if p.suffix in ['.json','.png']]
def hashes():return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
before=hashes();rows={}
with v.PrivateWeston() as socket:
 for c in v.COMPONENTS:
  slot=c+'-gl';(out/slot).mkdir(exist_ok=True)
  render=v.RENDERERS[c](slot,{'HYPRV_PARITY_BACKEND':'gl','WAYLAND_DISPLAY':socket,'CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(root/'target')})
  entries=json.loads((root/'parity/reference'/c/'manifest.json').read_text())['images']
  if c=='bar':
   exported={e['file'] for e in json.loads((root/'crates/hyprv-ui-bar/artifacts/reference-actual/coverage.json').read_text())['exported']}
   entries=[e for e in entries if e['file'] in exported]
  rs=[v.compare(c,e,out/slot/'actual',render.get('masks',{})) for e in entries]
  v.relabel_animation(rs)
  rows[c]={'render':render,'rows':rs}
  (out/(c+'.json')).write_text(json.dumps(rows[c],indent=2))
  print(c, len(rs), {s:sum(r['result']==s for r in rs) for s in set(r['result'] for r in rs)},flush=True)
after=hashes();drift=[p for p in before if before[p]!=after.get(p)]
(out/'receipt.json').write_text(json.dumps({'before':before,'after':after,'drift':drift,'components':{c:{'total':len(r['rows']),'counts':{s:sum(x['result']==s for x in r['rows']) for s in set(x['result'] for x in r['rows'])},'render':r['render']}for c,r in rows.items()}},indent=2))
assert not drift,drift
