#!/usr/bin/env python3
"""Publish unmasked native and independent full-screen measurements for a pinned ELF."""
import argparse,hashlib,json
from pathlib import Path
from verify_corners import verify as verify_corners
p=argparse.ArgumentParser(description=__doc__);p.add_argument('run',type=Path);p.add_argument('binary',type=Path);a=p.parse_args()
run=a.run.resolve();matrix=json.loads((run/'fullscreen-results.json').read_text());rows=json.loads((run/'compare/compare.json').read_text())
assert len(matrix['jobs'])==26 and all(j['runtime_ok']for j in matrix['jobs'])
assert len(rows)==64 and all('gate'in r for r in rows)
for r in rows:
 n=r['engine_comparison'];g=r['gate'];assert n['pass']==g['pass'];assert n['max_channel_diff']==g['max_channel_diff']
 assert abs(n['painted_mae']-g['painted_mae'])<=.000051
summary={'binary':str(a.binary),'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'jobs':26,'images':64,'passed':sum(r['gate']['pass']for r in rows),'exact':sum(r['gate']['max_channel_diff']==0 for r in rows),'max_channel_diff':max(r['gate']['max_channel_diff']for r in rows),'max_painted_mae':max(r['gate']['painted_mae']for r in rows),'min_painted_iou':min(r['gate']['painted_iou']for r in rows),'native_independent_agree':True,'masked_pixels':0,'states':{}}
for state in sorted({r['state']for r in rows}):
 rs=[r for r in rows if r['state']==state];summary['states'][state]={'total':len(rs),'passed':sum(r['gate']['pass']for r in rs),'themes':{t:{'max':max(r['gate']['max_channel_diff']for r in rs if r['theme']==t),'max_mae':max(r['gate']['painted_mae']for r in rs if r['theme']==t)}for t in ['light','dark']}}
corners=verify_corners(run);(run/'corner-comparison.json').write_text(json.dumps(corners,indent=2)+'\n')
summary['corners']={k:v for k,v in corners.items()if k!='rows'}
lines=['| State | Passed | Light max / worst MAE | Dark max / worst MAE |','|---|---:|---:|---:|']
for state,g in summary['states'].items():
 t=g['themes'];lines.append(f"| {state} | {g['passed']}/{g['total']} | {t['light']['max']} / {t['light']['max_mae']:.4f} | {t['dark']['max']} / {t['dark']['max_mae']:.4f} |")
(run/'state-table.md').write_text('\n'.join(lines)+'\n')
frames=['| State | Frame | Light max / MAE / IoU | Dark max / MAE / IoU |','|---|---|---:|---:|']
for state,frame in sorted({(r['state'],r['frame'])for r in rows}):
 def cell(theme):
  r=next(r for r in rows if r['state']==state and r['frame']==frame and r['theme']==theme);g=r['gate'];return f"{'PASS'if g['pass']else'FAIL'} {g['max_channel_diff']} / {g['painted_mae']:.4f} / {g['painted_iou']:.5f}"
 frames.append(f"| {state} | {frame or '—'} | {cell('light')} | {cell('dark')} |")
(run/'frame-table.md').write_text('\n'.join(frames)+'\n');(run/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps({k:v for k,v in summary.items()if k!='states'},indent=2))
