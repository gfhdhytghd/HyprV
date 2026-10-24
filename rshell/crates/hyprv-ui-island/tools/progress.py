#!/usr/bin/env python3
"""Class cohorts stay fixed to initial QA labels; canonical current counts separately."""
from pathlib import Path
import collections,json,sys
import numpy as np
from PIL import Image
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
before=json.loads((crate/'artifacts/exactness-baseline.json').read_text());after=json.loads(Path(sys.argv[1]).read_text());old=before['images'];new={r['file']:r for r in after['images']}
def pm(p):
 a=np.array(Image.open(p).convert('RGBA'),dtype=np.int32);a[:,:,:3]=(a[:,:,:3]*a[:,:,3:4]+127)//255;return a
source=root/'crates/hyprv-ui-bar/artifacts/exact2-final-cross/after/out/island-gl/actual'
for r in old:
 if 'integer_error'not in r:
  d=np.abs(pm(root/'parity/reference/island'/r['file'])-pm(source/r['file']));r['integer_error']=int(d.sum());r['integer_max']=int(d.max())
(crate/'artifacts/exactness-baseline.json').write_text(json.dumps(before,indent=1)+'\n')
rows=[]
for category in ['fill/color','canvas','text','animation','rect AA','svg/icon']:
 rs=[r for r in old if r['cause']==category];ns=[new[r['file']]for r in rs if r['file']in new]
 rows.append(dict(category=category,baseline_images=len(rs),baseline_error=sum(r['integer_error']for r in rs),now_error=sum(r['integer_error']for r in ns),now_passed=sum(r['result']=='pass'for r in ns),now_missing=len(rs)-len(ns)))
result=dict(before=dict(images=len(old),passed=sum(r['result']=='pass'for r in old),integer_error=sum(r['integer_error']for r in old)),after=after['summary'],cohorts=rows,current_causes=dict(collections.Counter(r['cause']for r in new.values()if r['result']!='pass')),lost_passes=[n for n,r in new.items()if next(x for x in old if x['file']==n)['result']=='pass'and r['result']!='pass'])
out=Path(sys.argv[1]).parent/'progress.json';out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
