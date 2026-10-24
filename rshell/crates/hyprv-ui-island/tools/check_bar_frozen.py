#!/usr/bin/env python3
"""Shared-engine control against frozen bar inputs; do not alter QA's live adapter."""
from pathlib import Path
import os,subprocess,json,hashlib,importlib.util,tempfile,shutil
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];bar=root/'crates/hyprv-ui-bar';out=crate/'artifacts/bar-frozen';source=out/'adapter';source.mkdir(parents=True,exist_ok=True)
s=(bar/'examples/reference.rs').read_text();s=s.replace('#[path = "../tests/common/mod.rs"]','#[path = "common.rs"]')
# Restore only the old, frozen capture protocol: unresolved icons had no image,
# selecting the letter. Keep the production full viewport for matching GL precision.
start=s.index(' else {\n                // Unresolvable theme name:');brace=s.index('{',start);depth=1;end=brace+1
while depth:
 if s[end]=='{':depth+=1
 elif s[end]=='}':depth-=1
 end+=1
s=s[:start]+s[end:];(source/'main.rs').write_text(s)
s=(bar/'tests/common/mod.rs').read_text().replace('std::path::Path::new(env!("CARGO_MANIFEST_DIR"))',f'std::path::Path::new("{bar}")');(source/'common.rs').write_text(s)
manifest='[package]\nname="hyprv-island-bar-control"\nversion="0.1.0"\nedition="2021"\n[workspace]\n[[bin]]\nname="island-bar-control"\npath="main.rs"\n[dependencies]\nserde_json="1"\n'
for name in ['hyprv-ui','hyprv-ui-bar','hyprv-widgets','hyprv-svc-sys','hyprv-svc-media']:manifest+=f'{name}={{path="{root}/crates/{name}"}}\n'
(source/'Cargo.toml').write_text(manifest)
env={**os.environ,'CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(root/'target')}
with(out/'build.log').open('w')as log:subprocess.run(['cargo','build','--manifest-path',str(source/'Cargo.toml')],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
spec=importlib.util.spec_from_file_location('v',root/'parity/verify/verify.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v);refs=bar/'artifacts/gate-inputs/reference';binary=root/'target/debug/island-bar-control'
receipt={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'adapter_sha256':hashlib.sha256((source/'main.rs').read_bytes()).hexdigest(),'reference_manifest':hashlib.sha256((refs/'manifest.json').read_bytes()).hexdigest(),'protocol':'frozen letter fallback; production full viewport; live adapter unchanged'}
with tempfile.TemporaryDirectory(prefix='island-bar-control-')as tmp,v.PrivateWeston()as socket:
 assets=Path(tmp)/'assets'
 for name in ['theme-icons','fixture-icons']:shutil.copytree(bar/'artifacts'/name,assets/name)
 with(out/'render.log').open('w')as log:subprocess.run([str(binary)],env={**env,'WAYLAND_DISPLAY':socket,'HYPRV_PARITY_BACKEND':'gl','HYPRV_BAR_REFERENCE_SNAPSHOT':str(refs),'HYPRV_BAR_ACTUAL_DIR':str(out/'actual'),'HYPRV_BAR_FIXTURE_ASSET_ROOT':str(assets)},stdout=log,stderr=subprocess.STDOUT,check=True)
 with(out/'metrics.log').open('w')as log:subprocess.run(['python3',str(bar/'scripts/gate_metrics.py'),str(out/'actual'),str(refs),str(out/'gate')],stdout=log,stderr=subprocess.STDOUT,check=True)
(out/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
