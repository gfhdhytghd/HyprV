#!/usr/bin/env python3
"""Check identical sources/dependencies while unrelated workspace members are edited."""
from pathlib import Path
import os,re,shutil,subprocess,sys
crate=Path(__file__).resolve().parents[1]
build=crate/'.build/check';build.mkdir(parents=True,exist_ok=True)
manifest=(crate/'Cargo.toml').read_text().replace('version.workspace = true','version = "0.1.0"').replace('edition.workspace = true','edition = "2021"')
manifest=re.sub(r'path = "([^"]+)"',lambda m:'path = "'+str((crate/m[1]).resolve())+'"',manifest)
manifest+='\n[workspace]\n[lib]\npath="../../src/lib.rs"\n'
for kind,directory in [('test','tests'),('example','examples')]:
 for source in sorted((crate/directory).glob('*.rs')):manifest+=f'\n[[{kind}]]\nname="{source.stem}"\npath="{source}"\n'
(build/'Cargo.toml').write_text(manifest)
if shutil.disk_usage(crate).free < 30 * 1024**3:raise SystemExit('Disk policy: need 30 GiB free before Cargo build/check')
env=dict(os.environ);env['CARGO_TARGET_DIR']=str(crate.parents[1]/'target');env['CARGO_INCREMENTAL']='0'
mode=sys.argv[1]if len(sys.argv)>1 else'test'
raise SystemExit(subprocess.call(['cargo',mode,'--manifest-path',str(build/'Cargo.toml'),*sys.argv[2:]],env=env))
