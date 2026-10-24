"""Laptop admission. Desktop visual/performance waivers never certify this profile."""
import hashlib, importlib.util, json, math, pathlib, stat, subprocess, time

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def input_digest(inputs):return hashlib.sha256(json.dumps(inputs,sort_keys=True).encode()).hexdigest()
def load_report(root,ref):
    name=pathlib.PurePosixPath(ref['path'])
    if name.is_absolute()or '..'in name.parts:raise ValueError('unsafe evidence path')
    path=root/name
    if not path.resolve().is_relative_to(root.resolve())or path.is_symlink():raise ValueError('escaped evidence path')
    if digest(path)!=ref['sha256']:raise ValueError('evidence digest differs')
    build=root/'packaging/dist/build.json'
    if build.is_file()and path.stat().st_mtime<json.loads(build.read_text()).get('epoch',0):raise ValueError('evidence predates candidate build')
    return json.loads(path.read_text())
def bound_report(root,name,binary,inputs):
    receipt=json.loads((root/'cutover/evidence'/name).read_text())
    if receipt.get('schema')!=1 or receipt.get('profile')!='laptop'or receipt.get('binary_sha256')!=binary or receipt.get('sources_sha256')!=input_digest(inputs):raise ValueError('laptop evidence binary/profile/source mismatch')
    epoch=receipt.get('epoch')
    if isinstance(epoch,bool)or not isinstance(epoch,(float,int))or not 0<=time.time()-epoch<=86400:raise ValueError('laptop evidence stale')
    return receipt

def functional(root,binary,inputs):
    receipt=bound_report(root,'laptop-e2e.json',binary,inputs)
    spec=importlib.util.spec_from_file_location('laptop_cases',root/'parity/laptop/e2e/run-laptop-e2e.py');qa=importlib.util.module_from_spec(spec);spec.loader.exec_module(qa)
    expected={row[0]for row in qa.CASES}
    for name in ('laptop','desktop'):
        report=load_report(root,receipt['reports'][name]);rows=report.get('cases',[])
        if report.get('suite_complete')is not True or not rows or any(r.get('status')!='PASS'for r in rows):raise ValueError(name+' functional suite incomplete/failed')
        if digest(pathlib.Path(report['binary']))!=binary:raise ValueError(name+' functional binary differs')
        ids=[r['id']for r in rows]
        if len(ids)!=len(set(ids)):raise ValueError('duplicate functional case')
        if name=='desktop'and len(rows)!=204:raise ValueError('desktop requires 204/204')
        if name=='laptop'and not expected<=set(ids):raise ValueError('laptop functional cases missing')
    return 'laptop cases and desktop 204/204 PASS, binary/source-bound'

def visual(root,binary,inputs):
    from PIL import Image
    receipt=bound_report(root,'laptop-parity.json',binary,inputs)
    # This canonical oracle is itself included in the all-input fingerprint.
    if receipt.get('backend')!='production-gl':raise ValueError('laptop requires production GL renders')
    oracle=json.loads((root/'parity/laptop/reference/manifest.json').read_text())
    slots=oracle.get('slots',[]);expected={r['id']:r for r in slots}
    if not slots or len(expected)!=len(slots)or {r['component']for r in slots}!={'bar','island','panels','fullscreen','corners'}:raise ValueError('laptop oracle missing/ambiguous component slots')
    rows=receipt.get('rows',[])
    if len(rows)!=len(expected)or {r['id']for r in rows}!=set(expected):raise ValueError('laptop parity missing/duplicate slots')
    for row in rows:
        slot=expected[row['id']]
        if row.get('reference')!=slot['reference']:raise ValueError('wrong parity reference')
        if row['actual']['path']==row['reference']['path']:raise ValueError('actual must be a separate render artifact')
        images=[]
        for role in ('reference','actual'):
            ref=row[role];name=pathlib.PurePosixPath(ref['path'])
            if name.is_absolute()or '..'in name.parts:raise ValueError('unsafe parity image path')
            path=root/name
            if not path.resolve().is_relative_to(root.resolve())or digest(path)!=ref['sha256']:raise ValueError('parity image missing/changed')
            with Image.open(path)as im:images.append(im.convert('RGBA').copy())
        if images[0].size!=images[1].size or images[0].tobytes()!=images[1].tobytes():raise ValueError('laptop exact image mismatch: '+row['id'])
    return f'{len(rows)} laptop production image slots exact; no missing images/masks/desktop waiver'

def performance(root,binary,inputs):
    receipt=bound_report(root,'laptop-perf.json',binary,inputs);window=load_report(root,receipt['report'])
    seconds=window.get('seconds')
    if isinstance(seconds,bool)or not isinstance(seconds,(int,float))or not math.isfinite(seconds)or seconds<60:raise ValueError('invalid laptop measurement duration')
    if window.get('profile')!='laptop'or window.get('binary_sha256')!=binary or window.get('nested')is not True or window.get('instrumented')is not False or window.get('seconds',0)<60 or window.get('hook_events')!=0:raise ValueError('requires isolated uninstrumented >=60s laptop quiet interval')
    for name,cap in (('cpu_percent',.1),('rss_mib',120)):
        value=window.get(name)
        if isinstance(value,bool)or not isinstance(value,(int,float))or not math.isfinite(value)or not 0<=value<=cap:raise ValueError('laptop '+name+' budget not met')
    if any(type(window.get(key))is not int or window[key]!=0 for key in ('hook_events','hidden_timer_wakeups','idle_gpu_submissions')):raise ValueError('hidden timers/GPU work not zero or unverified')
    return 'laptop quiet <=0.1% CPU, <=120MiB RSS; zero hidden timers/GPU submissions'

def inspect(root,g,installed=False,candidate=False):
    checks=[]
    def check(name,fn):
        try:checks.append({'check':name,'ok':True,'detail':str(fn())})
        except (OSError,ValueError,KeyError,TypeError,RuntimeError,subprocess.SubprocessError,ImportError)as error:checks.append({'check':name,'ok':False,'detail':str(error)})
    release=root/'packaging/dist/release';metadata=json.loads((release/'release.json').read_text())
    profile,cpu=g.build.paths.deployment_options(metadata)
    if profile!='laptop':raise ValueError('laptop preflight needs laptop manifest')
    receipt=json.loads((root/'packaging/dist/build.json').read_text());inputs=g.build.sources()
    binary=digest(root/'packaging/dist/hyprv-shell')
    spec=importlib.util.spec_from_file_location('laptop_installer',root/'packaging/install.py');installer=importlib.util.module_from_spec(spec);spec.loader.exec_module(installer)
    def binaries():
        installer.verify_release(release,receipt['release_id'])
        if receipt.get('profile')!='release'or receipt.get('deployment_profile')!='laptop'or receipt.get('cpu_power')is not cpu or receipt.get('sources')!=inputs:raise ValueError('laptop release source/options changed since build')
        for name in g.build.binaries_for('laptop',cpu):
            path=root/'packaging/dist'/name
            if path.read_bytes()[:4]!=b'\x7fELF'or digest(path)!=receipt['binaries'][name]:raise ValueError('laptop ELF missing/changed')
        if installed:
            if digest(pathlib.Path.home()/'.local/bin/hyprv-shell')!=binary or digest(pathlib.Path.home()/'.config/systemd/user/hyprv-shell.service')!=digest(release/'files/hyprv-shell.service'):raise ValueError('installed laptop binary/unit differs')
        return 'selected laptop ELF/install set verified; no combined cooling/eGPU payload'
    def tests():
        report=json.loads((root/'cutover/evidence/tests.json').read_text())
        if report.get('status')!='pass'or report.get('sources')!=inputs:raise ValueError('automated tests failed/stale')
        return 'full configured test commands PASS with current inputs'
    def helper():
        installer.verify_release(release)
        if not cpu:
            if installed and installer.DESKTOP_POLICY.exists():raise ValueError('desktop root helper policy present; manual laptop handoff review required')
            return 'N/A: user-only laptop; no CPU helper selected; cooling/eGPU absent'
        if candidate:return 'CPU-only manifest verified; installation not certified'
        record=json.loads(installer.CPU_RECORD.read_text())
        if record.get('id')!=receipt['release_id']:raise ValueError('CPU-only install release differs')
        for source,dest,mode in installer.FILES_CPU:
            path=pathlib.Path(dest);info=path.lstat()
            if not stat.S_ISREG(info.st_mode)or info.st_uid!=0 or stat.S_IMODE(info.st_mode)!=mode or digest(path)!=digest(release/source):raise ValueError('CPU-only installed artifact invalid: '+dest)
        if installer.DESKTOP_POLICY.exists():raise ValueError('desktop multi-action policy conflicts with laptop')
        for base in installer.CPU_UNIT_DIRS:
            override=base/installer.CPU_HELPER
            if override.exists()or override.is_symlink()or any((base/(installer.CPU_HELPER+'.d')).glob('*.conf')):raise ValueError('CPU-only unit administrator override requires review')
        return 'optional read-only CPU helper installed; cooling/eGPU excluded'
    check('release paths',g.validate_release_paths);check('release binaries',binaries)
    check('runtime assets',lambda:g.validate_assets(installed));check('automated tests',tests)
    check('laptop functional gate',lambda:functional(root,binary,inputs));check('parity gate',lambda:visual(root,binary,inputs))
    check('performance gate',lambda:performance(root,binary,inputs));check('CPU-only helper',helper)
    return checks
