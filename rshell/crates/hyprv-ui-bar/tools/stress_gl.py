#!/usr/bin/env python3
"""Run gl_stress in private Weston; optionally map test windows there."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,tempfile,time,sys
crate=Path(__file__).resolve().parents[1];root=crate.parents[1]
if '--tests' in sys.argv and shutil.disk_usage(root).free < 30 * 1024**3:raise SystemExit('Disk policy: need 30 GiB free before Cargo tests')
name=sys.argv[1];assert name.replace('-','').isalnum();out=crate/'artifacts'/name;out.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='gl-stress-',dir=crate/'.build') as tmp:
 runtime=Path(tmp);runtime.chmod(0o700)
 binary=root/'target/debug/examples/gl_stress';pinned=runtime/'gl-stress-bin'
 if '--tests' not in sys.argv:shutil.copy2(binary,pinned)
 (out/'provenance.json').write_text(json.dumps(dict(binary_sha256=hashlib.sha256(pinned.read_bytes()).hexdigest() if pinned.exists() else None,environment={k:v for k,v in os.environ.items()if k.startswith('HYPRV_STRESS')or k=='HYPRV_NATIVE_GL_DISABLE'},observed_source_sha256={str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest()for f in [root/'crates/hyprv-ui/src/egl.rs',root/'crates/hyprv-ui/src/native_gl.rs',crate/'examples/gl_stress.rs']}),indent=2))
 env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),WAYLAND_DISPLAY='gl-stress',CARGO_TARGET_DIR=str(root/'target'),CARGO_INCREMENTAL='0')
 with (out/'weston.log').open('w')as log:
  weston=subprocess.Popen(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket=gl-stress'],env=env,stdout=log,stderr=subprocess.STDOUT)
  try:
   for _ in range(100):
    if(runtime/'gl-stress').exists():break
    time.sleep(.05)
   else:raise RuntimeError('private compositor failed')
   with (out/'run.log').open('w')as log:code=subprocess.call(['cargo','test','-p','hyprv-ui','--test','native_gl','--','--ignored','--nocapture'] if '--tests' in sys.argv else [str(pinned),str(out)],env=env,stdout=log,stderr=subprocess.STDOUT)
  finally:weston.terminate();weston.wait(timeout=10)
raise SystemExit(code)
