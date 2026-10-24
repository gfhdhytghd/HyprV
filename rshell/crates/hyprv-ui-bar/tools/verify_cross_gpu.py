#!/usr/bin/env python3
"""Pin copied island/panels test translators using the engine offscreen backend.
Owners' source files are untouched. Compare disabled/enabled native GL on the
same linked test binaries with the actual canonical verifier.
"""
from pathlib import Path
import hashlib,importlib.util,json,os,shutil,subprocess,sys,re
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
base=crate/'artifacts'/(sys.argv[1] if len(sys.argv)>1 else 'native-cross-gpu');source=base/'source';source.mkdir(parents=True,exist_ok=True)
files={'island.rs':root/'crates/hyprv-ui-island/tests/parity.rs','common/mod.rs':root/'crates/hyprv-ui-island/tests/common/mod.rs','panels.rs':root/'crates/hyprv-ui-panels/tests/shared_parity.rs','shared_common/mod.rs':root/'crates/hyprv-ui-panels/tests/shared_common/mod.rs','shared_common/translate.rs':root/'crates/hyprv-ui-panels/tests/shared_common/translate.rs','calendar.rs':root/'crates/hyprv-ui-island/tests/calendar_morph.rs'}
original={}
(source/'shared_common').mkdir(exist_ok=True)
shutil.copyfile(root/'crates/hyprv-ui-panels/tests/shared_common/targets.json',source/'shared_common/targets.json')
for name,path in files.items():
 s=path.read_text();original[str(path)]=hashlib.sha256(s.encode()).hexdigest()
 s=s.replace('Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")',f'PathBuf::from("{root}")')
 if name=='calendar.rs':
  s=s.replace('../../hyprv-ui-panels/tests/shared_common/targets.json','shared_common/targets.json')
  s=s.replace('#[path = "../../hyprv-ui-panels/tests/shared_common/translate.rs"]','#[path = "shared_common/translate.rs"]')
 p=source/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s)
manifest='[package]\nname="hyprv-native-cross-parity"\nversion="0.1.0"\nedition="2021"\n[workspace]\n[dependencies]\nserde_json="1"\nwayland-client={version="0.31",features=["system"]}\nchrono={version="0.4",default-features=false,features=["clock","std"]}\n'
for package in ['hyprv-ui','hyprv-ui-island','hyprv-ui-panels','hyprv-widgets']:
 manifest+=f'{package}={{path="{root}/crates/{package}",default-features=false}}\n'
for test in ['island','panels','calendar']:manifest+=f'\n[[test]]\nname="{test}"\npath="{test}.rs"\n'
(source/'Cargo.toml').write_text(manifest)
if shutil.disk_usage(root).free < 30 * 1024**3:raise SystemExit('Disk policy: need 30 GiB free before Cargo build')
env={**os.environ,'CARGO_TARGET_DIR':str(root/'target'),'CARGO_INCREMENTAL':'0'}
with (base/'build.jsonl').open('w')as log, (base/'build.stderr').open('w')as err:
 subprocess.run(['cargo','test','--manifest-path',str(source/'Cargo.toml'),'--tests','--no-run','--message-format=json'],env=env,stdout=log,stderr=err,check=True)
binaries={}
for line in (base/'build.jsonl').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('executable'):binaries[d['target']['name']]=d['executable']
(base/'provenance.json').write_text(json.dumps(dict(original=original,binaries={k:dict(path=v,sha256=hashlib.sha256(Path(v).read_bytes()).hexdigest())for k,v in binaries.items()}),indent=2))
for phase in ['before','after']:
 spec=importlib.util.spec_from_file_location('canonical_verify',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
 v.OUT=base/phase/'out';v.REPORT=base/phase/'report.md'
 def render_island(slot,env):
  raw=v.OUT/slot/'raw';raw.mkdir(parents=True,exist_ok=True)
  e={**env,'HYPRV_ISLAND_PARITY_OUT':str(raw)}
  if phase=='before':e['HYPRV_NATIVE_GL_DISABLE']='1'
  code,secs=v.run([binaries['island'],'--nocapture'],env=e,log=v.OUT/slot/'render.log')
  n,status=v.publish(slot,{f.name[5:]:f for f in raw.glob('mine-*.png')},code)
  return dict(status=status,rendered=n,seconds=round(secs),exit=code,masks={})
 def render_panels(slot,env):
  raw=v.OUT/slot/'raw';morph=v.OUT/slot/'morph';raw.mkdir(parents=True,exist_ok=True);morph.mkdir(parents=True,exist_ok=True)
  e={**env,'HYPRV_PANELS_PARITY_OUT':str(raw),'HYPRV_CALENDAR_PARITY_OUT':str(morph)}
  if phase=='before':e['HYPRV_NATIVE_GL_DISABLE']='1'
  code,secs=v.run([binaries['panels'],'--nocapture'],env=e,log=v.OUT/slot/'render.log')
  v.run([binaries['calendar'],'--nocapture'],env=e,log=v.OUT/slot/'calendar.log')
  fs={f.name[5:]:f for directory in [raw,morph]for f in directory.glob('mine-*.png')};n,status=v.publish(slot,fs,code)
  return dict(status=status,rendered=n,seconds=round(secs),exit=code,masks={})
 v.RENDERERS['island']=render_island;v.RENDERERS['panels']=render_panels;v.gpu_hook_wired=lambda c:True
 sys.argv=[str(root/'parity/verify/verify.py'),'--component','island','--component','panels','--backend','gl']
 v.main()
