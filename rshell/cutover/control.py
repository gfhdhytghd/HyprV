#!/usr/bin/env python3
"""Two-phase, journalled cutover. Default is a side-effect-free printed plan.

User scope runs from the checkout as the desktop user. System (root) scope runs
only from the root-owned staged release (/usr/local/lib/hyprv-cutover/current),
with /usr/bin/python3 -I, after its ownership and every manifest digest check out.
"""
import argparse, contextlib, fcntl, importlib.util, json, os, pathlib, pwd, shlex, stat, subprocess, sys, tempfile, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
STAGED=(ROOT/'release.json').is_file()and(ROOT/'MANIFEST.sha256').is_file()
RELEASE=json.loads((ROOT/'release.json').read_text())if STAGED else {}
# The user's rshell checkout (source of the hypr patch target and legacy Quickshell), also when staged.
_paths_spec=importlib.util.spec_from_file_location('hyprv_control_paths',ROOT/'packaging/release_paths.py')
paths=importlib.util.module_from_spec(_paths_spec);_paths_spec.loader.exec_module(paths)
TREE=paths.validate_metadata(RELEASE)if STAGED else ROOT
HYPRV=TREE.parent
LEGACY=json.loads((ROOT/'cutover/legacy-units.json').read_text())
HELPER='hyprv-privhelper.service'; NATIVE='hyprv-shell.service'; OWNER=RELEASE.get('owner','wilf')
SYSTEMCTL='/usr/bin/systemctl'; HYPRCTL='/usr/bin/hyprctl'; GIT='/usr/bin/git'; RUNUSER='/usr/bin/runuser'; PYTHON='/usr/bin/python3'
HOME=pathlib.Path.home(); USER_STATE=HOME/'.local/state/hyprv-cutover'
SYSTEM_STATE=pathlib.Path('/var/lib/hyprv-cutover')
RELEASES=pathlib.Path('/usr/local/lib/hyprv-cutover/releases'); STAGED_TOOL=RELEASES.parent/'current/cutover/control.py'
INSTALLED=RELEASES.parent/'installed.json'
BAR_NAMESPACE='hyprv-quickshell'; READY_TIMEOUT=20
ENV_KEYS=('WAYLAND_DISPLAY','XDG_CURRENT_DESKTOP','DBUS_SESSION_BUS_ADDRESS','DISPLAY','XDG_RUNTIME_DIR','HYPRLAND_INSTANCE_SIGNATURE')
# Unit-file states whose links `disable` removes; their exact symlinks are journalled and recreated.
DISABLE_STATES=('enabled','enabled-runtime','linked','linked-runtime','alias')
# Bounds for commands on the switch/fallback/rollback paths, so recovery itself cannot hang.
UNIT_TIMEOUT=30; RELOAD_TIMEOUT=10; LAUNCH_TIMEOUT=30
def call(cmd,check=True,timeout=None):
    print('+ '+shlex.join(map(str,cmd)),flush=True)
    return subprocess.run(list(map(str,cmd)),check=check,text=True,capture_output=False,timeout=timeout)
def output(cmd,timeout=None):return subprocess.run(list(map(str,cmd)),check=True,text=True,capture_output=True,timeout=timeout).stdout.strip()
FAILURES=(OSError,ValueError,RuntimeError,subprocess.CalledProcessError,subprocess.TimeoutExpired)
def quiet(cmd):return subprocess.run(list(map(str,cmd)),text=True,capture_output=True,timeout=UNIT_TIMEOUT)
def manager(scope):return [SYSTEMCTL]+(['--user']if scope=='user'else [])
# ---------------------------------------------------------------- root trust
def verify_staged(direction):
    """Root may only run this file from the root-owned staged release whose digests verify."""
    if not sys.flags.isolated:raise RuntimeError('run the root phase as: sudo '+PYTHON+' -I -B '+str(STAGED_TOOL)+' ...')
    if not STAGED or RELEASES not in ROOT.parents:raise RuntimeError('root phase never runs from the user-writable checkout; use sudo '+PYTHON+' -I -B '+str(STAGED_TOOL)+' ...')
    for part in [*reversed(ROOT.parents),ROOT,ROOT/'packaging',ROOT/'packaging/install.py']:
        info=part.lstat()
        if stat.S_ISLNK(info.st_mode)or info.st_uid!=0 or info.st_mode&0o022:raise RuntimeError('untrusted staged path: '+str(part))
    spec=importlib.util.spec_from_file_location('hyprv_staged_install',ROOT/'packaging/install.py');installer=importlib.util.module_from_spec(spec);spec.loader.exec_module(installer)
    rid,_=installer.verify_release(ROOT,root_owned=True)
    try:
        record=json.loads(INSTALLED.read_text());installed=record.get('id')
        if direction=='switch':paths.validate_installed_record(record,RELEASE)
    except FileNotFoundError:installed=None
    if installed!=rid:
        if direction=='switch':raise RuntimeError(f'staged tool release {rid} is not the installed release {installed}; install it first')
        print(f'# warning: rollback tool release {rid} differs from installed {installed}; continuing (rollback must work)',flush=True)
    return rid
# ---------------------------------------------------------------- units
def unit_dirs(scope):
    if scope=='system':return [pathlib.Path('/etc/systemd/system'),pathlib.Path('/run/systemd/system')]
    return [HOME/'.config/systemd/user',pathlib.Path(f'/run/user/{os.getuid()}/systemd/user')]
def unit_links(scope,unit):
    """Every symlink naming or pointing at the unit: link (linked), alias, *.wants/*.requires/*.upholds."""
    found=[]
    for base in unit_dirs(scope):
        if not base.is_dir():continue
        for path in sorted([*base.glob('*'),*base.glob('*/*')]):
            if path.is_symlink()and(path.name==unit or pathlib.PurePath(os.readlink(path)).name==unit):
                found.append({'path':str(path),'target':os.readlink(path)})
    return found
def units(scope):
    result={}
    for name in LEGACY[scope]:
        prefix=manager(scope)
        load=output(prefix+['show','--property=LoadState','--value',name])
        if load=='not-found':continue
        canonical=output(prefix+['show','--property=Id','--value',name])
        if canonical not in LEGACY[scope]:raise RuntimeError('unexpected legacy alias target: '+canonical)
        if canonical in result:continue
        name=canonical
        enabled=quiet(prefix+['is-enabled',name]).stdout.strip()
        result[name]={'enabled':enabled,'active':output(prefix+['show','--property=ActiveState','--value',name])=='active','links':unit_links(scope,name)}
    return result
def stop_legacy(scope,snapshot):
    for unit in snapshot:
        # masked/static/indirect/generated/disabled units are only stopped; their unit-file state is untouched.
        if snapshot[unit]['enabled']in DISABLE_STATES:call(manager(scope)+['disable','--now',unit],timeout=UNIT_TIMEOUT)
        else:call(manager(scope)+['stop',unit],timeout=UNIT_TIMEOUT)
def restore_units(scope,snapshot):
    """Recreate exactly the journalled links, start what was active, then compare states. Returns problems."""
    problems=[]
    for unit,before in snapshot.items():
        if 'links'not in before:
            if before['enabled']in ('enabled','enabled-runtime'):
                try:call(manager(scope)+['enable']+(['--runtime']if before['enabled']=='enabled-runtime'else [])+[unit],timeout=UNIT_TIMEOUT)
                except FAILURES as e:problems.append(f'{unit}: enable failed: {e}')
            continue
        for link in before['links']:
            path=pathlib.Path(link['path'])
            try:
                if path.is_symlink():
                    if os.readlink(path)!=link['target']:problems.append(f'{path} now points to {os.readlink(path)}, journal had {link["target"]}; left as is')
                    continue
                if path.exists():problems.append(f'{path} exists and is not a link; left as is');continue
                print(f'+ ln -s {shlex.quote(link["target"])} {shlex.quote(str(path))}',flush=True)
                path.parent.mkdir(parents=True,exist_ok=True);os.symlink(link['target'],path)
            except OSError as e:problems.append(f'{path}: link not recreated: {e}')
    try:
        if call(manager(scope)+['daemon-reload'],check=False,timeout=UNIT_TIMEOUT).returncode!=0:problems.append('daemon-reload failed')
    except FAILURES as e:problems.append('daemon-reload failed: '+str(e))
    for unit,before in snapshot.items():
        if not before['active']:continue
        try:
            if call(manager(scope)+['start',unit],check=False,timeout=UNIT_TIMEOUT).returncode!=0:problems.append(unit+': start failed')
        except FAILURES as e:problems.append(f'{unit}: start failed: {e}')
    for unit,before in snapshot.items():
        try:now=quiet(manager(scope)+['is-enabled',unit]).stdout.strip()
        except FAILURES as e:now='? ('+str(e)+')'
        if now!=before['enabled']:problems.append(f'{unit}: unit-file state {now or "?"}, was {before["enabled"]}')
    return problems
# ---------------------------------------------------------------- journal
@contextlib.contextmanager
def journal(directory):
    directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    info=directory.lstat()
    if not stat.S_ISDIR(info.st_mode)or info.st_uid!=os.geteuid()or info.st_mode&0o077:
        raise RuntimeError('journal directory must be owned by caller, mode 0700, and not a symlink: '+str(directory))
    fd=os.open(directory/'lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
    try:
        lock_info=os.fstat(fd)
        if not stat.S_ISREG(lock_info.st_mode)or lock_info.st_uid!=os.geteuid()or lock_info.st_nlink!=1 or lock_info.st_mode&0o077:raise RuntimeError('unsafe journal lock')
        fcntl.flock(fd,fcntl.LOCK_EX)
        path=directory/'state.json'
        if path.is_symlink():raise RuntimeError('journal symlink refused')
        if path.exists():
            info=path.lstat()
            if not stat.S_ISREG(info.st_mode)or info.st_uid!=os.geteuid()or info.st_nlink!=1 or info.st_mode&0o077:raise RuntimeError('unsafe journal state file')
        data=json.loads(path.read_text())if path.exists()else {}
        def save():
            fd2,temp=tempfile.mkstemp(prefix='.state-',dir=directory)
            try:
                with os.fdopen(fd2,'w')as out:json.dump(data,out,indent=2);out.write('\n');out.flush();os.fsync(out.fileno())
                os.replace(temp,path)
                directory_fd=os.open(directory,os.O_RDONLY|os.O_DIRECTORY)
                try:os.fsync(directory_fd)
                finally:os.close(directory_fd)
            finally:
                if os.path.exists(temp):os.unlink(temp)
        yield data,save
    finally:os.close(fd)
# ---------------------------------------------------------------- processes
def processes():
    for p in pathlib.Path('/proc').glob('[0-9]*'):
        try:yield int(p.name),p.stat().st_uid,(p/'cmdline').read_bytes().split(b'\0')[:-1]
        except OSError:continue
def qs_running(uid=None):
    uid=os.getuid()if uid is None else uid
    config=str((HYPRV/'quickshell').resolve()).encode()
    for _,process_uid,args in processes():
        if process_uid!=uid or not args or pathlib.Path(os.fsdecode(args[0])).name not in ('qs','quickshell'):continue
        for i,a in enumerate(args[:-1]):
            if a in (b'-p',b'--path')and args[i+1]==config:return True
    return False
def native_running():
    return any(args and pathlib.Path(os.fsdecode(args[0])).name=='hyprv-shell'for _,_,args in processes())
def start_quickshell():
    """Legacy UI back on screen; True once the main Quickshell config runs."""
    if qs_running():return True
    try:call([HYPRV/'quickshell/scripts/launch.sh'],check=False,timeout=LAUNCH_TIMEOUT)
    except FAILURES as e:print('# legacy launch failed: '+str(e),file=sys.stderr,flush=True);return False
    deadline=time.monotonic()+5
    while not qs_running():
        if time.monotonic()>deadline:return False
        time.sleep(.1)
    return True
def preflight():call([PYTHON,'-B',TREE/'cutover/preflight.py','--installed'])
# ---------------------------------------------------------------- hypr patch
def git_apply(*args,check=False):
    cmd=[GIT,'apply',*args,str(ROOT/'cutover/hypr.patch')]
    if not check:return subprocess.run(cmd,cwd=HYPRV,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
    print('+ (cd '+shlex.quote(str(HYPRV))+' && '+shlex.join(cmd)+')',flush=True)
    subprocess.run(cmd,cwd=HYPRV,check=True)
def patch_state():
    if git_apply('--check'):return 'legacy'
    if git_apply('--reverse','--check'):return 'native'
    raise RuntimeError('Hyprland files drifted from both patch states; no edits made')
def patch_files():return list(json.loads((ROOT/'cutover/hypr-inputs.json').read_text())['files'])
def backup_hypr():
    """Copies of the patched files taken right before each patch application (the files are in
    their exact legacy state then), kept for manual recovery. Refreshed every time, so a later
    cutover never points recovery at an older generation."""
    directory=USER_STATE/'hypr-backup';directory.mkdir(mode=0o700,exist_ok=True)
    for name in patch_files():
        target=directory/name.replace('/','__')
        fd,temp=tempfile.mkstemp(prefix='.backup-',dir=directory)
        with os.fdopen(fd,'wb')as out:out.write((HYPRV/name).read_bytes());out.flush();os.fsync(out.fileno())
        os.replace(temp,target)
    return directory
def patch_pairs():
    """{file: [(legacy line, native line)]} from the patch's -/+ runs."""
    pairs={};current=None;minus=[];plus=[]
    def flush():
        if current and(minus or plus):pairs[current].append((list(minus),list(plus)))
        minus.clear();plus.clear()
    for line in (ROOT/'cutover/hypr.patch').read_text().splitlines():
        if line.startswith('+++ b/'):flush();current=line[6:];pairs.setdefault(current,[])
        elif line.startswith(('--- ','diff ','index ','@@')):flush()
        elif line.startswith('-'):
            if plus:flush()
            minus.append(line[1:])
        elif line.startswith('+'):plus.append(line[1:])
        else:flush()
    flush();return pairs
def revert_by_lines():
    """Drift-tolerant reverse: each patched line found exactly once is put back. Returns (notes, unresolved)."""
    notes=[];unresolved=False
    for name,runs in patch_pairs().items():
        path=HYPRV/name;lines=path.read_text().split('\n');done=0;left=[]
        for minus,plus in runs:
            if len(minus)!=len(plus):left+=plus;continue
            for old,new in zip(minus,plus):
                hits=[i for i,l in enumerate(lines)if l==new]
                if len(hits)==1:lines[hits[0]]=old;done+=1
                elif not hits and old in lines:continue
                else:left.append(new)
        if done:
            mode=stat.S_IMODE(path.stat().st_mode);fd,temp=tempfile.mkstemp(prefix='.'+path.name+'.',dir=path.parent)
            with os.fdopen(fd,'w')as out:out.write('\n'.join(lines));out.flush();os.fsync(out.fileno())
            os.chmod(temp,mode);os.replace(temp,path)
        if left:unresolved=True;notes.append(f'{name}: {done} line(s) reverted by content; revert by hand: '+' | '.join(left))
        elif done:notes.append(f'{name}: {done} line(s) reverted by content (whole-patch reverse impossible; other content kept as is)')
    return notes,unresolved
def revert_hypr(data):
    """Never raises for drift: exact reverse, else line-level reverse, else report. Returns (notes, unresolved)."""
    if not data.get('patch_owned'):return ['hypr patch not applied by this journal; hypr files untouched'],False
    backup=USER_STATE/'hypr-backup'
    try:
        if git_apply('--check'):return ['hypr files already in legacy state'],False
        if git_apply('--reverse','--check'):git_apply('--reverse',check=True);return ['hypr patch reversed exactly'],False
        notes,unresolved=revert_by_lines()
    except (OSError,subprocess.CalledProcessError)as e:notes,unresolved=['hypr revert failed: '+str(e)],True
    if unresolved:notes.append('pre-cutover copies: '+str(backup))
    return notes,unresolved
# ---------------------------------------------------------------- native readiness
def native_surfaces(deadline=None):
    """(ready, detail): the native unit's process owns a bar layer on every enabled monitor.
    Every query is bounded by `deadline` (time.monotonic()); a hung query raises TimeoutExpired."""
    def left():return None if deadline is None else max(.1,deadline-time.monotonic())
    pid=int(output(manager('user')+['show','--property=MainPID','--value',NATIVE],timeout=left())or 0)
    if not pid:return False,'native unit has no main process'
    monitors=[m['name']for m in json.loads(output([HYPRCTL,'-j','monitors'],timeout=left()))if not m.get('disabled')]
    layers=json.loads(output([HYPRCTL,'-j','layers'],timeout=left()))
    missing=[m for m in monitors if not any(l.get('namespace')==BAR_NAMESPACE and l.get('pid')==pid for level in layers.get(m,{}).get('levels',{}).values()for l in level)]
    if not monitors:return False,'no monitors reported by hyprctl'
    return not missing,('bar surfaces missing on '+', '.join(missing))if missing else 'bar mapped on '+', '.join(monitors)
def wait_native(timeout=READY_TIMEOUT):
    ipc=[str(HOME/'.local/bin/hyprv-shell'),'ipc','call','bar','dataStatus']
    print(f'+ {shlex.join(ipc)} && {HYPRCTL} -j layers  # wait up to {timeout}s for IPC + bar surfaces',flush=True)
    deadline=time.monotonic()+timeout;detail='not started'
    while True:
        # Bound every ping: a server that accepts but never answers must not outlive the deadline.
        try:response=subprocess.run(ipc,text=True,capture_output=True,timeout=max(1.,min(5.,deadline-time.monotonic())))
        except subprocess.TimeoutExpired:response=None;detail='IPC: no reply (ping timed out)'
        if response is None:pass
        elif response.returncode!=0:detail='IPC: '+(response.stderr.strip()or 'no reply')
        else:
            try:
                ready,detail=native_surfaces(deadline)
                # Success counts only inside the budget, even when a slow reply finally looks right.
                if ready and time.monotonic()<=deadline:print('# '+detail,flush=True);return
                if ready:detail='surfaces became ready only after the deadline'
            except subprocess.TimeoutExpired:detail='surfaces: query timed out'
            except (OSError,ValueError,subprocess.CalledProcessError)as e:detail='surfaces: '+str(e)
        if time.monotonic()>=deadline:raise RuntimeError('native shell not ready within '+str(timeout)+'s: '+detail)
        time.sleep(.2)
def native_active():
    try:return subprocess.run(manager('user')+['is-active','--quiet',NATIVE],timeout=UNIT_TIMEOUT).returncode==0
    except FAILURES:return True  # unknown counts as running: never start a second shell on a guess
def stop_native():
    """Stop the native unit (SIGKILL as a last resort). True only once it is confirmed inactive."""
    for cmd in (manager('user')+['stop',NATIVE],manager('user')+['kill','--signal=SIGKILL',NATIVE]):
        try:call(cmd,check=False,timeout=UNIT_TIMEOUT)
        except FAILURES as e:print('# '+str(e),file=sys.stderr,flush=True)
        deadline=time.monotonic()+5
        while native_active()and time.monotonic()<deadline:time.sleep(.1)
        if not native_active():return True
    return False
def fallback(data,save,reason):
    """Native did not come up: stop it, undo our hypr edit and bring legacy Quickshell back.
    Legacy UI and routing come back only after native is confirmed gone (never two shells)."""
    print('# Native shell failed ('+reason+'); falling back to Quickshell',file=sys.stderr,flush=True)
    (USER_STATE/'native.enabled').unlink(missing_ok=True)
    if not stop_native():
        data.update(phase='fallback-blocked',fallback_reason=reason+'; native unit could not be stopped');save()
        print('# Native shell is still running and could not be stopped: legacy UI NOT started (it would be a second shell). Stop it, then rerun switch or rollback.',file=sys.stderr,flush=True)
        return False
    notes,unresolved=revert_hypr(data)
    for note in notes:print('# hypr: '+note,flush=True)
    data['patch_owned']=data.get('patch_owned')and unresolved
    try:call([HYPRCTL,'reload'],check=False,timeout=RELOAD_TIMEOUT)
    except FAILURES as e:notes.append('hyprctl reload failed: '+str(e))
    up=start_quickshell()
    data.update(phase='fallback',fallback_reason=reason,hypr=notes);save()
    return up
# ---------------------------------------------------------------- user scope
def user_switch(quiesce):
    if os.geteuid()==0:raise RuntimeError('user scope must run as desktop user')
    preflight()
    if not quiesce:
        if not os.environ.get('WAYLAND_DISPLAY')or not os.environ.get('XDG_RUNTIME_DIR'):raise RuntimeError('run user switch from the graphical session terminal')
        display=pathlib.Path(os.environ['WAYLAND_DISPLAY'])
        if not display.is_absolute():display=pathlib.Path(os.environ['XDG_RUNTIME_DIR'])/display
        if not display.is_socket():raise RuntimeError('Wayland socket missing; native not started')
    with journal(USER_STATE)as(data,save):
        if data.get('phase')not in ('quiesced','native','switching','fallback','fallback-blocked'):
            if patch_state()!='legacy':raise RuntimeError('patch already applied outside this journal; reverse it manually before first switch')
            data.clear();data.update(phase='captured',units=units('user'),quickshell=qs_running(),patch_owned=False);save()
        if not quiesce:
            if subprocess.run([SYSTEMCTL,'is-active','--quiet',HELPER]).returncode!=0:
                raise RuntimeError('run user --quiesce-legacy, then the root switch from the staged release first')
            for unit in LEGACY['system']:
                if subprocess.run([SYSTEMCTL,'is-active','--quiet',unit]).returncode==0:raise RuntimeError('legacy system unit remains active: '+unit)
        stop_legacy('user',data['units'])
        if qs_running():call(['quickshell','kill','-p',HYPRV/'quickshell'])
        deadline=time.monotonic()+5
        while qs_running():
            if time.monotonic()>deadline:raise RuntimeError('main Quickshell did not exit; native not started')
            time.sleep(.05)
        if quiesce:data['phase']='quiesced';save();return 0
        data['phase']='switching';save()
        try:
            if patch_state()=='legacy':
                backup_hypr()
                # Persist ownership before mutation, so interrupted cutover is reversible.
                data['patch_owned']=True;save();git_apply(check=True);call([HYPRCTL,'reload'],timeout=RELOAD_TIMEOUT)
            (USER_STATE/'native.enabled').touch(mode=0o600)
            call(manager('user')+['import-environment',*ENV_KEYS],timeout=UNIT_TIMEOUT)
            call(manager('user')+['start',NATIVE],timeout=UNIT_TIMEOUT)
            wait_native()
        except FAILURES as e:
            up=fallback(data,save,str(e))
            state='legacy Quickshell restored'if up else('native could NOT be stopped; legacy UI not started'if data['phase']=='fallback-blocked'else 'legacy Quickshell did NOT start')
            raise RuntimeError('native shell did not come up; '+state+'. Cooling stays with the helper. Fix and rerun switch, or run rollback.')from e
        data['phase']='native';save()
    return 0
def user_rollback(quiesce):
    if os.geteuid()==0:raise RuntimeError('user scope must run as desktop user')
    with journal(USER_STATE)as(data,save):
        if not data or data.get('phase')=='legacy':print('# No pending user cutover');return 0
        (USER_STATE/'native.enabled').unlink(missing_ok=True)
        if not stop_native():raise RuntimeError('native shell still active after stop (and SIGKILL); nothing else changed')
        if quiesce:
            if data.get('phase')not in ('rollback-incomplete','ui-restored'):data['phase']='native-stopped'
            save();return 0
        problems=[]
        notes,unresolved=revert_hypr(data)
        if unresolved:problems+=notes
        for note in notes:print('# hypr: '+note,flush=True)
        data['patch_owned']=data.get('patch_owned')and unresolved
        try:call([HYPRCTL,'reload'],check=False,timeout=RELOAD_TIMEOUT)
        except FAILURES as e:problems.append('hyprctl reload failed: '+str(e))
        try:helper=subprocess.run([SYSTEMCTL,'is-active','--quiet',HELPER],timeout=UNIT_TIMEOUT).returncode==0
        except FAILURES:helper=True
        if helper:problems.append('root helper still active: legacy user units left disabled; run the root rollback, then rerun this rollback')
        else:
            try:problems+=restore_units('user',data['units'])
            except FAILURES as e:problems.append('unit restoration failed: '+str(e))
        # The UI comes back whatever happened above: native is confirmed stopped.
        if not start_quickshell():problems.append('legacy Quickshell did not start: run '+str(HYPRV/'quickshell/scripts/launch.sh'))
        # Only a complete restoration is final; anything else stays retryable with the same baseline.
        data.update(phase='legacy'if not problems else('ui-restored'if helper else 'rollback-incomplete'),problems=problems);save()
    for p in problems:print('# PROBLEM: '+p,file=sys.stderr,flush=True)
    return 2 if problems else 0
# ---------------------------------------------------------------- system scope
def attributes():
    chips=[p for p in pathlib.Path('/sys/class/hwmon').glob('hwmon*')if(p/'name').read_text().strip()=='nct6799']
    if len(chips)>1:raise RuntimeError('ambiguous nct6799 chips')
    if not chips:return []
    result=[]
    for name in ('pwm1','pwm1_enable','pwm7','pwm7_enable'):
        p=chips[0]/name;s=p.stat()
        result.append({'name':name,'device':str(chips[0].resolve().parent.parent),'uid':s.st_uid,'gid':s.st_gid,'mode':stat.S_IMODE(s.st_mode)})
    return result
def set_permissions(saved,restore):
    current={r['name']:r for r in attributes()}
    for row in saved:
        if row['name']not in current or current[row['name']]['device']!=row['device']:raise RuntimeError('cooling hardware identity changed; permission restore refused')
    chips=[p for p in pathlib.Path('/sys/class/hwmon').glob('hwmon*')if(p/'name').read_text().strip()=='nct6799']
    errors=[]
    for row in saved:
        p=chips[0]/row['name'];uid,gid,mode=(row['uid'],row['gid'],row['mode'])if restore else(0,0,0o600)
        print(f'+ chown {uid}:{gid} {p}; chmod {mode:04o} {p}',flush=True)
        try:os.chown(p,uid,gid)
        except OSError as e:errors.append(f'{p}: chown: {e}')
        try:os.chmod(p,mode)
        except OSError as e:errors.append(f'{p}: chmod: {e}')
    if errors:raise RuntimeError('; '.join(errors))
def owner_manager():
    owner=pwd.getpwnam(OWNER);run=f'/run/user/{owner.pw_uid}'
    return [RUNUSER,'-u',OWNER,'--','/usr/bin/env','HOME='+owner.pw_dir,'XDG_RUNTIME_DIR='+run,'DBUS_SESSION_BUS_ADDRESS=unix:path='+run+'/bus']
def ensure_helper():
    """Idempotent: the helper must be unmasked, enabled and active once the root journal says native."""
    if quiet([SYSTEMCTL,'is-enabled',HELPER]).stdout.strip().startswith('masked'):call([SYSTEMCTL,'unmask',HELPER],timeout=UNIT_TIMEOUT)
    call([SYSTEMCTL,'enable','--now',HELPER],timeout=UNIT_TIMEOUT);call([SYSTEMCTL,'is-active','--quiet',HELPER],timeout=UNIT_TIMEOUT)
def owner_preflight_command():
    # Validate again immediately before handoff (including targets behind symlinks).
    tree=paths.validate_metadata(RELEASE)if STAGED else paths.stable_path(str(TREE))
    return owner_manager()+[PYTHON,'-B',tree/'cutover/preflight.py','--installed']
def cooling_capability():
    # Read/open-only checks first, then exercise the failsafe while legacy still
    # owns cooling. A broken binary/sysfs store cannot strand stopped units.
    call(['/usr/libexec/hyprv-cooling-failsafe','--check'],timeout=UNIT_TIMEOUT)
    call(['/usr/libexec/hyprv-privhelper','--check'],timeout=UNIT_TIMEOUT)
    call(['/usr/libexec/hyprv-cooling-failsafe'],timeout=UNIT_TIMEOUT)
def validate_cooling_identity(data):
    current={r['name']:r for r in attributes()}
    if any(r['name']not in current or r['device']!=current[r['name']]['device']for r in data['attributes']):
        raise RuntimeError('cooling device changed; explicit recovery needed')
def recover_system(data,save):
    """Restore the captured legacy owner, regardless of a broken failsafe.

    Never recapture a partially changed baseline. Errors in an individual step
    cannot skip the other restoration steps. Unknown helper state is unsafe.
    """
    validate_cooling_identity(data)
    problems=[];warnings=[]
    # Check/establish high duty before relinquishing the current owner, but a
    # defective old binary must never veto the independent legacy recovery.
    try:cooling_capability()
    except FAILURES as e:warnings.append('cooling capability: '+str(e))
    # Persist intent, but even a full disk must not block immediate restoration.
    data['phase']='rollback-incomplete'
    try:save()
    except FAILURES as e:warnings.append('journal intent: '+str(e))
    for cmd in ([SYSTEMCTL,'disable','--now',HELPER],[SYSTEMCTL,'mask','--now',HELPER]):
        try:call(cmd,timeout=UNIT_TIMEOUT)
        except FAILURES as e:warnings.append('helper stop/mask: '+str(e))
    try:
        state=output([SYSTEMCTL,'show','--property=ActiveState','--value',HELPER],timeout=UNIT_TIMEOUT)
        if state not in ('inactive','failed'):raise RuntimeError('helper still active or unknown ('+state+'); legacy cooling NOT restored')
    except FAILURES as e:
        data.update(problems=[str(e)],warnings=warnings);save();raise RuntimeError('helper still active or unknown; legacy cooling NOT restored')from e
    # A failed old failsafe MUST NOT abort the legacy restoration again.
    try:call(['/usr/libexec/hyprv-cooling-failsafe'],timeout=UNIT_TIMEOUT)
    except FAILURES as e:warnings.append('failsafe: '+str(e))
    try:set_permissions(data['attributes'],True)
    except FAILURES as e:problems.append('permissions: '+str(e))
    ordered=dict(sorted(data['units'].items(),key=lambda item:0 if 'permissions'in item[0]else 1))
    try:problems+=restore_units('system',ordered)
    except FAILURES as e:problems.append('legacy restoration: '+str(e))
    # Verify actual active states, including originally inactive units. Do not
    # declare legacy on a successful `start` that immediately failed afterwards.
    for unit,before in ordered.items():
        try:
            state=output([SYSTEMCTL,'show','--property=ActiveState','--value',unit],timeout=UNIT_TIMEOUT)
            if not before['active'] and state=='active':
                call([SYSTEMCTL,'stop',unit],timeout=UNIT_TIMEOUT)
                state=output([SYSTEMCTL,'show','--property=ActiveState','--value',unit],timeout=UNIT_TIMEOUT)
            if (state=='active')!=before['active'] or state not in ('active','inactive','failed'):
                problems.append(f'{unit}: active state {state}, expected {before["active"]}')
        except FAILURES as e:problems.append(f'{unit}: cannot verify active state: {e}')
    data.update(phase='legacy'if not problems else 'rollback-incomplete',problems=problems,warnings=warnings);save()
    for message in warnings:print('# WARNING: '+message,file=sys.stderr,flush=True)
    for message in problems:print('# PROBLEM: '+message,file=sys.stderr,flush=True)
    return 2 if problems else 0

def root_handoff(direction):
    if os.geteuid()!=0:raise RuntimeError('system scope requires root')
    verify_staged(direction)
    if direction!='switch'and native_running():raise RuntimeError('stop every native shell before root rollback/recovery')
    with journal(SYSTEM_STATE)as(data,save):
        if direction in ('rollback','recover'):
            if not data or data.get('phase')=='legacy':print('# No pending system cutover');return 0
            # Recovery is deliberately independent of readiness/preflight and of
            # whether manually restored legacy units are already active.
            return recover_system(data,save)
        if data.get('phase')=='switching':
            result=recover_system(data,save)
            print('# Interrupted switch reconciled toward legacy; rerun switch explicitly after inspection',flush=True)
            return result or 2
        if data.get('phase')=='rollback-incomplete':raise RuntimeError('a root rollback is incomplete; finish it (rerun rollback or recover) before switching again')
        # All capability/readiness checks precede any legacy unit mutation.
        call(owner_preflight_command(),timeout=120)
        for unit in LEGACY['user']:
            load=output(owner_manager()+[SYSTEMCTL,'--user','show','--property=LoadState','--value',unit],timeout=UNIT_TIMEOUT)
            if load!='not-found'and output(owner_manager()+[SYSTEMCTL,'--user','show','--property=ActiveState','--value',unit],timeout=UNIT_TIMEOUT)=='active':raise RuntimeError('quiesce legacy user unit first: '+unit)
        if qs_running(pwd.getpwnam(OWNER).pw_uid):raise RuntimeError('quiesce main Quickshell before root handoff')
        cooling_capability()
        if data.get('phase')=='native':
            ensure_helper();print('# Root handoff already native; helper unmasked/enabled/active');return 0
        data.clear();data.update(phase='switching',units=units('system'),attributes=attributes());save()
        managed={int(output([SYSTEMCTL,'show','--property=MainPID','--value',u],timeout=UNIT_TIMEOUT)or 0)for u in data['units']}
        for pid,uid,args in processes():
            names=[pathlib.Path(os.fsdecode(a)).name for a in args]
            if uid==pwd.getpwnam(OWNER).pw_uid and pid not in managed and any(n in ('hyprv-fanctl','fan-control-helper.sh')for n in names)and any(a in (b'run',b'run-system')for a in args):raise RuntimeError('unmanaged legacy cooling loop still running: '+str(pid))
        try:
            stop_legacy('system',data['units'])
            call(['/usr/libexec/hyprv-cooling-failsafe'],timeout=UNIT_TIMEOUT)
            set_permissions(data['attributes'],False)
            ensure_helper()
            data['phase']='native';save();return 0
        except BaseException as error:
            # Includes interrupts and journal-save failures after unit changes.
            # Original baseline remains in the journal for a repeatable recovery.
            try:result=recover_system(data,save)
            except BaseException as recovery:
                raise RuntimeError(f'root switch failed: {error}; legacy recovery incomplete: {recovery}; run recover --scope system --apply')from error
            raise RuntimeError(f'root switch failed: {error}; '+('legacy units restored'if result==0 else 'legacy recovery incomplete; run recover --scope system --apply'))from error
# ---------------------------------------------------------------- plans
def plan(direction,scope,quiesce,ab):
    print(f'# PLAN ONLY: {direction}, scope={scope}. No subprocess, file write, service action, sysfs access or patch application is performed.')
    if ab:
        print('# A/B UNSUPPORTED: current native CLI mounts every output and original QML has no output exclusion contract. No second shell will be launched. Requested output: '+ab);return 4
    if scope=='system':
        print(f'# Root phase runs ONLY from the root-owned staged release (never from this checkout):\nsudo {PYTHON} -I -B {STAGED_TOOL} {direction} --scope system --apply')
        print('# It verifies root ownership of the release tree and every MANIFEST digest; switch also requires it to be the installed release. Journal /var/lib/hyprv-cutover/state.json (0700 dir, 0600 file).')
        if direction=='switch':
            print('+ /usr/libexec/hyprv-cooling-failsafe --check\n+ /usr/libexec/hyprv-privhelper --check\n+ /usr/libexec/hyprv-cooling-failsafe  # capability before stopping legacy')
            print('# Require green preflight (run as the desktop user) and stopped legacy user controller/main Quickshell; journal unit states with their exact symlinks and nct6799 PWM ownership/modes.')
            for u in LEGACY['system']:print('+ systemctl disable --now '+u+'  # if installed and enabled/linked; otherwise stop only')
            print('+ /usr/libexec/hyprv-cooling-failsafe\n# chown root:root, chmod 0600: nct6799 pwm1, pwm1_enable, pwm7, pwm7_enable (resolve unique chip; journal exact prior values)\n+ systemctl unmask '+HELPER+'  # if masked\n+ systemctl enable --now '+HELPER)
            print('# Re-run when already native: re-ensures the helper is unmasked/enabled/active (repairs a masked helper).')
        else:
            print('# Require native shell stopped; validate journal and cooling hardware identity. Reconcile interrupted switching to journalled legacy state, preserving the original baseline.')
            print('# Failsafe errors are warnings: restoration still runs; helper must be confirmed inactive first. Verify actual unit states before phase=legacy.')
            print('+ systemctl disable --now '+HELPER+'\n+ /usr/libexec/hyprv-cooling-failsafe\n+ systemctl mask '+HELPER+'\n# Restore journalled PWM owners/modes; recreate exactly the journalled unit symlinks (enabled/linked/alias/runtime), permissions unit first, start previously active units, compare unit-file states:')
            print('# '+', '.join(LEGACY['system']))
    elif direction=='switch':
        print('+ '+shlex.join([PYTHON,'-B',str(TREE/'cutover/preflight.py'),'--installed']))
        print('# Record original unit states with their symlinks and whether main Quickshell was running; an existing transaction is resumed.')
        for u in LEGACY['user']:print('+ systemctl --user disable --now '+u+'  # if installed and enabled/linked; otherwise stop only')
        print('+ quickshell kill -p '+shlex.quote(str(HYPRV/'quickshell'))+'  # only if that config runs; wait for exit')
        if quiesce:print('# Save phase=quiesced; next run the root switch from the staged release');return 0
        print('# Require root helper active and listed legacy system units inactive. Back up the three hypr files to ~/.local/state/hyprv-cutover/hypr-backup, record patch ownership, then:')
        print('+ git -C '+shlex.quote(str(HYPRV))+' apply '+shlex.quote(str(ROOT/'cutover/hypr.patch'))+'  # only from the exact preimage\n+ hyprctl reload\n# Create ~/.local/state/hyprv-cutover/native.enabled')
        print('+ systemctl --user import-environment '+' '.join(ENV_KEYS)+'\n+ systemctl --user start '+NATIVE)
        print(f'# Ready = `hyprv-shell ipc call bar dataStatus` succeeds AND hyprctl shows a {BAR_NAMESPACE} layer owned by the unit MainPID on every enabled monitor, within {READY_TIMEOUT}s.')
        print('# Otherwise automatic fallback: stop native, remove marker, reverse our hypr edit, hyprctl reload, launch legacy Quickshell; phase=fallback (cooling stays with the helper).')
    else:
        print('# Remove native.enabled marker.\n+ systemctl --user stop '+NATIVE)
        if quiesce:print('# Save phase=native-stopped (no hypr check); next run the root rollback from the staged release');return 0
        print('# Reverse our hypr edit: exact `git apply --reverse`, else line-by-line by content (other edits kept), else report the lines to revert by hand + backup path. Never blocks the rest.')
        print('+ hyprctl reload')
        print('# If the helper is stopped: recreate the journalled user unit links ('+', '.join(LEGACY['user'])+') and start what ran. If still active: keep them disabled, phase=ui-restored, rerun after root rollback.')
        print('+ '+shlex.quote(str(HYPRV/'quickshell/scripts/launch.sh'))+'  # always, unless already running')
    return 0
def main():
    p=argparse.ArgumentParser();p.add_argument('direction',choices=['switch','rollback','recover']);p.add_argument('--scope',choices=['user','system'],default='user');g=p.add_mutually_exclusive_group();g.add_argument('--apply',action='store_true');g.add_argument('--dry-run',action='store_true');p.add_argument('--quiesce-legacy',action='store_true');p.add_argument('--quiesce-native',action='store_true');p.add_argument('--ab-output');a=p.parse_args()
    quiesce=a.quiesce_legacy or a.quiesce_native
    if a.direction=='recover'and a.scope!='system':p.error('recover requires --scope system')
    if (a.quiesce_legacy and a.direction!='switch')or(a.quiesce_native and a.direction!='rollback')or(quiesce and a.scope!='user'):p.error('quiesce flag must match direction and user scope')
    if not a.apply or a.ab_output:return plan(a.direction,a.scope,quiesce,a.ab_output)
    try:
        if a.scope=='system':return root_handoff(a.direction)
        if a.direction=='switch':return user_switch(quiesce)
        return user_rollback(quiesce)
    except FAILURES as e:print('cutover blocked: '+str(e),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
