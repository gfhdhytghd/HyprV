#!/usr/bin/env python3
"""Repeat frozen-reference replays on private GL targets; diagnostic only."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,tempfile,time,shutil
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('name');p.add_argument('--filter');p.add_argument('--runs',type=int,default=2)
p.add_argument('--asset-root',type=Path,help='explicit private diagnostic asset snapshot')
a=p.parse_args()
if not a.name.replace('-','').isalnum():raise SystemExit('local artifact name required')
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
out=crate/'artifacts'/a.name;out.mkdir(exist_ok=True)
binary=root/'target/debug/examples/reference'
fingerprint=dict(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),reference_manifest_sha256=hashlib.sha256((crate/'artifacts/gate-inputs/reference/manifest.json').read_bytes()).hexdigest(),filter=a.filter,runs=a.runs,observed_source_sha256={str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest() for f in [root/'crates/hyprv-ui/src/qt_rect.rs',root/'crates/hyprv-ui/src/render.rs',root/'crates/hyprv-ui/src/text.rs',root/'crates/hyprv-ui/src/effects.rs',crate/'src/popups.rs',crate/'examples/reference.rs']})
(out/'provenance.json').write_text(json.dumps(fingerprint,indent=2))
with tempfile.TemporaryDirectory(prefix='bar-gl-',dir=crate/'.build') as tmp:
 runtime=Path(tmp);runtime.chmod(0o700);env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime))
 pinned=runtime/'reference';shutil.copy2(binary,pinned)
 assets=runtime/'assets'
 if a.asset_root:
  shutil.copytree(a.asset_root,assets)
 else:
  for name in ['theme-icons','fixture-icons']:
   shutil.copytree(crate/'artifacts'/name,assets/name)
  # A prep script atomically deleting/recreating a theme folder must not silently
  # become fallback letters. Snapshot all expected completed files or fail early.
  for theme in ['dark','light']:
   for scale in [1,2]:
    for icon in ['google-chrome','discord','keepassxc','fcitx','wechat']:
     if not (assets/f'theme-icons/{theme}@{scale}/{icon}.png').is_file():
      raise RuntimeError('incomplete prepared icon snapshot; retry after asset preparation')
 asset_hashes={str(f.relative_to(assets)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(assets.rglob('*')) if f.is_file()}
 fingerprint['asset_sha256']=asset_hashes
 (out/'provenance.json').write_text(json.dumps(fingerprint,indent=2))
 env['HYPRV_BAR_FIXTURE_ASSET_ROOT']=str(assets)

 with (out/'weston.log').open('w') as log:
  weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=bar-gl'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(runtime/'bar-gl').exists():break
    time.sleep(.05)
   else:raise RuntimeError('private compositor failed')
   for run in range(a.runs):
    destination=out/f'run-{run}';reference=crate/'artifacts/gate-inputs/reference'
    replay=dict(env,WAYLAND_DISPLAY='bar-gl',HYPRV_PARITY_BACKEND='gl',HYPRV_BAR_REFERENCE_SNAPSHOT=str(reference),HYPRV_BAR_ACTUAL_DIR=str(destination))
    if env.get('HYPRV_REPLAY_PRELOAD'):replay['LD_PRELOAD']=env['HYPRV_REPLAY_PRELOAD']
    with (out/f'run-{run}.log').open('w') as output:
     cmd=[str(pinned)]+([a.filter] if a.filter else [])
     subprocess.run(cmd,env=replay,stdout=output,stderr=subprocess.STDOUT,check=True)
     if env.get('HYPRV_BAR_FULL_TARGET'):
      from PIL import Image
      for entry in json.loads((destination/'coverage.json').read_text())['exported']:
       x,y,w,h=entry['crop'];s=entry['scale'];path=destination/entry['file']
       with Image.open(path) as im:
        cropped=im.crop((round(x*s),round(y*s),round((x+w)*s),round((y+h)*s)))
       cropped.save(path)
     subprocess.run(['python3',str(crate/'scripts/gate_metrics.py'),str(destination),str(reference),str(out/f'gate-{run}')],stdout=output,stderr=subprocess.STDOUT,check=True)
  finally:
   weston.terminate();weston.wait(timeout=10)


