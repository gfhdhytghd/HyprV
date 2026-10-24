#!/usr/bin/env python3
"""Finite GL replay and canonical strict gate for the user's assigned panel share."""
from pathlib import Path
import argparse, hashlib, importlib.util, json, os, subprocess

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('name');p.add_argument('--filter');p.add_argument('--no-render',action='store_true')
a=p.parse_args();assert a.name.replace('-','').isalnum()
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
out=crate/'artifacts/core-panels'/a.name;out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('verify',root/'parity/verify/verify.py')
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v);v.OUT=out
def hashes(paths):return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in paths}
sources=lambda:hashes(p for c in ['hyprv-ui','hyprv-ui-panels']for d in ['src','tests']for p in(root/'crates'/c/d).rglob('*.rs'))
refs=lambda:hashes(p for p in(root/'parity/reference/panels').iterdir()if p.suffix in ['.png','.json'])
before=sources();ref_before=refs()
if not a.no_render:
    env={**os.environ,'CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(root/'target'),'HYPRV_PARITY_BACKEND':'gl','HYPRV_PANELS_PARITY_OUT':str(out/'raw')}
    if a.filter:env['PANELS_FIXTURE']=a.filter
    build=subprocess.run(['cargo','test','-p','hyprv-ui-panels','--no-default-features','--test','shared_parity','--no-run','--message-format=json'],cwd=root,env=env,capture_output=True,text=True)
    (out/'build.log').write_text(build.stderr)
    if build.returncode:raise SystemExit('Build failed; see build.log')
    artifacts=[json.loads(line)for line in build.stdout.splitlines()if line.startswith('{')]
    binary=Path(next(e['executable']for e in artifacts if e.get('reason')=='compiler-artifact'and e.get('executable')and e['target']['name']=='shared_parity'))
    with v.PrivateWeston()as socket,(out/'render.log').open('w')as log,binary.open('rb')as pinned:
        env['WAYLAND_DISPLAY']=socket
        binary_hash=hashlib.sha256(pinned.read()).hexdigest();pinned.seek(0)
        proc=subprocess.run([f'/proc/self/fd/{pinned.fileno()}','--nocapture'],pass_fds=(pinned.fileno(),),cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
        pinned.seek(0);assert binary_hash==hashlib.sha256(pinned.read()).hexdigest(),'opened executable changed'
    count,status=v.publish('panels-gl',{p.name[5:]:p for p in(out/'raw').glob('mine-*.png')},proc.returncode)
    import shutil
    shutil.rmtree(out/'raw')
    (out/'render.json').write_text(json.dumps(dict(rendered=count,status=status,exit=proc.returncode,binary=str(binary),binary_sha256=binary_hash),indent=2))
manifest=json.loads((root/'parity/reference/panels/manifest.json').read_text())['images']
rows=[v.compare('panels',e,out/'panels-gl/actual',{})for e in manifest if not e['state'].startswith(('qa-','calendar-','startup-')) and (not a.filter or a.filter in e['state'])]
v.relabel_animation(rows)
groups={}
for r in rows:
    s=r['state'];g='Stats'if s.startswith('stats-')else'Wi-Fi'if s.startswith('wifi-')else'Bluetooth'if s.startswith('cp-bt-')else'Mihomo'if s.startswith('cp-mihomo-')else'VM/Viewflow'if s.startswith('cp-vm-')else'Control panel'
    groups.setdefault(g,[]).append(r)
table=[dict(group=g,images=len(rs),passed=sum(r['result']=='pass'for r in rs),exact=sum(r['max']==0 for r in rs),max=max(r['max']for r in rs),painted_mean=max(r['mean']for r in rs))for g,rs in groups.items()]
summary=dict(images=len(rows),passed=sum(r['result']=='pass'for r in rows),exact=sum(r['max']==0 for r in rows),table=table)
(out/'metrics.json').write_text(json.dumps(dict(summary=summary,images=rows),indent=1)+'\n')
after=sources();ref_after=refs()
(out/'provenance.json').write_text(json.dumps(dict(before=before,after=after,source_changes=[n for n,h in before.items()if after.get(n)!=h],references_before=ref_before,references_after=ref_after,reference_changes=[n for n,h in ref_before.items()if ref_after.get(n)!=h]),indent=1)+'\n')
print(json.dumps(summary,indent=2))
