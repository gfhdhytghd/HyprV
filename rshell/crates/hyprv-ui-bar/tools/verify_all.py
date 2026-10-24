#!/usr/bin/env python3
"""Run the canonical parity/verify evaluator with private artifact publication.
Only output routing is adapted; references, renderers and comparison rules remain
canonical. Use a distinct run name for each invocation during parallel work.
"""
from pathlib import Path
import importlib.util,json,os,sys,time
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
if len(sys.argv)<2:raise SystemExit('usage: verify_all.py RUN_NAME [canonical verifier options]')
name=sys.argv[1];options=sys.argv[2:]
if not name.replace('-','').isalnum():raise SystemExit('run name must be alphanumeric/hyphen')
base=crate/'artifacts'/name;base.mkdir(exist_ok=True)
spec=importlib.util.spec_from_file_location('canonical_verify',root/'parity/verify/verify.py')
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
v.OUT=base/'out';v.REPORT=base/'report.md'
canonical_run=v.run
def routed_run(cmd,env=None,**kwargs):
 # The current verifier uses an older calendar output variable spelling.
 # Match the fixture translator's actual variable without changing its replay.
 if 'calendar_morph' in cmd and env and 'HYPRV_CALENDAR_MORPH_OUT' in env:
  env={**env,'HYPRV_CALENDAR_PARITY_OUT':env['HYPRV_CALENDAR_MORPH_OUT']}
 return canonical_run(cmd,env=env,**kwargs)
v.run=routed_run

def render_bar(slot='bar',env=None):
 started=time.time();source=base/f'{slot}-actual';report=base/f'{slot}-adapter'
 code,secs=v.run(['python3',str(crate/'scripts/parity.py')],env={**(env or {}),'HYPRV_BAR_ACTUAL_DIR':str(source),'HYPRV_BAR_PARITY_REPORT_DIR':str(report)},log=v.OUT/slot/'render.log')
 masks={};files={};coverage=source/'coverage.json'
 if coverage.exists() and coverage.stat().st_mtime>=started:
  for entry in json.loads(coverage.read_text()).get('exported',[]):
   f=source/entry['file']
   if f.exists():files[entry['file']]=f;masks[entry['file']]=entry.get('foreign_island_local_rect')
 n,status=v.publish(slot,files,code)
 return dict(status=status,rendered=n,seconds=round(secs),exit=code,masks=masks)
v.RENDERERS['bar']=render_bar
# Reject experimental env overrides inherited from other runs.
for key in ['HYPRV_QT_TIE','HYPRV_BAR_RECT_MODE','HYPRV_BAR_PROBE_F16','HYPRV_BAR_HIDE_TEXT']:
 os.environ.pop(key,None)
sys.argv=[str(root/'parity/verify/verify.py'),*options]
v.main()
