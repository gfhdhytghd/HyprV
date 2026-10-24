#!/usr/bin/env python3
"""User-only laptop cutover. Default is a printed plan; never manages root services."""
import argparse, importlib.util, json, os, pathlib, shlex, subprocess, sys, tempfile, time
FILE=pathlib.Path(__file__).resolve()
ROOT=FILE.parents[2]if FILE.parent.name=='laptop'else FILE.parents[1]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
installer=load('laptop_install',ROOT/'packaging/install.py')
STAGED=(ROOT/'release.json').is_file()
RELEASE=ROOT if STAGED else ROOT/'packaging/dist/release'
HOME=pathlib.Path.home();STATE=HOME/'.local/state/hyprv-cutover/laptop'
MARKER=HOME/'.local/state/hyprv-cutover/native.enabled'
UNIT='hyprv-shell.service'
PROC=pathlib.Path('/proc')
def command(args):
    print('+ '+shlex.join(map(str,args)),flush=True)
    subprocess.run(list(map(str,args)),check=True,timeout=30)
def output(args):
    return subprocess.run(list(map(str,args)),check=True,capture_output=True,text=True,timeout=3).stdout.strip()
def native_running():
    return subprocess.run(['/usr/bin/systemctl','--user','is-active','--quiet',UNIT],capture_output=True,timeout=3).returncode==0

def wait_native(timeout=20):
    deadline=time.monotonic()+timeout;detail='not started'
    while time.monotonic()<deadline:
        try:
            pid=int(output(['/usr/bin/systemctl','--user','show','--property=MainPID','--value',UNIT])or 0)
            if not pid:raise ValueError('unit has no process')
            if installer.sha(PROC/str(pid)/'exe')!=installer.sha(HOME/'.local/bin/hyprv-shell'):raise ValueError('running ELF differs from installed candidate')
            if b'HYPRV_PROFILE=laptop'not in (PROC/str(pid)/'environ').read_bytes().split(b'\0'):raise ValueError('running process lacks laptop profile')
            monitors=[m['name']for m in json.loads(output(['/usr/bin/hyprctl','-j','monitors']))if not m.get('disabled')]
            layers=json.loads(output(['/usr/bin/hyprctl','-j','layers']))
            if not monitors:raise ValueError('no active outputs')
            for name in monitors:
                owned=[row for level in layers.get(name,{}).get('levels',{}).values()for row in level if row.get('pid')==pid]
                if not any(row.get('namespace')=='hyprv-quickshell'or row.get('namespace','').startswith('hyprv-screen-corner-overlay-')for row in owned):raise ValueError('native layer missing on '+name)
                status=json.loads(output([HOME/'.local/bin/hyprv-shell','ipc','call','bar','layoutStatus',name]))
                if not isinstance(status,dict)or type(status.get('expanded'))is not bool:raise ValueError('bar IPC model missing on '+name)
            return
        except (OSError,ValueError,subprocess.SubprocessError)as error:detail=str(error)
        time.sleep(min(.1,max(0,deadline-time.monotonic())))
    raise RuntimeError('native laptop readiness failed: '+detail)

def metadata(release=RELEASE):
    rid,_=installer.verify_release(release)
    data=json.loads((release/'release.json').read_text())
    if installer.paths.deployment_options(data)[0]!='laptop':raise RuntimeError('requires a laptop release')
    return rid,data

def inputs(release):
    data=json.loads((release/'cutover/hypr-inputs.json').read_text())
    if data.get('schema')!=1 or not data.get('files'):raise RuntimeError('missing laptop patch input/output hashes')
    for name,row in data['files'].items():
        path=pathlib.PurePosixPath(name)
        if path.is_absolute()or '..'in path.parts or path.parts[0]!='hypr':raise RuntimeError('unsafe patch path')
        if set(row)!={'before_sha256','after_sha256'}:raise RuntimeError('invalid patch hashes')
    return data['files']

def prepare(release,tree):
    """Generate in private scratch, then transact verified bytes; never git-apply live."""
    rows=inputs(release);temporary=tempfile.TemporaryDirectory(prefix='hyprv-laptop-patch-')
    scratch=pathlib.Path(temporary.name)
    try:
        before=[]
        for name,row in rows.items():
            source=tree/name
            if not source.resolve().is_relative_to((tree/'hypr').resolve())or source.is_symlink()or installer.sha(source)!=row['before_sha256']:raise RuntimeError('laptop config base drift: '+name)
            target=scratch/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(source.read_bytes())
            before.append((target,source,source.stat().st_mode&0o777))
        patch=release/'cutover/hypr.patch'
        subprocess.run(['/usr/bin/git','apply','--check',str(patch.resolve())],cwd=scratch,check=True)
        subprocess.run(['/usr/bin/git','apply',str(patch.resolve())],cwd=scratch,check=True)
        for source,dest,_ in before:
            if installer.sha(source)!=rows[str(dest.relative_to(tree))]['after_sha256']:raise RuntimeError('patched output differs from manifest')
        return temporary,before
    except BaseException:temporary.cleanup();raise

def legacy_running(config):
    """Only the same owner's exact Quickshell config; never the dcal family."""
    for entry in pathlib.Path('/proc').iterdir():
        if not entry.name.isdigit():continue
        try:
            if entry.stat().st_uid!=os.getuid():continue
            argv=(entry/'cmdline').read_bytes().decode().split('\0')
            if pathlib.Path(argv[0]).name not in ('quickshell','qs'):continue
            for n,arg in enumerate(argv[:-1]):
                if arg in ('-p','--path')and pathlib.Path(argv[n+1]).expanduser().resolve()==config.resolve():return True
        except (OSError,ValueError,UnicodeError):continue
    return False

class PatchTransaction(installer.Transaction):
    def save(self,data):
        previous=self.load()
        for key in ('legacy_running','marker_was_present','workspace','patch_hashes'):
            if key in previous:data.setdefault(key,previous[key])
        super().save(data)

def execute(direction,apply=False):
    if apply and os.geteuid()==0:raise RuntimeError('laptop cutover runs only as the session user')
    prior=PatchTransaction(STATE,os.getuid()).load()if direction=='rollback'else {}
    if direction=='rollback'and not prior:
        print('# No laptop journal; rollback is already complete.')
        return 0
    if prior and prior.get('workspace'):
        rid=prior['release'];data={'workspace':str(installer.paths.stable_path(prior['workspace']))}
    else:rid,data=metadata()
    tree=pathlib.Path(data['workspace']).parent
    if not apply:
        print('# PLAN ONLY: user session/config files only; no root unit or helper handoff')
        print('# '+direction+' laptop release '+rid)
        print('# Check full installed laptop preflight, base/output hashes and journal; restore files on failure.')
        print('+ systemctl --user '+('start'if direction=='switch'else'stop')+' '+UNIT)
        print('# '+('Stop only matching legacy config, transact patch and native.enabled marker.'if direction=='switch'else'Restore journalled config/marker; relaunch only legacy config previously running.'))
        return 0
    tx=PatchTransaction(STATE,os.getuid())
    with tx.locked():
        journal=tx.load()
        if direction=='switch'and journal.get('phase')=='done':
            for name,row in journal.get('patch_hashes',{}).items():
                if installer.sha(tree/name)!=row['after_sha256']:raise RuntimeError('active laptop config drift: '+name)
            if journal.get('release')!=rid:raise RuntimeError('another laptop generation active; rollback before switching')
            command(['/usr/bin/systemctl','--user','start',UNIT]);wait_native();return 0
        if direction=='rollback'and(not journal or journal.get('phase')=='restored'):return 0
        if direction=='switch':
            if journal and journal.get('phase')not in ('done','restored'):raise RuntimeError('interrupted laptop switch; run rollback first')
            if native_running():raise RuntimeError('native unit active outside laptop journal; resolve prior cutover first')
            command([sys.executable,'-B',pathlib.Path(data['workspace'])/'cutover/preflight.py','--installed'])
            temporary,changes=prepare(RELEASE,tree)
            previous=legacy_running(tree/'quickshell');marker=MARKER.exists()
            # Persist intent before stopping anything. Transaction.run preserves the extras below.
            pending={'phase':'switching','legacy_running':previous,'marker_was_present':marker,'release':rid,'workspace':data['workspace'],'patch_hashes':inputs(RELEASE),'tx':'pending','entries':[]}
            tx.save(pending)
            committed=None
            try:
                if previous:command(['quickshell','kill','-p',tree/'quickshell'])
                committed=tx.run(changes,rid)
                committed.update(legacy_running=previous,marker_was_present=marker)
                tx.save(committed)
                MARKER.parent.mkdir(parents=True,exist_ok=True);MARKER.touch(mode=0o600)
                command(['/usr/bin/systemctl','--user','start',UNIT])
                command(['/usr/bin/systemctl','--user','is-active','--quiet',UNIT])
                # Check process/profile/ELF and PID-owned layers plus IPC on each output.
                wait_native()
                tx.finish(committed)
            except BaseException:
                if committed:tx.restore(committed)
                if not marker:MARKER.unlink(missing_ok=True)
                command(['/usr/bin/systemctl','--user','stop',UNIT])
                if previous:command([tree/'quickshell/scripts/launch.sh'])
                if not committed:pending['phase']='restored';tx.save(pending)
                raise
            finally:temporary.cleanup()
        else:
            # Refuse to clobber unrelated edits made after the cutover.
            rows=journal.get('patch_hashes')or inputs(RELEASE)
            for entry in journal.get('entries',[]):
                dest=pathlib.Path(entry['dest']);name=str(dest.relative_to(tree))
                if installer.sha(dest)not in (rows[name]['before_sha256'],rows[name]['after_sha256']):raise RuntimeError('config changed after switch: '+name)
            command(['/usr/bin/systemctl','--user','stop',UNIT])
            if journal.get('entries'):tx.restore(journal)
            else:journal['phase']='restored';tx.save(journal)
            if not journal.get('marker_was_present'):MARKER.unlink(missing_ok=True)
            if journal.get('legacy_running'):command([tree/'quickshell/scripts/launch.sh'])
    return 0

def main():
    p=argparse.ArgumentParser();p.add_argument('direction',choices=['switch','rollback']);g=p.add_mutually_exclusive_group();g.add_argument('--apply',action='store_true');g.add_argument('--dry-run',action='store_true');a=p.parse_args()
    try:return execute(a.direction,a.apply)
    except (OSError,RuntimeError,ValueError,KeyError,subprocess.SubprocessError)as error:print('laptop cutover: '+str(error),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
