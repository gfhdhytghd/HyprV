#!/usr/bin/env python3
"""Current bar adapter against current QA references, with private pinned inputs."""
from pathlib import Path
import importlib.util,subprocess,tempfile,os,shutil,hashlib,json,argparse
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--name',default='bar-current');args=parser.parse_args();assert args.name.replace('-','').isalnum()
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];bar=root/'crates/hyprv-ui-bar';out=crate/'artifacts'/args.name;out.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('v',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
refs=root/'parity/reference/bar';binary=root/'target/debug/examples/reference';receipt={'reference_manifest':hashlib.sha256((refs/'manifest.json').read_bytes()).hexdigest()}
def sources():
 return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for name in ['hyprv-ui','hyprv-ui-bar']for p in(root/'crates'/name).rglob('*.rs')if not any(x in p.parts for x in ['artifacts','.build','target'])}
def reference_hashes():
 return {p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in refs.iterdir()if p.suffix in ['.png','.json']}
receipt['sources_before']=sources();receipt['references_before']=reference_hashes()

with tempfile.TemporaryDirectory(prefix='island-bar-')as tmp,v.PrivateWeston()as socket:
 assets=Path(tmp)/'assets'
 for name in ['theme-icons','fixture-icons']:shutil.copytree(bar/'artifacts'/name,assets/name)
 # Current QA references use live device-pixel requests at DPR1. Bake privately;
 # never delete/rebuild the shared icon directories while other agents read them.
 for theme in ['dark','light']:
  for scale in [1,2]:
   destination=assets/f'theme-icons/{theme}@{scale}';shutil.rmtree(destination)
   subprocess.run([str(bar/'.build/theme-icons'),str(destination),str(18*scale),'hicolor','1'],env={**os.environ,'QT_QPA_PLATFORM':'offscreen'},capture_output=True,check=True)
  destination=assets/f'theme-icons/{theme}@2-initial';shutil.rmtree(destination,ignore_errors=True)
  subprocess.run([str(bar/'.build/theme-icons'),str(destination),'18','hicolor','1'],env={**os.environ,'QT_QPA_PLATFORM':'offscreen'},capture_output=True,check=True)
 receipt['assets']={str(p.relative_to(assets)):hashlib.sha256(p.read_bytes()).hexdigest()for p in assets.rglob('*')if p.is_file()}

 with binary.open('rb')as pinned:
  receipt['binary_sha256']=hashlib.sha256(pinned.read()).hexdigest();pinned.seek(0)
  with(out/'render.log').open('w')as log:subprocess.run([f'/proc/self/fd/{pinned.fileno()}'],pass_fds=(pinned.fileno(),),env={**os.environ,'WAYLAND_DISPLAY':socket,'HYPRV_PARITY_BACKEND':'gl','HYPRV_BAR_REFERENCE_SNAPSHOT':str(refs),'HYPRV_BAR_ACTUAL_DIR':str(out/'actual'),'HYPRV_BAR_FIXTURE_ASSET_ROOT':str(assets)},stdout=log,stderr=subprocess.STDOUT,check=True)
 with(out/'metrics.log').open('w')as log:subprocess.run(['python3',str(bar/'scripts/gate_metrics.py'),str(out/'actual'),str(refs),str(out/'gate')],stdout=log,stderr=subprocess.STDOUT,check=True)
receipt['sources_after']=sources();receipt['references_after']=reference_hashes();receipt['source_changes']=[p for p,h in receipt['sources_before'].items()if receipt['sources_after'].get(p)!=h];receipt['reference_changes']=[p for p,h in receipt['references_before'].items()if receipt['references_after'].get(p)!=h]
(out/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
