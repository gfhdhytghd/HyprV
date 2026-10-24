#!/usr/bin/env python3
"""Owned finite GL replay; canonical gate unchanged, plus integer-PM diagnostics."""
from pathlib import Path
import argparse,collections,hashlib,importlib.util,json,os,shutil,sys,subprocess,time
import numpy as np
from PIL import Image
p=argparse.ArgumentParser(description=__doc__);p.add_argument('name');p.add_argument('--filter');p.add_argument('--no-render',action='store_true');p.add_argument('--binary',type=Path);a=p.parse_args()
assert a.name.replace('-','').isalnum()
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];out=crate/'artifacts'/a.name;out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('verify',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v);v.OUT=out
os.environ['CARGO_TARGET_DIR']=str(root/'target');os.environ['CARGO_INCREMENTAL']='0'
def source_hashes():
 return {str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest()for d in ['hyprv-ui-island','hyprv-ui']for f in(root/'crates'/d/'src').rglob('*.rs')}
started_sources=source_hashes()
def reference_hashes():
 return {p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in(root/'parity/reference/island').iterdir()if p.suffix in ['.png','.json']}
started_references=reference_hashes()
if a.filter:os.environ['ISLAND_FIXTURE']=a.filter
if not a.no_render:
 if shutil.disk_usage(root).free<30*1024**3:raise SystemExit('Disk policy: less than30GiB free')
 (out/'island-gl').mkdir(exist_ok=True)
 with v.PrivateWeston() as socket:
  env={v.GPU_ENV:'gl','WAYLAND_DISPLAY':socket}
  if a.binary:
   binary=a.binary.resolve();raw=out/'island-gl/raw';raw.mkdir(exist_ok=True)
   with binary.open('rb') as pinned:
    before=hashlib.sha256(pinned.read()).hexdigest();pinned.seek(0);started=time.time()
    proc=subprocess.run([f'/proc/self/fd/{pinned.fileno()}','--nocapture'],pass_fds=(pinned.fileno(),),cwd=root,env={**os.environ,**env,'HYPRV_ISLAND_PARITY_OUT':str(raw)},capture_output=True,text=True)
    secs=time.time()-started;code=proc.returncode;(out/'island-gl/render.log').write_text(proc.stdout+'\n--- stderr ---\n'+proc.stderr)
    pinned.seek(0);assert before==hashlib.sha256(pinned.read()).hexdigest(),'opened executable inode changed'
   count,status=v.publish('island-gl',{f.name[5:]:f for f in raw.glob('mine-*.png')},code);shutil.rmtree(raw)
   render=dict(status=status,rendered=count,exit=code,seconds=secs,binary_sha256=before)
  else:render=v.render_island('island-gl',env)
 (out/'render.json').write_text(json.dumps(render,indent=2))
 if not render['rendered']:raise SystemExit('Render failed; see render.log')
actual=out/'island-gl/actual';manifest=json.loads((root/'parity/reference/island/manifest.json').read_text());rows=[]
def pm(path):
 x=np.asarray(Image.open(path).convert('RGBA'),dtype=np.int32);x[:,:,:3]=(x[:,:,:3]*x[:,:,3:4]+127)//255;return x
for e in manifest['images']:
 if not(actual/e['file']).exists():continue
 m=v.compare('island',e,actual,{});d=np.abs(pm(root/'parity/reference/island'/e['file'])-pm(actual/e['file']));m.update(integer_max=int(d.max()),integer_error=int(d.sum()),integer_pixels=int(np.any(d,axis=2).sum()));rows.append(m)
v.relabel_animation(rows)
summary=dict(images=len(rows),passed=sum(x['result']=='pass'for x in rows),integer_exact=sum(x['integer_max']==0 for x in rows),causes=dict(collections.Counter(x['cause']for x in rows if x['result']!='pass')),integer_error=sum(x['integer_error']for x in rows),worst_max=max((x['max']for x in rows),default=0))
(out/'metrics.json').write_text(json.dumps(dict(summary=summary,images=rows),indent=1)+'\n')
finished_references=reference_hashes();(out/'reference-hashes.json').write_text(json.dumps(dict(before=started_references,after=finished_references,changed=[n for n in started_references if started_references[n]!=finished_references.get(n)]),indent=1)+'\n');
finished_sources=source_hashes();(out/'source-hashes.json').write_text(json.dumps(dict(before=started_sources,after=finished_sources,changed=[n for n in started_sources if started_sources[n]!=finished_sources.get(n)]),indent=1)+'\n');print(json.dumps(summary,indent=2))
# Keep a few representative diagnostics, rather than duplicate every render.
worst={r['file']for r in sorted(rows,key=lambda x:x['integer_error'],reverse=True)[:8]}
for folder in ['heat','tri']:
 for f in(out/'island-gl'/folder).glob('*.png'):
  if f.name not in worst:f.unlink()
