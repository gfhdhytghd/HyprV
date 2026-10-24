#!/usr/bin/env python3
"""Compare canonical Rust geometry with independently built Qt rectangle nodes.
Build tools/rect_mesh.cpp with installed QtQuickPrivate; cargo build --example
rect_mesh. This diagnostic never edits references or production engine sources.
"""
from pathlib import Path
import hashlib,json,subprocess
import numpy as np
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
qt=crate/'.build/rect-mesh';rust=root/'target/debug/examples/rect_mesh'
out=crate/'artifacts/rect-mesh-sweep';out.mkdir(exist_ok=True);rows=[]
for w,h,r in [(223.375,38,19),(300,230,19),(300,190,14),(272,220,14),(344,180,14),(407.5,38,19),(80,38,10),(300,1,1)]:
 for pen in [0,1]:
  args=list(map(str,[w,pen,h,r]));q=json.loads(subprocess.check_output([str(qt),*args]));s=json.loads(subprocess.check_output([str(rust),*args]));qa=np.array(q['vertices'],dtype=np.float32);sa=np.array(s['vertices'],dtype=np.float32)
  triangles=[]
  for i,(a,b,c) in enumerate(zip(q['indices'],q['indices'][1:],q['indices'][2:])):
   if len({a,b,c})==3:triangles.append([a,b,c] if i%2==0 else[b,a,c])
  rows.append(dict(width=w,height=h,radius=r,pen=pen,qt_vertices=len(qa),rust_vertices=len(sa),max_vertex_component_diff=float(np.max(abs(qa-sa)))if qa.shape==sa.shape else None,triangle_order_exact=triangles==s['triangles']))
(out/'metrics.json').write_text(json.dumps(rows,indent=2)+'\n');(out/'provenance.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest()for p in[qt,rust,crate/'tools/rect_mesh.cpp',crate/'examples/rect_mesh.rs']},indent=2)+'\n')
print(json.dumps(rows,indent=2))
