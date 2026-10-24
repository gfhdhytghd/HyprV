#!/usr/bin/env python3
"""Read-only readiness checks. --run-tests explicitly runs local automated tests."""
import argparse, datetime, hashlib, importlib.util, json, math, os, pathlib, stat, subprocess, sys, time
sys.dont_write_bytecode=True
ROOT=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('release_build',ROOT/'packaging/build_release.py');build=importlib.util.module_from_spec(spec);spec.loader.exec_module(build)
COMPONENTS={'island':'hyprv-ui-island','bar':'hyprv-ui-bar','panels':'hyprv-ui-panels'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
SHELL_PACKAGES={'hyprv-privhelper','hyprv-core','hyprv-ui','hyprv-widgets','hyprv-ui-bar','hyprv-ui-island','hyprv-ui-panels','hyprv-shell',*('hyprv-svc-'+n for n in ('net','media','power','sys','desktop'))}
def shell_sources(root=ROOT):
    paths=[root/'Cargo.toml',root/'Cargo.lock']
    for name in sorted(SHELL_PACKAGES):
        crate=root/'crates'/name
        paths += [p for p in (crate/'Cargo.toml',crate/'build.rs',crate/'README.md')if p.is_file()]
        for part in ('src','assets','native','csrc','vendor'):
            paths += [p for p in (crate/part).rglob('*')if p.is_file()]
    return {str(p.relative_to(root)):sha(p)for p in sorted(set(paths))}
def shell_perf_gate(data):
    if data.get('mode')!='real-shell-release'or '--release'not in data.get('build_snapshot',{}).get('command',[]):raise ValueError('real release shell build provenance missing')
    for key in ('live_idle','activity'):
        window=data.get(key,{})
        if window.get('instrumented')is not False or window.get('seconds',0)<60:raise ValueError(key+': requires an uninstrumented >=60s real-shell interval')
        cpu=window.get('cpu_percent')
        if isinstance(cpu,bool)or not isinstance(cpu,(int,float))or not math.isfinite(cpu)or cpu<0:raise ValueError(key+': invalid CPU')
        if 'task_counters'not in window:raise ValueError(key+': scheduling evidence missing')
    idle=data['live_idle']
    controlled=data.get('controlled_idle',{})
    if controlled.get('seconds',0)>=60 and controlled.get('instrumented')is False and controlled.get('hook_file_events')==0:
        cpu=controlled.get('cpu_percent')
        if isinstance(cpu,bool)or not isinstance(cpu,(int,float))or not math.isfinite(cpu)or cpu<0:raise ValueError('controlled real-shell idle CPU invalid')
        if cpu>.1:raise ValueError('real-shell controlled quiet CPU exceeds 0.1%: '+str(cpu)+'; live agent activity is reported separately in perf-shell.md')
    if idle.get('pid_namespace')!='host'or idle.get('agent_inputs_frozen')is not False:raise ValueError('isolated/frozen real-shell runs are diagnostic only')
    if idle.get('hook_file_events')!=0:raise ValueError('real-shell full-host idle budget UNVERIFIED: agent-hook file activity; controlled quiet results are separate in perf-shell.md')
    if idle['cpu_percent']>.1:raise ValueError('real-shell idle CPU exceeds 0.1%: '+str(idle['cpu_percent']))
    return True
def accepted_shell_perf_gate(data, acceptance, binary_hash, now=None):
    """Explicit user acceptance; never reuse it for another artifact or old evidence."""
    if acceptance.get('accepted_by')!='user' or acceptance.get('date')!='2026-10-23' or not isinstance(acceptance.get('reason'),str) or not acceptance['reason'].strip():
        raise ValueError('performance acceptance requires user, date and reason')
    if acceptance.get('binary_sha256')!=binary_hash or data.get('binary_sha256')!=binary_hash:
        raise ValueError('performance acceptance/measured binary hash differs from candidate')
    if acceptance.get('hook_events_allowed')is not True:raise ValueError('performance acceptance must explicitly allow reported hook events')
    epoch=data.get('epoch');now=time.time()if now is None else now
    if isinstance(epoch,bool)or not isinstance(epoch,(int,float))or not math.isfinite(epoch)or not 0<=now-epoch<=86400:
        raise ValueError('performance measurement stale or invalid (requires age <=24 hours)')
    # Reuse every release/live/activity validation; only the two accepted CPU
    # bounds and the explicitly allowed live hook events differ from strict mode.
    copy=json.loads(json.dumps(data));copy.pop('controlled_idle',None)
    if 'live_idle'in copy:copy['live_idle'].update(cpu_percent=0,hook_file_events=0)
    shell_perf_gate(copy)
    values=[]
    for key,limit_key,ceiling in (('controlled_idle','controlled_quiet_max_pct',.5),('live_idle','live_hook_idle_max_pct',2.0)):
        limit=acceptance.get(limit_key);window=data.get(key,{})
        if isinstance(limit,bool)or not isinstance(limit,(int,float))or not math.isfinite(limit)or not 0<limit<=ceiling:
            raise ValueError('invalid accepted CPU bound: '+limit_key)
        cpu=window.get('cpu_percent');events=window.get('hook_file_events')
        if isinstance(cpu,bool)or not isinstance(cpu,(int,float))or not math.isfinite(cpu)or not 0<=cpu<=limit:
            raise ValueError(key+': CPU exceeds accepted bound or invalid: '+str(cpu))
        if window.get('instrumented')is not False or window.get('seconds',0)<60 or 'task_counters'not in window:
            raise ValueError(key+': requires uninstrumented >=60s scheduling evidence')
        if isinstance(events,bool)or not isinstance(events,int)or events<0:raise ValueError(key+': hook event count missing/invalid')
        if key=='controlled_idle'and events!=0:raise ValueError('controlled quiet interval contains hook events')
        values.append(f'{key} {cpu:.6f}% <= {limit:g}%, hook events {events}')
    return 'accepted: user 2026-10-23; '+'; '.join(values)+'; '+acceptance['reason']

def validate_candidate_sources(receipt, current):
    """Keep frozen build provenance intact; audit separately tested readiness edits."""
    old=receipt.get('sources',{})
    if old==current:return
    changed={k:current.get(k)for k in old.keys()|current.keys()if old.get(k)!=current.get(k)}
    allowed={'cutover/preflight.py','cutover/perf-acceptance.json','cutover/tests/test_perf_acceptance.py','cutover/parity-acceptance.json','cutover/tests/test_parity_acceptance.py'}
    if not set(changed)<=allowed:raise ValueError('candidate source changed since release build')
    amendment=json.loads((ROOT/'cutover/evidence/readiness-amendment.json').read_text())
    if amendment.get('build_receipt_sha256')!=sha(ROOT/'packaging/dist/build.json') or amendment.get('binary_sha256')!=receipt.get('binaries',{}).get('hyprv-shell')or amendment.get('changed_files')!=changed:
        raise ValueError('readiness-only source amendment missing or stale')
parity_spec=importlib.util.spec_from_file_location('cutover_parity',ROOT/'cutover/parity_gate.py')
parity=importlib.util.module_from_spec(parity_spec);parity_spec.loader.exec_module(parity)
parity_gate=parity.rows_gate
def accepted_parity_gate(acceptance, binary_hash, root=ROOT):
    """User visual sign-off, pinned to the ELF and immutable historical PASS receipts."""
    if acceptance.get('schema')!=1 or acceptance.get('accepted_by')!='user':raise ValueError('visual acceptance requires schema1 and user')
    date=acceptance.get('date','')
    if not isinstance(date,str)or datetime.date.fromisoformat(date).isoformat()!=date:raise ValueError('visual acceptance date requires YYYY-MM-DD')
    if not isinstance(acceptance.get('note'),str)or not acceptance['note'].strip():raise ValueError('visual acceptance note missing')
    if acceptance.get('binary_sha256')!=binary_hash:raise ValueError('visual acceptance binary differs from staged/installed shell')
    evidence={}
    for key in ('preflight','release'):
        ref=acceptance.get('last_passing_evidence',{}).get(key,{})
        rel=pathlib.Path(ref.get('path',''))
        if rel.is_absolute()or '..'in rel.parts or not rel.parts:raise ValueError('unsafe visual evidence path')
        path=root/rel;path.resolve().relative_to(root.resolve())
        if sha(path)!=ref.get('sha256'):raise ValueError('last passing visual evidence missing or changed: '+key)
        evidence[key]=json.loads(path.read_text())
    prior=evidence['preflight'];rows=[r for r in prior.get('checks',[])if r.get('check')=='parity gate']
    if prior.get('ready')is not True or len(rows)!=1 or rows[0].get('ok')is not True:raise ValueError('visual reference is not a fully passing preflight')
    release=evidence['release']
    if release.get('release_id')!=acceptance.get('evidence_release_id')or release.get('shell_sha256')!=binary_hash or release.get('all_sources_and_manifest_and_three_ELFs_verified')is not True:
        raise ValueError('last passing release evidence does not identify accepted shell')
    return 'accepted: user '+date+'; '+acceptance['note']+'; last fully passing evidence '+acceptance['evidence_release_id'][:16]+'; exact shell SHA-256 verified'

def validate_parity():
    path=ROOT/'cutover/parity-acceptance.json'
    if path.exists():return accepted_parity_gate(json.loads(path.read_text()),sha(ROOT/'packaging/dist/hyprv-shell'))
    return parity.inspect(ROOT)

def perf_gate(data):
    probe=data.get('probe',{});command=data.get('build_snapshot',{}).get('command',[])
    if data.get('mode')not in ('idle','release-latest'):raise ValueError('requires full live idle/release-latest report, not an isolation/before/fixture run')
    if data.get('excludes')or data.get('agent_inputs_frozen'):raise ValueError('excluded/frozen inputs are ineligible')
    if '--release'not in command:raise ValueError('release build provenance missing')
    cpu=probe.get('cpu_percent');seconds=probe.get('seconds',0)
    if isinstance(cpu,bool)or not isinstance(cpu,(int,float))or not math.isfinite(cpu)or cpu<0 or cpu>.1:raise ValueError('full service CPU exceeds 0.1% or invalid: '+str(cpu))
    if seconds<60:raise ValueError('idle interval shorter than 60 seconds')
    if 'task_counters'not in probe or 'state_events'not in data:raise ValueError('scheduling/event evidence missing')
    return True
def test_commands():
    return [
        ['cargo','test','--locked','--workspace','--all-targets'],
        ['cargo','test','--locked','--workspace','--doc'],
        ['cargo','test','--locked','--manifest-path','xtask/Cargo.toml'],
        [sys.executable,'-m','unittest','discover','-s','cutover/tests','-p','test*.py'],
        [sys.executable,'-m','unittest','discover','-s','crates/hyprv-svc-probe/scripts','-p','test*.py']]
def run_tests():
    directory=ROOT/'cutover/evidence';directory.mkdir(parents=True,exist_ok=True)
    before=build.sources();results=[]
    env=dict(os.environ,HYPRV_ASSET_ROOT=str(ROOT/'packaging/dist/release/runtime'))
    for i,cmd in enumerate(test_commands()):
        print('TEST '+repr(cmd),flush=True)
        with (directory/f'test-{i}.log').open('w')as log:code=subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT).returncode
        results.append({'command':cmd,'returncode':code,'log':f'test-{i}.log'})
    result={'schema':1,'epoch':time.time(),'sources':before,'results':results,'status':'pass'if all(r['returncode']==0 for r in results)and before==build.sources()else'fail'}
    (directory/'tests.json').write_text(json.dumps(result,indent=2)+'\n')
    print('# Test receipts exclude explicitly ignored opt-in live/device tests; their separate evidence remains pending.',flush=True)
def validate_helper_install_set():
    """Candidate admission proves bundled install inputs, never host installation."""
    spec=importlib.util.spec_from_file_location('candidate_install',ROOT/'packaging/install.py')
    installer=importlib.util.module_from_spec(spec);spec.loader.exec_module(installer)
    release=installer.candidate();rid,rows=installer.verify_release(release)
    receipt=json.loads((ROOT/'packaging/dist/build.json').read_text())
    if rid!=receipt.get('release_id'):raise ValueError('helper install manifest differs from build receipt')
    for name in build.BINS:
        if rows.get('bin/'+name)!=receipt.get('binaries',{}).get(name):raise ValueError('helper install ELF differs from staged build: '+name)
    for source,destination,mode in installer.FILES_ROOT:
        if source not in rows:raise ValueError('helper install input missing: '+source)
        if sha(release/source)!=rows[source]:raise ValueError('helper install input changed: '+source)
    required={'files/org.hyprv.Shell.PrivHelper.conf','files/org.hyprv.shell.fan-control.policy'}
    if not required <= {s for s,_,_ in installer.FILES_ROOT}:raise ValueError('helper policy missing from install set')
    return f'{len(installer.FILES_ROOT)} manifest-verified helper/unit/D-Bus/polkit install inputs; installed state NOT certified (candidate mode)'

def validate_helper():
    spec2=importlib.util.spec_from_file_location('install',ROOT/'packaging/install.py');installer=importlib.util.module_from_spec(spec2);spec2.loader.exec_module(installer)
    record=json.loads(installer.INSTALLED.read_text())
    name=record.get('dir','')
    if not name or pathlib.PurePath(name).name!=name or name in ('.','..'):raise ValueError('unsafe installed release directory')
    release=installer.RELEASES/name
    rid,rows=installer.verify_release(release,record['id'],root_owned=True)
    installer.paths.validate_installed_record(record,json.loads((release/'release.json').read_text()))
    candidate,_=installer.verify_release(installer.candidate())
    if candidate!=rid:raise ValueError(f'installed release {rid[:16]} differs from candidate {candidate[:16]} in packaging/dist/release')
    for source,destination,mode in installer.FILES_ROOT:
        path=pathlib.Path(destination);info=path.lstat()
        if not stat.S_ISREG(info.st_mode)or info.st_uid!=0 or info.st_mode&0o022:raise ValueError('unsafe/missing helper artifact: '+destination)
        if sha(path)!=rows[source]:raise ValueError('installed helper artifact differs from release: '+destination)
        if stat.S_IMODE(info.st_mode)!=mode:raise ValueError('installed helper artifact mode differs: '+destination)
    override=pathlib.Path('/etc/systemd/system/hyprv-privhelper.service')
    if (override.exists()or override.is_symlink())and not(override.is_symlink()and os.readlink(override)=='/dev/null'):
        raise ValueError('administrator helper unit override needs review: '+str(override))
    for directory in (pathlib.Path('/etc/systemd/system/hyprv-privhelper.service.d'),pathlib.Path('/run/systemd/system/hyprv-privhelper.service.d')):
        if any(directory.glob('*.conf')):raise ValueError('unreviewed helper drop-ins: '+str(directory))
    return True
def validate_assets(installed=False):
    spec=importlib.util.spec_from_file_location('asset_installer',ROOT/'packaging/install.py');installer=importlib.util.module_from_spec(spec);spec.loader.exec_module(installer)
    release=installer.candidate()
    installer.verify_release(release)
    files=installer.user_files(release)
    catalog=ROOT/'crates/hyprv-shell/src/runtime-assets.json'
    if sha(release/'runtime/runtime-assets.json')!=sha(catalog):raise ValueError('runtime asset catalog differs from candidate source')
    expected='Environment=HYPRV_ASSET_ROOT=%h/.local/share/hyprv-shell'
    unit=pathlib.Path.home()/'.config/systemd/user/hyprv-shell.service'if installed else release/'files/hyprv-shell.service'
    if expected not in unit.read_text().splitlines():raise ValueError('unit does not set the stable runtime asset root')
    if installed:
        for source,dest,_ in files:
            path=pathlib.Path.home()/dest
            if source.startswith('runtime/')and(path.is_symlink()or sha(path)!=sha(release/source)):raise ValueError('installed runtime asset differs: '+dest)
    root=pathlib.Path.home()/'.local/share/hyprv-shell'if installed else release/'runtime'
    binary=pathlib.Path.home()/'.local/bin/hyprv-shell'if installed else ROOT/'packaging/dist/hyprv-shell'
    result=subprocess.run([str(binary),'--check-assets'],env=dict(os.environ,HYPRV_ASSET_ROOT=str(root.resolve())),capture_output=True,text=True,timeout=10)
    if result.returncode:raise ValueError('runtime asset startup check failed: '+result.stderr.strip())
    payloads=sum(source.startswith('runtime/')and source!='runtime/runtime-assets.json'for source,_,_ in files)
    return f'{payloads} runtime payloads and catalog hash-verified; unit asset root configured'+(' (installed)'if installed else ' (staged)')
def validate_release_paths():
    release=ROOT/'packaging/dist/release'
    metadata=json.loads((release/'release.json').read_text())
    workspace=build.paths.validate_metadata(metadata)
    if workspace!=ROOT.resolve():raise ValueError('release workspace differs from deployment checkout: '+str(workspace))
    receipt=json.loads((ROOT/'packaging/dist/build.json').read_text())
    command=receipt.get('command',[])
    for index,argument in enumerate(command):
        if isinstance(argument,str)and argument.startswith('/'):
            build.paths.stable_path(argument,allow_build_target=index>0 and command[index-1]=='--target-dir')
    if receipt.get('workspace')is not None and build.paths.stable_path(receipt['workspace'])!=workspace:raise ValueError('build/release workspace differs')
    return 'stable deployment workspace and preflight exist: '+str(workspace)
def inspect(installed=False,perf_path=None,candidate=False):
    if installed and candidate:raise ValueError("candidate and installed modes are mutually exclusive")
    metadata_path=ROOT/'packaging/dist/release/release.json'
    if metadata_path.is_file()and json.loads(metadata_path.read_text()).get('deployment_profile')=='laptop':
        spec=importlib.util.spec_from_file_location('laptop_preflight',ROOT/'cutover/laptop/preflight.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        return module.inspect(ROOT,__import__('types').SimpleNamespace(**globals()),installed,candidate)
    checks=[]
    def check(name,fn):
        try:detail=fn();checks.append({'check':name,'ok':True,'detail':str(detail or 'pass')})
        except (OSError,ValueError,KeyError,TypeError,RuntimeError,StopIteration,subprocess.SubprocessError)as error:checks.append({'check':name,'ok':False,'detail':str(error)})
    def binaries():
        receipt=json.loads((ROOT/'packaging/dist/build.json').read_text())
        if receipt.get('profile')!='release':raise ValueError('release build receipt missing')
        validate_candidate_sources(receipt,build.sources())
        for name in build.BINS:
            path=ROOT/'packaging/dist'/name
            if not os.access(path,os.X_OK)or path.read_bytes()[:4]!=b'\x7fELF'or sha(path)!=receipt['binaries'].get(name):raise ValueError('release artifact missing/digest invalid: '+name)
        if installed:
            binary=pathlib.Path.home()/'.local/bin/hyprv-shell';unit=pathlib.Path.home()/'.config/systemd/user/hyprv-shell.service'
            if sha(binary)!=receipt['binaries']['hyprv-shell']or sha(unit)!=sha(ROOT/'packaging/hyprv-shell.service'):raise ValueError('installed user binary/unit differs from candidate')
        return 'release candidate and digests verified'+(' (installed)'if installed else '')
    def tests():
        receipt=json.loads((ROOT/'cutover/evidence/tests.json').read_text())
        if receipt.get('status')!='pass'or receipt.get('sources')!=build.sources():raise ValueError('automated test receipt failed or stale; run preflight --run-tests')
        rows=receipt.get('results',[])
        if [r.get('command')for r in rows]!=test_commands()or any(r.get('returncode')!=0 for r in rows):raise ValueError('full configured test suite did not pass')
        return 'workspace all-targets + doctests + xtask + cutover/probe tests passed; opt-in live tests excluded'
    def parity_check():
        return validate_parity()
    def perf():
        real=ROOT/'docs/verify/raw/perf-shell-latest.json'
        path=perf_path or (real if real.exists()else ROOT/'docs/verify/raw/perf-release-latest.json');data=json.loads(path.read_text())
        if data.get('mode')=='real-shell-release':
            executable=pathlib.Path(data['binary_path'])
            if executable.name!='hyprv-shell'or executable.read_bytes()[:4]!=b'\x7fELF'or sha(executable)!=data.get('binary_sha256'):raise ValueError('measured real-shell artifact missing or replaced')
            if data.get('build_snapshot',{}).get('files')!=shell_sources():raise ValueError('real-shell performance source evidence stale; rerun perf-shell measurement')
            candidate=ROOT/'packaging/dist/hyprv-shell'
            digest=sha(candidate)
            if data.get('binary_sha256')!=digest:raise ValueError('measured real-shell binary differs from staged candidate')
            if installed and sha(pathlib.Path.home()/'.local/bin/hyprv-shell')!=digest:raise ValueError('installed real-shell binary differs from measured candidate')
            acceptance_path=ROOT/'cutover/perf-acceptance.json'
            detail=accepted_shell_perf_gate(data,json.loads(acceptance_path.read_text()),digest)if acceptance_path.exists()else None
            if detail is None:shell_perf_gate(data)
            if (ROOT/'docs/verify/perf-shell.md').stat().st_mtime<path.stat().st_mtime:raise ValueError('perf-shell.md not updated after raw data')
            if detail:return detail
            return 'real release shell <=0.1% idle; >=60s idle and activity; UI and all service source fingerprints current'
        perf_gate(data)
        files=data['build_snapshot']['files']
        required={'hyprv-core/Cargo.toml',*('hyprv-svc-'+name+'/Cargo.toml'for name in ('net','media','power','sys','desktop'))}
        if not required.issubset(files):raise ValueError('incomplete service source fingerprints')
        executable=pathlib.Path(data['binary_path'])
        if sha(executable)!=data.get('binary_sha256'):raise ValueError('measured probe binary missing or replaced')
        for name,digest in files.items():
            if name.startswith(('hyprv-core/','hyprv-svc-','hyprv-privhelper/'))and sha(ROOT/'crates'/name)!=digest:raise ValueError('service performance evidence stale: '+name)
        stream=path.with_name(path.stem+'-probe.jsonl')
        ready=next(json.loads(line)for line in stream.open()if json.loads(line).get('kind')=='ready')
        if ready.get('agent_inputs_frozen')or ready.get('actions_dispatched')!=0 or not all(ready.get('compiled',{}).get(k)for k in ('net','media','power','sys','desktop')):raise ValueError('not a full live, observation-only service run')
        if (ROOT/'docs/verify/perf.md').stat().st_mtime<path.stat().st_mtime:raise ValueError('perf.md not updated after raw data')
        return 'full release service budget <=0.1%, >=60s; UI rendering is covered separately by parity'
    check('release paths',validate_release_paths);check('release binaries',binaries);check('runtime assets',lambda:validate_assets(installed));check('automated tests',tests);check('parity gate',parity_check);check('performance gate',perf);check('privhelper install set'if candidate else'privhelper installed',validate_helper_install_set if candidate else validate_helper)
    return checks
def main():
    p=argparse.ArgumentParser();p.add_argument('--run-tests',action='store_true');scope=p.add_mutually_exclusive_group();scope.add_argument('--installed',action='store_true');scope.add_argument('--candidate',action='store_true',help='Validate readiness to install the candidate; do not certify installed helper artifacts');p.add_argument('--perf-json',type=pathlib.Path);p.add_argument('--json',action='store_true');a=p.parse_args()
    if a.run_tests:run_tests()
    checks=inspect(a.installed,a.perf_json,a.candidate)
    if a.json:print(json.dumps({'ready':all(c['ok']for c in checks),'scope':'candidate'if a.candidate else'installed'if a.installed else'cutover','checks':checks},indent=2))
    else:
        for c in checks:
            status=('PASS ('+c['detail']+')')if c['ok']and c['detail'].startswith('accepted:')else ('PASS'if c['ok']else 'FAIL'if c['check']=='performance gate'else 'BLOCKED')
            print(c['check']+': '+status+(''if status.startswith('PASS (')else ': '+c['detail']))
        print(('READY (candidate; installation not certified)'if a.candidate else'READY')if all(c['ok']for c in checks)else'NOT READY — no cutover is authorized by this report')
    return 0 if all(c['ok']for c in checks)else 1
if __name__=='__main__':sys.exit(main())
