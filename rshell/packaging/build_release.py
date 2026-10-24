#!/usr/bin/env python3
"""Build/stage artifacts only; never install, start helpers or open Wayland."""
import argparse, hashlib, json, os, pathlib, shlex, shutil, subprocess, time
import importlib.util
_path_spec=importlib.util.spec_from_file_location('hyprv_release_paths',pathlib.Path(__file__).with_name('release_paths.py'))
paths=importlib.util.module_from_spec(_path_spec);_path_spec.loader.exec_module(paths)
ROOT = pathlib.Path(__file__).resolve().parents[1]
BINS = ('hyprv-shell', 'hyprv-privhelper', 'hyprv-cooling-failsafe')
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def sources():
    paths = [ROOT/'Cargo.toml', ROOT/'Cargo.lock', ROOT/'packaging/release.toml']
    for crate in (ROOT/'crates').iterdir():
        paths += [p for p in [crate/'Cargo.toml', crate/'build.rs'] if p.is_file()]
        for part in ('src','tests','examples','benches','vendor','csrc','native','assets','packaging'):
            paths += [p for p in (crate/part).rglob('*') if p.is_file()]
    paths += [p for p in (ROOT/'packaging').iterdir() if p.is_file() and p.suffix in ('.py','.sh','.service','.toml','.conf','.policy')]
    paths += [p for p in (ROOT/'packaging/assets').rglob('*') if p.is_file()]
    paths += [p for p in (ROOT/'cutover').rglob('*') if p.is_file() and p.suffix in ('.py','.sh','.json','.patch','.lua') and not any(part in ('.check','evidence','__pycache__')for part in p.relative_to(ROOT/'cutover').parts)]
    # These are consumed by the ordinary suite or the readiness verifier; a
    # copied source receipt must not certify tests against a different oracle.
    for area in ('parity/verify', 'parity/fixtures', 'parity/reference',
                 'parity/fullscreen/fixtures', 'parity/fullscreen/reference', 'parity/fullscreen/tools',
                 'parity/laptop/fakes', 'parity/laptop/e2e', 'parity/laptop/reference',
                 'fixtures/flickable-implementation', 'fixtures/panels-flickable',
                 'experiments/text'):
        paths += [p for p in (ROOT/area).rglob('*') if p.is_file() and p.suffix in ('.py','.rs','.json','.csv','.png','.bin','.qml','.js')
                  and not any(part in ('target','__pycache__') for part in p.relative_to(ROOT/area).parts)
                  and not (area == 'parity/verify' and 'out' in p.relative_to(ROOT/area).parts)]
    for crate in (ROOT/'crates').iterdir():
        for part in ('scripts','tools','qml-ref'):
            paths += [p for p in (crate/part).rglob('*') if p.is_file() and p.suffix in ('.py','.sh','.rs','.cpp','.h','.json','.csv','.png','.bin','.qml')
                      and not any(x in ('target','__pycache__','.build','artifacts') for x in p.relative_to(crate/part).parts)]
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(set(paths)) if p.is_file()}
# Release layout: everything the root and user installers need, hash-listed in
# MANIFEST.sha256 (sha256sum format). Its digest is the release id.
RELEASE_FILES={
    **{'bin/'+name:'packaging/dist/'+name for name in BINS},
    'files/hyprv-privhelper.service':'packaging/hyprv-privhelper.service',
    'files/org.hyprv.Shell.PrivHelper.service':'packaging/org.hyprv.Shell.PrivHelper.service',
    'files/org.hyprv.Shell.PrivHelper.conf':'packaging/org.hyprv.Shell.PrivHelper.conf',
    'files/org.hyprv.shell.fan-control.policy':'packaging/org.hyprv.shell.fan-control.policy',
    'files/hyprv-shell.service':'packaging/hyprv-shell.service',
    'packaging/install.py':'packaging/install.py',
    'packaging/release_paths.py':'packaging/release_paths.py',
    'cutover/control.py':'cutover/control.py',
    'cutover/legacy-units.json':'cutover/legacy-units.json',
    'cutover/hypr.patch':'cutover/hypr.patch',
    'cutover/hypr-inputs.json':'cutover/hypr-inputs.json'}
def binaries_for(profile='desktop',cpu_power=False):
    if profile not in ('desktop','laptop'):raise ValueError('unknown deployment profile')
    if cpu_power and profile!='laptop':raise ValueError('CPU-only option requires laptop profile')
    return BINS if profile=='desktop'else ('hyprv-shell',)+(('hyprv-cpu-power',)if cpu_power else ())

def release_files(profile='desktop',cpu_power=False):
    binaries_for(profile,cpu_power)
    if profile=='desktop':return dict(RELEASE_FILES)
    files={
        'bin/hyprv-shell':'packaging/dist/hyprv-shell',
        'files/hyprv-shell.service':'packaging/hyprv-shell-laptop.service',
        'packaging/install.py':'packaging/install.py',
        'packaging/release_paths.py':'packaging/release_paths.py',
        'cutover/control.py':'cutover/laptop/control.py',
        'cutover/hypr.patch':'cutover/laptop/hypr.patch',
        'cutover/hypr-inputs.json':'cutover/laptop/hypr-inputs.json'}
    if cpu_power:
        files['bin/hyprv-cpu-power']='packaging/dist/hyprv-cpu-power'
        for name in ('hyprv-cpu-power-only.service','org.hyprv.Shell.CpuPower.service',
                     'org.hyprv.Shell.CpuPower.conf','org.hyprv.shell.cpu-power.policy'):
            files['files/'+name]='packaging/'+name
    return files

def runtime_files():
    catalog=ROOT/'crates/hyprv-shell/src/runtime-assets.json'
    data=json.loads(catalog.read_text())
    if data.get('schema')!=1 or not data.get('files'):raise RuntimeError('invalid runtime asset catalog')
    files={'runtime/runtime-assets.json':str(catalog.relative_to(ROOT))}
    for name,digest in data['files'].items():
        path=pathlib.PurePosixPath(name)
        if path.is_absolute()or '..'in path.parts or path.parts[0]not in ('assets','icons'):raise RuntimeError('unsafe runtime asset name: '+name)
        source=ROOT/'packaging/assets'/name
        if source.is_symlink()or sha(source)!=digest:raise RuntimeError('runtime asset catalog mismatch: '+name)
        files['runtime/'+name]=str(source.relative_to(ROOT))
    return files
def assemble(dist,sources_before,profile='desktop',cpu_power=False):
    """Write dist/release (regular files only, dereferenced) and return its id."""
    temp=dist/'release.new'
    if temp.exists():shutil.rmtree(temp)  # our own previous partial output
    for rel,src in {**release_files(profile,cpu_power),**runtime_files()}.items():
        out=temp/rel;out.parent.mkdir(parents=True,exist_ok=True)
        out.write_bytes((ROOT/src).read_bytes());out.chmod(0o755 if rel.startswith('bin/')or rel.endswith('.py')else 0o644)
    meta={'schema':1,'workspace':str(paths.stable_path(os.environ.get('HYPRV_DEPLOY_WORKSPACE',str(ROOT)))),'owner':os.environ.get('USER')or __import__('pwd').getpwuid(os.getuid()).pw_name,'sources_sha256':hashlib.sha256(json.dumps(sources_before,sort_keys=True).encode()).hexdigest()}
    if profile=='laptop':meta.update(schema=2,deployment_profile=profile,cpu_power=cpu_power)
    paths.validate_metadata(meta)
    (temp/'release.json').write_text(json.dumps(meta,indent=2)+'\n');(temp/'release.json').chmod(0o644)
    rows=sorted(str(p.relative_to(temp))for p in temp.rglob('*')if p.is_file())
    (temp/'MANIFEST.sha256').write_text(''.join(f'{sha(temp/r)}  {r}\n'for r in rows));(temp/'MANIFEST.sha256').chmod(0o644)
    final=dist/'release'
    if final.exists():shutil.rmtree(final)  # generated output of the previous build only
    temp.rename(final)
    return sha(final/'MANIFEST.sha256')
def repackage(dry_run=False):
    """Reuse exact ELFs, never relabel changed runtime inputs as a prior build."""
    dist=ROOT/'packaging/dist';old_path=dist/'build.json'
    previous=json.loads(old_path.read_text());before=sources()
    if previous.get('profile')!='release':raise RuntimeError('release build receipt required')
    if previous.get('deployment_profile','desktop')!='desktop':raise RuntimeError('laptop repackage requires a full profile build')
    def compiled(rows):return {k:v for k,v in rows.items()if not k.startswith(('packaging/','cutover/')) or k=='packaging/release.toml' or k.startswith('packaging/assets/')}
    if compiled(previous['sources'])!=compiled(before):raise RuntimeError('non-packaging inputs changed; full release build and evidence regeneration required')
    for name in BINS:
        if sha(dist/name)!=previous['binaries'][name]:raise RuntimeError('release ELF changed: '+name)
    print('Repackage exact release ELFs; validate stable deployment references',flush=True)
    if dry_run:return
    parent_digest=sha(old_path)
    audit=ROOT/'cutover/evidence';audit.mkdir(parents=True,exist_ok=True)
    (audit/('build-parent-'+parent_digest+'.json')).write_bytes(old_path.read_bytes())
    rid=assemble(dist,before)
    if sources()!=before:raise RuntimeError('sources changed while repackaging')
    receipt={'schema':2,'epoch':time.time(),'profile':'release','command':[str(paths.stable_path(os.environ.get('HYPRV_DEPLOY_WORKSPACE',str(ROOT)))/'packaging/build-release.sh'),'--repackage'],
             'workspace':str(paths.stable_path(os.environ.get('HYPRV_DEPLOY_WORKSPACE',str(ROOT)))),
             'sources':before,'binaries':previous['binaries'],'release_id':rid,
             'provenance':{'mode':'packaging-only; exact prior ELFs retained','parent_build_receipt_sha256':parent_digest}}
    old_path.write_text(json.dumps(receipt,indent=2)+'\n')
    print('Release id (compare staged MANIFEST.sha256 before root execution):\n'+rid,flush=True)

def main():
    p=argparse.ArgumentParser();p.add_argument('--dry-run',action='store_true');p.add_argument('--profile',choices=['desktop','laptop'],default='desktop');p.add_argument('--with-cpu-power',action='store_true');p.add_argument('--repackage',action='store_true',help='Rebuild packaging only, retaining hash-verified release ELFs when all non-packaging inputs match');a=p.parse_args()
    if a.with_cpu_power and a.profile!='laptop':p.error('--with-cpu-power requires --profile laptop')
    selected=binaries_for(a.profile,a.with_cpu_power)
    if a.repackage:
        if a.profile!='desktop'or a.with_cpu_power:raise SystemExit('profile change requires a full build')
        return repackage(a.dry_run)
    target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))
    if not target.is_absolute():target=ROOT/target
    cmd=['cargo','build','--manifest-path',str(ROOT/'Cargo.toml'),'--locked','--release','--target-dir',str(target),'--config',str(ROOT/'packaging/release.toml'),'-p','hyprv-shell']
    if len(selected)>1:cmd+=['-p','hyprv-privhelper']
    for name in selected:cmd+=['--bin',name]
    print(shlex.join(cmd), flush=True)
    print('Stage '+', '.join(selected)+', build.json and the hash-manifested release/ (MANIFEST.sha256) under '+str(ROOT/'packaging/dist'),flush=True)
    if a.dry_run:return
    deployment=paths.stable_path(os.environ.get('HYPRV_DEPLOY_WORKSPACE',str(ROOT)))
    paths.validate_metadata({'schema':1,'workspace':str(deployment),'owner':os.environ.get('USER')or __import__('pwd').getpwuid(os.getuid()).pw_name,'sources_sha256':'0'*64})
    before=sources();subprocess.run(cmd,cwd=ROOT,check=True)
    if sources()!=before:raise SystemExit('sources changed during build; refusing an ambiguous receipt, rerun')
    target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))
    if not target.is_absolute():target=ROOT/target
    dist=ROOT/'packaging/dist';dist.mkdir(parents=True,exist_ok=True)
    for name in selected:shutil.copy2(target/'release'/name,dist/name)
    if sources()!=before:raise SystemExit('sources changed while staging; no new receipt written')
    rid=assemble(dist,before,a.profile,a.with_cpu_power)
    # Command paths are source-relative; a frozen build location is not a deployment reference.
    recorded_cmd=[str(pathlib.Path(arg).relative_to(ROOT))if isinstance(arg,str)and arg.startswith(str(ROOT)+'/')and not arg.startswith(str(target))else arg for arg in cmd]
    paths.stable_path(str(target),allow_build_target=True)
    receipt={'schema':2,'epoch':time.time(),'profile':'release','command':recorded_cmd,'workspace':str(deployment),'build_command_sha256':hashlib.sha256(json.dumps(cmd).encode()).hexdigest(),'sources':before,'binaries':{name:sha(dist/name)for name in selected},'release_id':rid,'deployment_profile':a.profile,'cpu_power':a.with_cpu_power}
    (dist/'build.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print('Release id (compare with sha256sum of the staged MANIFEST.sha256 before any root step):\n'+rid,flush=True)
if __name__=='__main__':main()
