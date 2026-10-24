#!/usr/bin/env python3
"""Plans by default; --apply installs. Neither mode starts the shell.

Root scope runs only from a root-owned, staged release under RELEASES (see
cutover/README.md): every file is root-owned, not group/other-writable, no
symlinks, and matches MANIFEST.sha256 whose digest is the release id typed on
the command line. Nothing from the user's tree is read or executed as root.

Installs are journalled transactions: new files are written beside their
destination, the old ones are backed up, then everything is renamed into place.
Any failure, or an interrupted run found on the next start, restores the backups.
"""
import argparse, contextlib, fcntl, hashlib, json, os, pathlib, shlex, shutil, stat, subprocess, sys, time
import importlib.util
_path_spec=importlib.util.spec_from_file_location('hyprv_release_paths',pathlib.Path(__file__).with_name('release_paths.py'))
paths=importlib.util.module_from_spec(_path_spec);_path_spec.loader.exec_module(paths)
ROOT=pathlib.Path(__file__).resolve().parents[1]
RELEASES=pathlib.Path('/usr/local/lib/hyprv-cutover/releases')
CURRENT=RELEASES.parent/'current'
INSTALLED=RELEASES.parent/'installed.json'
STATE_ROOT=pathlib.Path('/var/lib/hyprv-cutover')
SYSTEMCTL='/usr/bin/systemctl'
HELPER='hyprv-privhelper.service'
MANIFEST='MANIFEST.sha256'
# (path inside the release, destination, mode)
FILES_ROOT=[
 ('bin/hyprv-privhelper','/usr/libexec/hyprv-privhelper',0o755),
 ('bin/hyprv-cooling-failsafe','/usr/libexec/hyprv-cooling-failsafe',0o755),
 ('files/hyprv-privhelper.service','/usr/lib/systemd/system/hyprv-privhelper.service',0o644),
 ('files/org.hyprv.Shell.PrivHelper.service','/usr/share/dbus-1/system-services/org.hyprv.Shell.PrivHelper.service',0o644),
 ('files/org.hyprv.Shell.PrivHelper.conf','/etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf',0o644),
 ('files/org.hyprv.shell.fan-control.policy','/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy',0o644)]
FILES_CPU=[
 ('bin/hyprv-cpu-power','/usr/libexec/hyprv-cpu-power',0o755),
 ('files/hyprv-cpu-power-only.service','/usr/lib/systemd/system/hyprv-cpu-power-only.service',0o644),
 ('files/org.hyprv.Shell.CpuPower.service','/usr/share/dbus-1/system-services/org.hyprv.Shell.CpuPower.service',0o644),
 ('files/org.hyprv.Shell.CpuPower.conf','/etc/dbus-1/system.d/org.hyprv.Shell.CpuPower.conf',0o644),
 ('files/org.hyprv.shell.cpu-power.policy','/usr/share/polkit-1/actions/org.hyprv.shell.cpu-power.policy',0o644)]
CPU_HELPER='hyprv-cpu-power-only.service'
CPU_UNIT_DIRS=(pathlib.Path('/etc/systemd/system'),pathlib.Path('/run/systemd/system'))
CPU_STATE=pathlib.Path('/var/lib/hyprv-cpu-power-install')
CPU_RECORD=pathlib.Path('/usr/local/lib/hyprv-cpu-power/installed.json')
DESKTOP_POLICY=pathlib.Path('/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy')
FILES_USER=[
 ('bin/hyprv-shell','.local/bin/hyprv-shell',0o755),
 ('files/hyprv-shell.service','.config/systemd/user/hyprv-shell.service',0o644)]
def sha(p):
    h=hashlib.sha256()
    with open(p,'rb')as f:
        for block in iter(lambda:f.read(1<<20),b''):h.update(block)
    return h.hexdigest()
def candidate():return ROOT/'packaging/dist/release'
def user_files(release):
    """Runtime payload paths come only from the hash-verified release catalog."""
    catalog=json.loads((release/'runtime/runtime-assets.json').read_text())
    if catalog.get('schema')!=1 or not catalog.get('files'):raise RuntimeError('invalid runtime asset catalog')
    metadata=json.loads((release/'release.json').read_text());profile,_=paths.deployment_options(metadata)
    files=list(FILES_USER)+[('runtime/runtime-assets.json','.local/share/hyprv-shell/runtime-assets.json',0o644)]
    for name,digest in sorted(catalog['files'].items()):
        path=pathlib.PurePosixPath(name)
        if path.is_absolute()or '..'in path.parts or path.parts[0]not in ('assets','icons'):raise RuntimeError('unsafe runtime asset name: '+name)
        source=release/'runtime'/name
        if source.is_symlink()or sha(source)!=digest:raise RuntimeError('runtime asset digest mismatch: '+name)
        files.append(('runtime/'+name,'.local/share/hyprv-shell/'+name,0o644))
    if profile=='laptop':files.append(('release.json','.local/share/hyprv-shell/release.json',0o644))
    return files
# ---------------------------------------------------------------- release trust
def read_manifest(release):
    rows={}
    for line in (release/MANIFEST).read_text().splitlines():
        digest,_,name=line.partition('  ')
        if len(digest)!=64 or not name or name.startswith('/')or '..'in pathlib.PurePosixPath(name).parts or name in rows:raise RuntimeError('malformed manifest line: '+line)
        rows[name]=digest
    return rows
def verify_release(release,expected_id=None,root_owned=False):
    """Hash-check every file of a release; optionally require root ownership of the tree and path."""
    release=pathlib.Path(release)
    if root_owned:
        for part in [*reversed(release.parents),release]:
            info=part.lstat()
            if stat.S_ISLNK(info.st_mode)or not stat.S_ISDIR(info.st_mode)or info.st_uid!=0 or info.st_mode&0o022:raise RuntimeError('untrusted release path component: '+str(part))
    rid=sha(release/MANIFEST)
    if expected_id is not None and rid!=expected_id:raise RuntimeError(f'release id mismatch: {MANIFEST} digest is {rid}, expected {expected_id}')
    rows=read_manifest(release);seen=set()
    for path in release.rglob('*'):
        info=path.lstat();rel=str(path.relative_to(release))
        if root_owned and(info.st_uid!=0 or info.st_mode&0o022 or info.st_mode&(stat.S_ISUID|stat.S_ISGID)):raise RuntimeError('release file not root-owned/read-only: '+rel)
        if stat.S_ISDIR(info.st_mode):continue
        if not stat.S_ISREG(info.st_mode):raise RuntimeError('release contains a non-regular file: '+rel)
        if rel==MANIFEST:continue
        if rel not in rows:raise RuntimeError('file not in manifest: '+rel)
        if sha(path)!=rows[rel]:raise RuntimeError('manifest digest mismatch: '+rel)
        seen.add(rel)
    if seen!=set(rows):raise RuntimeError('manifest lists missing files: '+', '.join(sorted(set(rows)-seen)))
    metadata=json.loads((release/'release.json').read_text())
    profile,cpu_power=paths.deployment_options(metadata)
    if profile=='laptop':
        required={'release.json','bin/hyprv-shell','files/hyprv-shell.service','packaging/install.py',
                  'packaging/release_paths.py','cutover/control.py','cutover/hypr.patch','cutover/hypr-inputs.json',
                  'runtime/runtime-assets.json'}
        catalog=json.loads((release/'runtime/runtime-assets.json').read_text())
        required|={'runtime/'+name for name in catalog.get('files',{})}
        if cpu_power:required|={source for source,_,_ in FILES_CPU}
        if set(rows)!=required:raise RuntimeError('laptop release install set missing/extra files: '+str(set(rows)^required))
    return rid,rows
def require_root_context(release,expected_id):
    if os.geteuid()!=0:raise RuntimeError('root install requires root')
    if not sys.flags.isolated:raise RuntimeError('run root tools with /usr/bin/python3 -I')
    if RELEASES not in pathlib.Path(release).parents:raise RuntimeError(f'root install runs only from a staged release under {RELEASES}, not {release}')
    return verify_release(release,expected_id,root_owned=True)
# ---------------------------------------------------------------- transaction
def fsync_dir(d):
    fd=os.open(d,os.O_RDONLY|os.O_DIRECTORY)
    try:os.fsync(fd)
    finally:os.close(fd)
def write_file(path,data,mode):
    """Exclusive, no-follow temp beside path; returns the temp path."""
    temp=path.with_name('.'+path.name+'.hyprv-new')
    with contextlib.suppress(FileNotFoundError):
        if stat.S_ISREG(temp.lstat().st_mode):temp.unlink()
    fd=os.open(temp,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,mode)
    try:
        os.fchmod(fd,mode);view=memoryview(data)
        while view:view=view[os.write(fd,view):]
        os.fsync(fd)
    finally:os.close(fd)
    return temp
def atomic_json(path,data,mode=0o600):
    temp=write_file(path,(json.dumps(data,indent=2)+'\n').encode(),mode);os.replace(temp,path);fsync_dir(path.parent)
@contextlib.contextmanager
def orchestration_lock(state=None):
    """The root orchestration lock, shared with cutover/control.py's root journal (state/lock).
    Lock order: this lock, then install.lock. Held for the whole root install."""
    state=state or STATE_ROOT
    state.mkdir(parents=True,exist_ok=True,mode=0o700);info=state.lstat()
    if not stat.S_ISDIR(info.st_mode)or info.st_uid!=os.geteuid()or info.st_mode&0o077:raise RuntimeError('unsafe root state directory: '+str(state))
    fd=os.open(state/'lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
    try:
        lock=os.fstat(fd)
        if not stat.S_ISREG(lock.st_mode)or lock.st_uid!=os.geteuid()or lock.st_nlink!=1 or lock.st_mode&0o077:raise RuntimeError('unsafe orchestration lock')
        fcntl.flock(fd,fcntl.LOCK_EX);yield
    finally:os.close(fd)
class Transaction:
    """Journalled replace of a set of files with full restore on failure."""
    def __init__(self,state,owner_uid):
        self.state=state;self.uid=owner_uid;self.journal=state/'install.json'
    def safe_dir(self,d,mode=0o700):
        d.mkdir(parents=True,exist_ok=True,mode=mode);info=d.lstat()
        if not stat.S_ISDIR(info.st_mode)or info.st_uid!=self.uid or info.st_mode&0o077:raise RuntimeError('unsafe installer state directory: '+str(d))
    @contextlib.contextmanager
    def locked(self):
        self.safe_dir(self.state)
        fd=os.open(self.state/'install.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
        try:fcntl.flock(fd,fcntl.LOCK_EX);yield
        finally:os.close(fd)
    def load(self):
        if self.journal.is_symlink():raise RuntimeError('install journal symlink refused')
        return json.loads(self.journal.read_text())if self.journal.exists()else {}
    def save(self,data):atomic_json(self.journal,data)
    @staticmethod
    def dest_ok(dest):
        try:info=dest.lstat()
        except FileNotFoundError:return False
        if not stat.S_ISREG(info.st_mode):raise RuntimeError('destination is not a regular file (refusing to follow/replace): '+str(dest))
        return True
    # Terminal phases. Anything else (prepared, committing, committed, restoring, or an unknown
    # phase) is an unfinished install: files may be new while systemd, the running helper or the
    # published record are not. Recovery restores the previous generation; the caller then runs
    # a complete install again (reload, restart, health check, publication).
    TERMINAL=('done','restored')
    def recover(self):
        """Restore any unfinished transaction. Returns True if one was restored."""
        data=self.load()
        if not data or data.get('phase')in self.TERMINAL:return False
        print('# Restoring interrupted install transaction '+data['tx'],flush=True)
        self.restore(data);return True
    def restore(self,data):
        data['phase']='restoring';self.save(data)
        backup=pathlib.Path(data['backup'])
        for entry in data['entries']:
            dest=pathlib.Path(entry['dest'])
            temp=dest.with_name('.'+dest.name+'.hyprv-new')
            with contextlib.suppress(FileNotFoundError):temp.unlink()
            if entry['existed']:
                saved=backup/entry['backup']
                tmp=write_file(dest,saved.read_bytes(),entry['mode']);os.chown(tmp,entry['uid'],entry['gid']);os.replace(tmp,dest)
            else:
                with contextlib.suppress(FileNotFoundError):dest.unlink()
            fsync_dir(dest.parent)
        data['phase']='restored';self.save(data)
    def run(self,changes,tag):
        """changes: [(source_path, dest_path, mode)]. Replace all or none."""
        tx=time.strftime('%Y%m%dT%H%M%S')+f'-{os.getpid()}'
        backup=self.state/'install-tx'/tx;self.safe_dir(self.state/'install-tx');self.safe_dir(backup)
        entries=[]
        for i,(source,dest,mode)in enumerate(changes):
            existed=self.dest_ok(dest);entry={'dest':str(dest),'existed':existed,'backup':f'{i}-{dest.name}','mode':mode,'uid':self.uid,'gid':os.getegid()}
            if existed:
                info=dest.lstat();entry.update(mode=stat.S_IMODE(info.st_mode),uid=info.st_uid,gid=info.st_gid)
                shutil.copyfile(dest,backup/entry['backup'],follow_symlinks=False)
            entries.append(entry)
        fsync_dir(backup)
        data={'tx':tx,'release':tag,'phase':'prepared','backup':str(backup),'entries':entries};self.save(data)
        try:
            temps=[]
            for source,dest,mode in changes:
                dest.parent.mkdir(parents=True,exist_ok=True,mode=0o755)
                temps.append(write_file(dest,pathlib.Path(source).read_bytes(),mode))
            data['phase']='committing';self.save(data)
            for temp,(_,dest,_) in zip(temps,changes):os.replace(temp,dest);fsync_dir(dest.parent)
            data['phase']='committed';self.save(data)
        except BaseException:
            print('# Install failed; restoring previous files',file=sys.stderr,flush=True)
            self.restore(data);raise
        return data
    def finish(self,data):
        """Only after reload, restart/health check and publication: the whole install succeeded."""
        data['phase']='done';self.save(data)
        old=sorted(p for p in (self.state/'install-tx').iterdir()if p.is_dir()and p.name!=data['tx'])
        for p in old[:-1]:shutil.rmtree(p)  # keep the previous generation's backups
# ---------------------------------------------------------------- installer
class Installer:
    def __init__(self,scope,apply=False,release_id=None,profile=None):
        self.scope=scope;self.apply=apply;self.release_id=release_id;self.requested_profile=profile
    def say(self,*cmd):print('+ '+shlex.join(map(str,cmd)),flush=True)
    def systemctl(self,*args,check=True,quiet=False):
        cmd=[SYSTEMCTL]+(['--user']if self.scope=='user'else [])+list(args)
        if not quiet:self.say(*cmd)
        if not self.apply:return subprocess.CompletedProcess(cmd,0,'','')
        return subprocess.run(cmd,check=check,text=True,capture_output=quiet)
    def probe(self,*args):
        cmd=[SYSTEMCTL]+(['--user']if self.scope=='user'else [])+list(args)
        return subprocess.run(cmd,text=True,capture_output=True).stdout.strip()
    def plan_changes(self,release,files,base):
        changes=[]
        for rel,dest,mode in files:
            dest=base/dest;source=release/rel
            try:
                info=dest.lstat()
                same=stat.S_ISREG(info.st_mode)and stat.S_IMODE(info.st_mode)==mode and info.st_uid==os.geteuid()and sha(dest)==sha(source)
            except FileNotFoundError:same=False
            except PermissionError:same=False
            if not same:changes.append((source,dest,mode))
        return changes
    def cutover_phase(self):
        try:return json.loads((STATE_ROOT/'state.json').read_text()).get('phase','legacy')
        except FileNotFoundError:return 'legacy'
        except PermissionError:return 'unknown (root-only journal)'
    def execute(self):
        if self.requested_profile is not None:
            release=ROOT if self.apply and self.scope=='root'else candidate()
            profile,_=paths.deployment_options(json.loads((release/'release.json').read_text()))
            if profile!=self.requested_profile:raise RuntimeError('requested profile differs from release manifest')
        return self.root()if self.scope=='root'else self.user()
    def root(self):
        release=ROOT if self.apply else candidate()
        if (release/'release.json').is_file():
            profile,_=paths.deployment_options(json.loads((release/'release.json').read_text()))
            if profile=='laptop':return self.cpu_root(release)
        if not self.apply:return self.root_plan()
        release=ROOT
        rid,_=require_root_context(release,self.release_id)
        tx=Transaction(STATE_ROOT,0)
        with orchestration_lock(),tx.locked():
            if tx.recover():self.systemctl('daemon-reload')
            phase=self.cutover_phase();after=phase=='native'
            # An interrupted switch may already have manually restored legacy
            # fans. Never start/restart a second controller while recovering it.
            changes=self.plan_changes(release,FILES_ROOT,pathlib.Path('/'))
            active=self.probe('is-active',HELPER)=='active'
            enabled=self.probe('is-enabled',HELPER)
            print(f'# Release {rid}; cutover phase {phase}; helper {enabled}/{"active"if active else"inactive"}; {len(changes)} file(s) to replace',flush=True)
            if active and not after:raise RuntimeError('helper is running outside a journalled cutover; refusing')
            if not after and not enabled.startswith('masked'):
                # Before cutover the helper stays masked so D-Bus activation cannot start it beside legacy cooling.
                self.systemctl('mask',HELPER)
            if changes:
                originals=STATE_ROOT/'install-backup';tx.safe_dir(originals)
                for _,dest,_ in changes:
                    first=originals/dest.name
                    if tx.dest_ok(dest)and not first.exists():shutil.copyfile(dest,first,follow_symlinks=False)
                data=tx.run(changes,rid)
                try:
                    self.systemctl('daemon-reload')
                    if after and enabled.startswith('enabled'):
                        # Cutover owns cooling: bring the helper back on the new files, or restore the old ones.
                        self.systemctl('restart',HELPER);self.systemctl('is-active','--quiet',HELPER)
                except (subprocess.CalledProcessError,OSError):
                    print('# Helper did not come back on the new release; restoring previous files',file=sys.stderr,flush=True)
                    tx.restore(data);self.systemctl('daemon-reload')
                    if after and enabled.startswith('enabled'):self.systemctl('restart',HELPER,check=False)
                    raise
                self.publish(rid,release);tx.finish(data)
            else:
                if after and enabled.startswith('enabled')and not active:self.systemctl('start',HELPER)
                self.publish(rid,release)
        print('# Done. Helper '+('unchanged state (cutover owns it).'if after else 'remains masked until the reviewed root switch.'),flush=True)
        return 0
    def cpu_root(self,release):
        """Optional read-only laptop install; never enters desktop cooling handoff."""
        metadata=json.loads((release/'release.json').read_text())
        profile,enabled=paths.deployment_options(metadata)
        if profile!='laptop'or not enabled:
            if self.apply:raise RuntimeError('user-only laptop release has no root install set')
            print('# Laptop user-only release: no root files, commands, units or policy changes.')
            return 0
        rid,rows=verify_release(release,self.release_id)
        if not self.apply:
            target=RELEASES/rid[:16]
            print('# PLAN ONLY: optional CPU telemetry; no cooling/eGPU, enable/start, or live actions.')
            print(f'# Stage root-owned release at {target}; verify ownership, MANIFEST and full id {rid}.')
            print(f'# Then run /usr/bin/python3 -I -B {target}/packaging/install.py root --release {rid} --apply as root.')
            for src,dest,mode in FILES_CPU:print(f'# {dest} ({mode:04o}) <- {src}')
            print(f'# Journal/backups: {CPU_STATE}; public install record: {CPU_RECORD}; systemctl daemon-reload only (restart if already active).')
            return 0
        require_root_context(release,self.release_id)
        if DESKTOP_POLICY.exists():raise RuntimeError('desktop multi-action helper policy installed; refuse mixed profile root install')
        for base in CPU_UNIT_DIRS:
            override=base/CPU_HELPER
            if override.exists()or override.is_symlink()or any((base/(CPU_HELPER+'.d')).glob('*.conf')):raise RuntimeError('CPU-only unit administrator override requires review: '+str(override))
        tx=Transaction(CPU_STATE,0)
        with orchestration_lock(CPU_STATE),tx.locked():
            if tx.recover():self.systemctl('daemon-reload')
            active=self.probe('is-active',CPU_HELPER)=='active'
            changes=self.plan_changes(release,FILES_CPU,pathlib.Path('/'))
            if changes:
                data=tx.run(changes,rid)
                try:
                    for source,dest,_ in FILES_CPU:
                        if sha(dest)!=rows[source]:raise RuntimeError('installed CPU-only digest mismatch: '+dest)
                    self.systemctl('daemon-reload')
                    if active:self.systemctl('restart',CPU_HELPER);self.systemctl('is-active','--quiet',CPU_HELPER)
                    CPU_RECORD.parent.mkdir(parents=True,exist_ok=True,mode=0o755)
                    atomic_json(CPU_RECORD,{'id':rid,'workspace':metadata['workspace'],'owner':metadata['owner'],
                        'files':{dest:sha(dest)for _,dest,_ in FILES_CPU}},0o644)
                except BaseException:
                    tx.restore(data);self.systemctl('daemon-reload')
                    if active:self.systemctl('restart',CPU_HELPER,check=False)
                    raise
                tx.finish(data)
            else:
                # Recreate an absent publication record after an interrupted final
                # publication, without enabling or starting an idle service.
                CPU_RECORD.parent.mkdir(parents=True,exist_ok=True,mode=0o755)
                atomic_json(CPU_RECORD,{'id':rid,'workspace':metadata['workspace'],'owner':metadata['owner'],
                    'files':{dest:sha(dest)for _,dest,_ in FILES_CPU}},0o644)
        return 0
    def publish(self,rid,release):
        """Public, root-owned record of the installed release (read by preflight)."""
        metadata=json.loads((release/'release.json').read_text());paths.validate_metadata(metadata)
        record={'id':rid,'dir':release.name,'workspace':metadata['workspace'],'owner':metadata['owner'],'files':{dest:sha(dest)for _,dest,_ in FILES_ROOT}}
        try:current=json.loads(INSTALLED.read_text())
        except FileNotFoundError:current=None
        if current!=record:atomic_json(INSTALLED,record,0o644)
        link=CURRENT
        if not(link.is_symlink()and os.readlink(link)==f'releases/{release.name}'):
            temp=link.with_name('.current.hyprv-new')
            with contextlib.suppress(FileNotFoundError):temp.unlink()
            os.symlink(f'releases/{release.name}',temp);os.replace(temp,link);fsync_dir(link.parent)
    def root_plan(self):
        rel=candidate()
        try:rid,_=verify_release(rel)
        except (OSError,RuntimeError)as e:rid='<release-id>';print('# No verified candidate release yet ('+str(e)+'); run packaging/build-release.sh first.')
        short=rid[:16]if len(rid)==64 else '<id16>'
        target=RELEASES/short
        print('# PLAN ONLY (no root, no writes). The root install never runs files from this user-writable tree.')
        print('# 1. Stage a root-owned copy using system tools only:')
        print(f'sudo /usr/bin/install -d -o root -g root -m 0755 {RELEASES}')
        print(f'sudo /usr/bin/cp -R -P -T -- {shlex.quote(str(rel))} {target}   # target must not exist yet')
        print('# 2. Verify the staged copy with system tools BEFORE executing anything from it.')
        print(f'#    The first line must print exactly the release id build-release printed: {rid}')
        print(f'/usr/bin/sha256sum {target}/{MANIFEST}')
        print(f"/usr/bin/sh -c 'cd {target} && /usr/bin/sha256sum --strict --quiet -c {MANIFEST}'")
        print('# 3. Install from the staged, root-owned copy (re-verifies tree ownership and every digest):')
        print(f'sudo /usr/bin/python3 -I -B {target}/packaging/install.py root --release {rid} --apply')
        print(f'# Install effects: journalled atomic replace of {len(FILES_ROOT)} files (restored on any failure or interruption):')
        for src,dest,mode in FILES_ROOT:print(f'#   {dest} ({mode:04o}) <- {src}')
        print(f'#   + {STATE_ROOT}/install.json, install-tx/<tx>/ backups, install-backup/ (first-ever originals, never overwritten)')
        print(f'#   + {INSTALLED} and {CURRENT} -> releases/<id16>; systemctl daemon-reload')
        print('#   Before cutover or phase=switching: keep helper masked/inactive; recover interrupted handoff before switching again. Only phase=native may restart the helper on new files (with file rollback on failure).')
        print('# Current cutover phase: '+self.cutover_phase())
        return 0
    def user(self):
        release=candidate();home=pathlib.Path.home()
        if not self.apply:
            print('# PLAN ONLY. Verify packaging/dist/release against its manifest, then replace atomically (journalled, restored on failure):')
            try:
                verify_release(release,self.release_id);files=user_files(release)
            except (OSError,RuntimeError,ValueError):
                files=FILES_USER
                print('# Runtime assets and catalog will also install under ~/.local/share/hyprv-shell; build a verified release to list each file.')
            for src,dest,mode in files:print(f'#   ~/{dest} ({mode:04o}) <- release/{src}')
            self.say(SYSTEMCTL,'--user','daemon-reload');print('# No enable/start, root command or cutover marker.')
            return 0
        if os.geteuid()==0:raise RuntimeError('user install must run as the desktop user')
        rid,rows=verify_release(release,self.release_id)
        files=user_files(release)
        tx=Transaction(home/'.local/state/hyprv-cutover',os.geteuid())
        with tx.locked():
            if tx.recover():self.systemctl('daemon-reload')
            changes=self.plan_changes(release,files,home)
            print(f'# Release {rid}; {len(changes)} file(s) to replace',flush=True)
            if changes:
                data=tx.run(changes,rid)
                try:
                    for source,dest,_ in files:
                        if sha(home/dest)!=rows[source]:raise RuntimeError('installed digest mismatch: '+dest)
                    self.systemctl('daemon-reload')
                except (subprocess.CalledProcessError,OSError,RuntimeError):tx.restore(data);raise
                tx.finish(data)
                if self.probe('is-active','hyprv-shell.service')=='active':print('# Native shell is running the old binary; it picks up the new one on its next start.')
        return 0
def main():
    p=argparse.ArgumentParser();p.add_argument('scope',choices=['user','root']);g=p.add_mutually_exclusive_group();g.add_argument('--apply',action='store_true');g.add_argument('--dry-run',action='store_true');p.add_argument('--profile',choices=['desktop','laptop']);p.add_argument('--release',help='full release id (sha256 of MANIFEST.sha256); required for root --apply');a=p.parse_args()
    if a.apply and a.scope=='root'and not a.release:p.error('root --apply requires --release <id printed by build-release>')
    try:return Installer(a.scope,a.apply,a.release,a.profile).execute()
    except (OSError,ValueError,RuntimeError,subprocess.CalledProcessError)as e:print('install: '+str(e),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
