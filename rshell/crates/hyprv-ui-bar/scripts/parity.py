#!/usr/bin/env python3
"""QML-state-matched comparisons. All output/masks stay inside this crate.
Full comparison preserves foreign island pixels. A second, explicitly scoped
comparison clears the foreign island rectangle in BOTH inputs, never the bar.
"""
from pathlib import Path
import json, math, os, subprocess,shlex,shutil
from PIL import Image, ImageDraw, ImageChops, ImageStat
crate=Path(__file__).resolve().parents[1]
workspace=crate.parents[1]
reference=workspace/'parity/reference/bar'
actual=Path(os.environ.get('HYPRV_BAR_ACTUAL_DIR',crate/'artifacts/reference-actual'))
report=Path(os.environ.get('HYPRV_BAR_PARITY_REPORT_DIR',crate/'artifacts/parity'))
report.mkdir(parents=True,exist_ok=True)
if not reference.exists() or not any(reference.rglob('*.png')):
 (report/'report.md').write_text('# Bar pixel parity\n\nUNAVAILABLE: no QML bar references in parity/reference/bar. Offscreen PNG export does not establish pixel parity.\n')
 print('UNAVAILABLE: no QML bar references; report:', report/'report.md')
 raise SystemExit(0)
# Freeze available inputs before rendering: other agents may be recapturing this
# shared directory. Never read half of a capture set later in the comparison.
source_reference=reference
snapshot=report/'inputs/current-reference';shutil.rmtree(snapshot,ignore_errors=True);snapshot.mkdir(parents=True)
(snapshot/'fixtures').mkdir()
if (reference/'regions').exists():shutil.copytree(reference/'regions',snapshot/'regions')
source_manifest=json.loads((reference/'manifest.json').read_text())
available=[];deferred=[]
for entry in source_manifest['images']:
 source=reference/entry['file']
 try:
  data=source.read_bytes()
  import io
  with Image.open(io.BytesIO(data)) as check:check.verify()
 except (FileNotFoundError,OSError):
  deferred.append(entry['file']);continue
 (snapshot/entry['file']).write_bytes(data)
 available.append(entry)
 fixture=workspace/'parity/fixtures/bar'/f"{entry['state']}.json"
 shutil.copyfile(fixture,snapshot/'fixtures'/fixture.name)
source_manifest['images']=available
(snapshot/'manifest.json').write_text(json.dumps(source_manifest,indent=2))
(report/'deferred.json').write_text(json.dumps(deferred,indent=2))
if not available:raise SystemExit('No complete QML inputs available while captures are in progress')
reference=snapshot
asset_root=report/'inputs/assets';asset_root.mkdir(parents=True,exist_ok=True)
# Match QML's platform theme icon provider, including inherited-theme lookup.
helper=crate/'.build/theme-icons';helper.parent.mkdir(parents=True,exist_ok=True)
flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Widgets'],text=True))
subprocess.run(['c++','-std=c++17',str(crate/'scripts/theme-icons.cpp'),'-o',str(helper),*flags],check=True)
for dark in [False,True]:
 for scale in [1,2]:
  # Captured bar references resolve app icons through hicolor (verified on
  # Chrome/Discord/KeePassXC/Fcitx and missing Telegram); network assets are Fluent.
  theme='hicolor'
  destination=asset_root/f"theme-icons/{'dark' if dark else 'light'}@{scale}"
  shutil.rmtree(destination,ignore_errors=True)
  qt_env=dict(os.environ);qt_env['QT_QPA_PLATFORM']='offscreen'
  # Live shell: request = sourceSize x DPR device px, resolved at DPR 1 (parity QA /
  # spec-panels nested measurement): pixmap(QSize(18*scale), 1.0), never 64-72 px.
  subprocess.run([str(helper),str(destination),str(18*scale),theme,'1'],env=qt_env,check=True)
# The captured network-offline fixture resolves its IconImage before the window
# has DPR 2: Qt texture-size trace proves the provider got 18 rather than 36.
# This is fixture input preparation, not state-dependent production icon code.
for dark in [False,True]:
 destination=asset_root/f"theme-icons/{'dark' if dark else 'light'}@2-initial"
 shutil.rmtree(destination,ignore_errors=True)
 qt_env=dict(os.environ,QT_QPA_PLATFORM='offscreen')
 subprocess.run([str(helper),str(destination),'18','hicolor','1'],env=qt_env,check=True)
# Raster sourceSize is logical raster pixels; Qt scales SVG sources by DPR.
scale_helper=crate/'.build/image-scale'
flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Gui'],text=True))
subprocess.run(['c++','-std=c++17',str(crate/'tools/image_scale.cpp'),'-o',str(scale_helper),*flags],check=True)
fixture_icons=asset_root/'fixture-icons';fixture_icons.mkdir(exist_ok=True)
for scale in [1,2]:
 subprocess.run([str(scale_helper),str(workspace/'parity/fixtures/assets/cover-warm.png'),str(fixture_icons/f'cover-warm@{scale}.png'),'16'],check=True)
render_env=dict(os.environ);render_env['HYPRV_BAR_FIXTURE_ASSET_ROOT']=str(asset_root.resolve());render_env['HYPRV_BAR_REFERENCE_SNAPSHOT']=str(snapshot);render_env['HYPRV_BAR_ACTUAL_DIR']=str(actual)
subprocess.run(['python3',str(crate/'scripts/check.py'),'run','--quiet','--example','reference'],cwd=workspace,check=True,env=render_env)
coverage=json.loads((actual/'coverage.json').read_text())
gate=report/'gate'
status=subprocess.run(['python3',str(crate/'scripts/gate_metrics.py'),str(actual),str(reference),str(gate)],check=True)
metrics=json.loads((gate/'gate.json').read_text())
(report/'coverage.json').write_text(json.dumps(coverage,indent=2))
(report/'report.md').write_text('# Bar QML parity\n\nPremultiplied RGBA acceptance is in [gate/gate.md](gate/gate.md). Current architecture rules are applied per reference distance-field flag. Strict exact counts and interim v1 counts remain separate. Calendar/island-expanded host crops compose the production components without masking. Other crops exclude the separately verified island slot. Shared engine QtQuick rectangle mode and image sampling are selected; no runtime Qt decoder is used.\n')
print(f"Gate {metrics['passed']}/{metrics['images']}; interim v1 {metrics['gate_v1_passed']}; strict exact {metrics['exact']}")
raise SystemExit(0 if metrics['passed']==metrics['images'] and not deferred else 1)
