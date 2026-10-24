"""Regressions for the round-6 review findings CUT-1..7 (docs/reviews/packaging-cutover.md).

Everything is mocked or runs in temporary directories: no systemctl, sudo,
hyprctl, real hypr file, /usr, /etc, /var or /sys is touched.
"""
import contextlib, hashlib, importlib.util, io, json, os, pathlib, shutil, subprocess, tempfile, unittest
from types import SimpleNamespace
from unittest import mock
ROOT=pathlib.Path(__file__).resolve().parents[2]
HYPR=ROOT/'cutover/tests/fixtures/legacy/hypr'
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
c=load('fix_control','cutover/control.py');i=load('fix_install','packaging/install.py')
def quiet():return contextlib.redirect_stdout(io.StringIO())
def sha(b):return hashlib.sha256(b).hexdigest()
REAL_RUN=subprocess.run

class FakeSystemd:
    """systemctl model; unit-file state comes from symlinks under `dirs` when given."""
    def __init__(self,units,dirs=None):
        self.units={k:dict(v)for k,v in units.items()};self.log=[];self.dirs=dirs
    def unit(self,name):return self.units.setdefault(name,{'load':'not-found','enabled':'','active':False})
    def links(self,name):
        return [p for d in self.dirs for p in [*d.glob('*'),*d.glob('*/*')]if p.is_symlink()and(p.name==name or pathlib.Path(os.readlink(p)).name==name)]
    def state(self,name):
        if self.dirs is None:return self.unit(name)['enabled']
        ls=self.links(name)
        if any(p.parent.name.endswith('.wants')for p in ls):return 'enabled'
        return 'linked'if ls else 'disabled'
    def run(self,cmd,check=False,**kw):
        cmd=[str(x)for x in cmd];self.log.append(cmd);rc,out=0,''
        at=next((n for n,x in enumerate(cmd)if x.endswith('systemctl')),None)
        if at is not None:
            args=cmd[at+1:]
            if args and args[0]=='--user':args=args[1:]
            verb,rest=args[0],args[1:];names=[a for a in rest if not a.startswith('-')]
            u=self.unit(names[-1])if names else None
            if verb=='show':out={'LoadState':u['load'],'Id':names[-1],'ActiveState':'active'if u['active']else'inactive','MainPID':'0'}[rest[0].split('=',1)[1]]
            elif verb=='is-enabled':out=self.state(names[0]);rc=0 if out.startswith('enabled')else 1
            elif verb=='is-active':
                rc=0 if u['active']else 3
                if '--quiet'not in rest:out='active'if u['active']else'inactive'
            elif verb=='mask':u['enabled']='masked'
            elif verb=='unmask':u['enabled']='disabled'
            elif verb in ('enable','start','restart'):
                if u['enabled']=='masked'or u['load']=='not-found':rc=1
                else:
                    if verb=='enable':u['enabled']='enabled'
                    if verb in ('start','restart')or '--now'in rest:u['active']=True
            elif verb=='disable':
                if self.dirs is not None:
                    for p in self.links(names[0]):p.unlink()
                elif u['enabled']in ('enabled','enabled-runtime'):u['enabled']='disabled'
                if '--now'in rest:u['active']=False
            elif verb=='stop':u['active']=False
        if check and rc:raise subprocess.CalledProcessError(rc,cmd)
        return subprocess.CompletedProcess(cmd,rc,out,'')

def journal_with(data):
    @contextlib.contextmanager
    def journal(_):yield data,(lambda:None)
    return journal

def make_release(directory,files):
    directory.mkdir(parents=True,exist_ok=True)
    files=dict(files)
    files.setdefault('release.json',json.dumps({'schema':1,'workspace':str(ROOT),'owner':__import__('pwd').getpwuid(os.getuid()).pw_name,'sources_sha256':'a'*64}).encode())
    for rel,data in files.items():
        p=directory/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
    rows=sorted(str(p.relative_to(directory))for p in directory.rglob('*')if p.is_file())
    (directory/i.MANIFEST).write_text(''.join(f'{sha((directory/r).read_bytes())}  {r}\n'for r in rows))
    return sha((directory/i.MANIFEST).read_bytes())

class RootInstallSandbox:
    """Root installer against temp destinations, state dir and public record."""
    def __init__(self,tmp,phase=None,helper=None):
        self.tmp=pathlib.Path(tmp);self.release=self.tmp/'rel'
        self.files={src:f'{src} v1'.encode()for src,_,_ in i.FILES_ROOT}
        self.rid=make_release(self.release,self.files)
        self.dests=[(src,self.tmp/'dest'/pathlib.Path(dest).name,mode)for src,dest,mode in i.FILES_ROOT]
        self.state=self.tmp/'state';self.state.mkdir(mode=0o700)
        if phase:(self.state/'state.json').write_text(json.dumps({'phase':phase}))
        self.sd=FakeSystemd({i.HELPER:helper or {'load':'loaded','enabled':'disabled','active':False}})
    def install(self,fail=None):
        stack=contextlib.ExitStack()
        stack.enter_context(mock.patch.object(i,'ROOT',self.release))
        stack.enter_context(mock.patch.object(i,'FILES_ROOT',[(s,str(d),m)for s,d,m in self.dests]))
        stack.enter_context(mock.patch.object(i,'STATE_ROOT',self.state))
        stack.enter_context(mock.patch.object(i,'INSTALLED',self.tmp/'installed.json'))
        stack.enter_context(mock.patch.object(i,'CURRENT',self.tmp/'current'))
        stack.enter_context(mock.patch.object(i,'require_root_context',lambda rel,rid:(self.rid,{})))
        stack.enter_context(mock.patch.object(i.Transaction,'safe_dir',lambda self_,d,mode=0o700:d.mkdir(parents=True,exist_ok=True,mode=mode)))
        stack.enter_context(mock.patch.object(i.os,'chown'))
        stack.enter_context(mock.patch.object(i.subprocess,'run',self.sd.run))
        if fail:stack.enter_context(mock.patch.object(i.os,'replace',fail))
        with stack,quiet():return i.Installer('root',apply=True,release_id=self.rid).execute()
    def contents(self):return {d.name:(d.read_bytes()if d.exists()else None)for _,d,_ in self.dests}

class Cut1NoMaskAfterCutover(unittest.TestCase):
    def test_reinstall_after_cutover_never_masks_and_restarts_helper(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp,'native',{'load':'loaded','enabled':'enabled','active':False})
            self.assertEqual(box.install(),0)
            helper=box.sd.units[i.HELPER]
            self.assertEqual((helper['enabled'],helper['active']),('enabled',True))
            self.assertFalse(any('mask'in cmd for cmd in box.sd.log))
    def test_running_helper_is_upgraded_in_place_after_cutover(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp,'native',{'load':'loaded','enabled':'enabled','active':True})
            self.assertEqual(box.install(),0)
            self.assertTrue(any(cmd[-2:]==['restart',i.HELPER]for cmd in box.sd.log))
            self.assertEqual(box.sd.units[i.HELPER]['enabled'],'enabled')
    def test_native_restart_or_health_failure_restores_files_and_old_helper(self):
        for failure in ('restart','is-active'):
            with self.subTest(failure=failure),tempfile.TemporaryDirectory()as tmp:
                box=RootInstallSandbox(tmp,'native',{'load':'loaded','enabled':'enabled','active':True})
                for _,dest,_ in box.dests:
                    dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(b'old '+dest.name.encode())
                before=box.contents();real=box.sd.run;failed=[False]
                def run(cmd,**kw):
                    if not failed[0] and failure in cmd and (failure!='is-active' or '--quiet'in cmd):
                        failed[0]=True;box.sd.log.append(list(cmd));raise subprocess.CalledProcessError(1,cmd)
                    return real(cmd,**kw)
                box.sd.run=run
                with self.assertRaises(subprocess.CalledProcessError):box.install()
                self.assertTrue(failed[0]);self.assertEqual(box.contents(),before)
                self.assertEqual(json.loads((box.state/'install.json').read_text())['phase'],'restored')
                self.assertFalse((box.tmp/'installed.json').exists())
                self.assertTrue(box.sd.units[i.HELPER]['active'])
                self.assertEqual(box.sd.log[-2:],[[i.SYSTEMCTL,'daemon-reload'],[i.SYSTEMCTL,'restart',i.HELPER]])

    def test_first_install_masks_and_rerun_is_idempotent(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp)
            box.install();first=box.contents()
            self.assertEqual(box.sd.units[i.HELPER]['enabled'],'masked')
            box.sd.log.clear();box.install()
            self.assertEqual(box.contents(),first)
            self.assertEqual([cmd for cmd in box.sd.log if 'is-'not in ' '.join(cmd)],[],'second run must change nothing')
    def test_root_switch_when_native_repairs_a_masked_helper(self):
        sd=FakeSystemd({c.HELPER:{'load':'loaded','enabled':'masked','active':False}})
        with mock.patch.object(c.subprocess,'run',sd.run),mock.patch.object(c.os,'geteuid',return_value=0),mock.patch.object(c,'verify_staged'),\
             mock.patch.object(c,'journal',journal_with({'phase':'native','units':{},'attributes':[]})),mock.patch.object(c,'qs_running',return_value=False),quiet():
            self.assertEqual(c.root_handoff('switch'),0)
        self.assertEqual((sd.units[c.HELPER]['enabled'],sd.units[c.HELPER]['active']),('enabled',True))

class Cut2RootTrust(unittest.TestCase):
    def test_root_install_refuses_the_user_tree(self):
        with mock.patch.object(i.os,'geteuid',return_value=0),mock.patch.object(i.sys,'flags',SimpleNamespace(isolated=1)):
            with self.assertRaisesRegex(RuntimeError,'staged release'):i.require_root_context(ROOT,'0'*64)
        with mock.patch.object(i.os,'geteuid',return_value=0),mock.patch.object(i.sys,'flags',SimpleNamespace(isolated=0)):
            with self.assertRaisesRegex(RuntimeError,'-I'):i.require_root_context(i.RELEASES/'x','0'*64)
    def test_verify_release_rejects_user_owned_tampered_or_unlisted_files(self):
        with tempfile.TemporaryDirectory()as tmp:
            rel=pathlib.Path(tmp)/'r';rid=make_release(rel,{'bin/hyprv-privhelper':b'good','packaging/install.py':b'x'})
            self.assertEqual(i.verify_release(rel,rid)[0],rid)
            with self.assertRaisesRegex(RuntimeError,'untrusted'):i.verify_release(rel,rid,root_owned=True)
            with self.assertRaisesRegex(RuntimeError,'release id mismatch'):i.verify_release(rel,'f'*64)
            (rel/'bin/hyprv-privhelper').write_bytes(b'payload')
            with self.assertRaisesRegex(RuntimeError,'digest mismatch'):i.verify_release(rel,rid)
            (rel/'bin/hyprv-privhelper').write_bytes(b'good');(rel/'extra.py').write_text('evil')
            with self.assertRaisesRegex(RuntimeError,'not in manifest'):i.verify_release(rel,rid)
            (rel/'extra.py').unlink();(rel/'link').symlink_to('/etc/passwd')
            with self.assertRaisesRegex(RuntimeError,'non-regular'):i.verify_release(rel,rid)
    def test_root_control_refuses_the_checkout(self):
        with mock.patch.object(c.sys,'flags',SimpleNamespace(isolated=1)):
            with self.assertRaisesRegex(RuntimeError,'never runs from the user-writable checkout'):c.verify_staged('switch')
    def test_wrappers_refuse_root_and_root_preflight_runs_as_user(self):
        for script in ('packaging/install-root.sh','cutover/switch.sh','cutover/rollback.sh'):self.assertIn('id -u)" = 0',(ROOT/script).read_text())
        self.assertIn('--dry-run',(ROOT/'packaging/install-root.sh').read_text())
        self.assertEqual(c.owner_manager()[:3],[c.RUNUSER,'-u',c.OWNER])

class Cut3UnitStates(unittest.TestCase):
    def test_linked_and_enabled_links_are_restored_exactly(self):
        with tempfile.TemporaryDirectory()as tmp:
            etc=pathlib.Path(tmp)/'etc';run=pathlib.Path(tmp)/'run';(etc/'default.target.wants').mkdir(parents=True);run.mkdir()
            unit='hyprv-fan-control.service'
            (etc/unit).symlink_to('/opt/fan/'+unit);(etc/'default.target.wants'/unit).symlink_to('/opt/fan/'+unit)
            before={str(p):os.readlink(p)for p in etc.rglob('*')if p.is_symlink()}
            sd=FakeSystemd({unit:{'load':'loaded','enabled':'','active':True},'fan-control.service':{'load':'not-found','enabled':'','active':False}},dirs=[etc,run])
            with mock.patch.object(c,'unit_dirs',lambda scope:[etc,run]),mock.patch.object(c.subprocess,'run',sd.run),quiet():
                snap=c.units('user');c.stop_legacy('user',snap)
                self.assertEqual(list(etc.rglob('*.service')),[],'disable removed the links')
                problems=c.restore_units('user',snap)
            self.assertEqual(problems,[])
            self.assertEqual({str(p):os.readlink(p)for p in etc.rglob('*')if p.is_symlink()},before)
            self.assertTrue(sd.units[unit]['active'])
    def test_masked_static_and_disabled_units_are_only_stopped(self):
        sd=FakeSystemd({})
        snap={u:{'enabled':s,'active':True,'links':[]}for u,s in (('a.service','masked'),('b.service','static'),('d.service','disabled'),('e.service','linked-runtime'))}
        with mock.patch.object(c.subprocess,'run',sd.run),quiet():c.stop_legacy('system',snap)
        self.assertEqual([cmd[1:3]for cmd in sd.log],[['stop','a.service'],['stop','b.service'],['stop','d.service'],['disable','--now']])
    def test_state_mismatch_is_reported_not_hidden(self):
        sd=FakeSystemd({'x.service':{'load':'loaded','enabled':'disabled','active':False}})
        with mock.patch.object(c.subprocess,'run',sd.run),quiet():
            problems=c.restore_units('system',{'x.service':{'enabled':'enabled','active':False,'links':[]}})
        self.assertEqual(problems,['x.service: unit-file state disabled, was enabled'])

class Cut4NativeFallback(unittest.TestCase):
    def test_failed_readiness_falls_back_to_quickshell(self):
        data={};actions=[];qs=iter([True,True,False])
        with tempfile.TemporaryDirectory()as tmp:
            state=pathlib.Path(tmp)
            with mock.patch.object(c.os,'geteuid',return_value=1000),mock.patch.object(c,'preflight'),mock.patch.object(c,'USER_STATE',state),\
                 mock.patch.object(c,'patch_state',return_value='legacy'),mock.patch.object(c,'backup_hypr'),\
                 mock.patch.dict(c.os.environ,{'WAYLAND_DISPLAY':'wayland-1','XDG_RUNTIME_DIR':'/run/user/1000'}),mock.patch.object(pathlib.Path,'is_socket',return_value=True),\
                 mock.patch.object(c,'journal',journal_with(data)),mock.patch.object(c,'units',return_value={}),\
                 mock.patch.object(c,'qs_running',side_effect=lambda *a:next(qs,False)),\
                 mock.patch.object(c.subprocess,'run',side_effect=lambda cmd,**k:subprocess.CompletedProcess(cmd,0 if cmd[-1]==c.HELPER else 3)),mock.patch.object(c.time,'sleep'),\
                 mock.patch.object(c,'call',side_effect=lambda cmd,check=True,timeout=None:actions.append([str(x)for x in cmd])or subprocess.CompletedProcess(cmd,0)),\
                 mock.patch.object(c,'git_apply',side_effect=lambda *a,check=False:actions.append(['git-apply',*a])),\
                 mock.patch.object(c,'revert_hypr',return_value=(['hypr patch reversed exactly'],False)),\
                 mock.patch.object(c,'start_quickshell',side_effect=lambda:actions.append(['launch.sh'])or True),\
                 mock.patch.object(c,'wait_native',side_effect=RuntimeError('native shell not ready within 20s: bar surfaces missing on DP-4')),quiet(),contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError,'legacy Quickshell restored'):c.user_switch(False)
                self.assertFalse((state/'native.enabled').exists())
        self.assertEqual(data['phase'],'fallback');self.assertFalse(data['patch_owned'])
        order=[a[:3]for a in actions]
        self.assertLess(order.index([c.SYSTEMCTL,'--user','stop']),order.index(['launch.sh']))
        self.assertIn([c.HYPRCTL,'reload'],actions)

class Cut5DriftTolerantRollback(unittest.TestCase):
    def tree(self,tmp,edit_patched_line=False):
        hyprv=pathlib.Path(tmp)/'HyprV'
        for name in c.patch_files():(hyprv/name).parent.mkdir(parents=True,exist_ok=True);shutil.copy2(HYPR.parent/name,hyprv/name)
        original={p.name:p.read_bytes()for p in (hyprv/'hypr').rglob('*')if p.is_file()}
        subprocess.run([c.GIT,'apply',str(ROOT/'cutover/hypr.patch')],cwd=hyprv,check=True)
        p=hyprv/'hypr/hyprland-exec-once.lua';lines=p.read_text().split('\n')
        n=next(k for k,l in enumerate(lines)if 'hyprv-shell.service'in l)
        lines.insert(n+1,'lib.exec_once("easyeffects --gapplication-service")')
        if edit_patched_line:
            b=hyprv/'hypr/hyprland-bind.lua';b.write_text(b.read_text().replace('"systemctl --user stop hyprv-shell.service"','"systemctl --user stop hyprv-shell.service; notify-send bye"'))
        p.write_text('\n'.join(lines))
        return hyprv,original
    def rollback(self,hyprv,state,quiesce,data):
        def run(cmd,**k):
            if str(cmd[0])==c.GIT:return REAL_RUN(cmd,**k)
            return subprocess.CompletedProcess(cmd,3)
        with mock.patch.object(c,'HYPRV',hyprv),mock.patch.object(c,'USER_STATE',state),mock.patch.object(c.os,'geteuid',return_value=1000),\
             mock.patch.object(c,'journal',journal_with(data)),mock.patch.object(c.subprocess,'run',side_effect=run),\
             mock.patch.object(c,'call',side_effect=lambda cmd,check=True,timeout=None:subprocess.CompletedProcess(cmd,0)),\
             mock.patch.object(c,'restore_units',return_value=[]),mock.patch.object(c,'start_quickshell',return_value=True),quiet(),contextlib.redirect_stderr(io.StringIO()):
            return c.user_rollback(quiesce)
    def test_drift_never_blocks_quiesce_and_full_rollback_reverts_by_content(self):
        with tempfile.TemporaryDirectory()as tmp:
            hyprv,original=self.tree(tmp);state=pathlib.Path(tmp)/'state';state.mkdir()
            data={'phase':'native','units':{},'quickshell':True,'patch_owned':True}
            self.assertEqual(self.rollback(hyprv,state,True,data),0);self.assertEqual(data['phase'],'native-stopped')
            self.assertEqual(self.rollback(hyprv,state,False,data),0);self.assertEqual(data['phase'],'legacy')
            for name in c.patch_files():
                if not name.endswith('exec-once.lua'):self.assertEqual((hyprv/name).read_bytes(),original[pathlib.Path(name).name],name)
                self.assertEqual(oct((hyprv/name).stat().st_mode&0o777),oct((HYPR.parent/name).stat().st_mode&0o777),name)
            text=(hyprv/'hypr/hyprland-exec-once.lua').read_text()
            self.assertIn('quickshell/scripts/launch.sh',text);self.assertIn('easyeffects',text);self.assertNotIn('hyprv-shell.service',text)
    def test_unresolvable_line_is_reported_but_ui_still_restored(self):
        with tempfile.TemporaryDirectory()as tmp:
            hyprv,_=self.tree(tmp,edit_patched_line=True);state=pathlib.Path(tmp)/'state';state.mkdir()
            data={'phase':'native-stopped','units':{},'quickshell':True,'patch_owned':True}
            self.assertEqual(self.rollback(hyprv,state,False,data),2)
            self.assertEqual(data['phase'],'rollback-incomplete')  # R2-3: retryable, not finalized
            self.assertTrue(any('revert by hand'in p and 'notify-send'not in p for p in data['problems']))
            self.assertNotIn('hyprv-shell.service',(hyprv/'hypr/hyprland-exec-once.lua').read_text())

class Cut6AtomicInstall(unittest.TestCase):
    def test_failure_mid_commit_restores_every_file(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp)
            for _,d,_ in box.dests[:4]:d.parent.mkdir(parents=True,exist_ok=True);d.write_bytes(b'old '+d.name.encode())
            before=box.contents();real=os.replace;count=[0]
            def flaky(a,b):
                if str(a).endswith('.hyprv-new')and str(b).startswith(str(box.tmp/'dest')):
                    count[0]+=1
                    if count[0]==4:raise OSError(28,'No space left on device')
                return real(a,b)
            with self.assertRaises(OSError):box.install(fail=flaky)
            self.assertEqual(box.contents(),before)
            self.assertEqual(json.loads((box.state/'install.json').read_text())['phase'],'restored')
            self.assertFalse((box.tmp/'installed.json').exists())
    def test_interrupted_transaction_is_restored_on_next_run(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp);d=box.dests[0][1];d.parent.mkdir(parents=True);d.write_bytes(b'old')
            tx=i.Transaction(box.state,os.geteuid());data=tx.run([(box.release/box.dests[0][0],d,0o755),(box.release/box.dests[1][0],box.dests[1][1],0o755)],'x')
            data['phase']='committing';tx.save(data)  # power loss after the renames
            self.assertTrue(tx.recover())
            self.assertEqual(d.read_bytes(),b'old');self.assertFalse(box.dests[1][1].exists())
    def test_user_install_is_idempotent(self):
        with tempfile.TemporaryDirectory()as tmp:
            home=pathlib.Path(tmp)/'home';rel=pathlib.Path(tmp)/'release'
            make_release(rel,{'bin/hyprv-shell':b'ELF','files/hyprv-shell.service':b'[Unit]',
                'runtime/runtime-assets.json':json.dumps({'schema':1,'files':{'assets/test.svg':sha(b'asset')}}).encode(),
                'runtime/assets/test.svg':b'asset'})
            runs=[]
            with mock.patch.object(i,'candidate',lambda:rel),mock.patch.object(i.pathlib.Path,'home',return_value=home),\
                 mock.patch.object(i.subprocess,'run',side_effect=lambda cmd,**k:runs.append(cmd)or subprocess.CompletedProcess(cmd,3,'','')),quiet():
                i.Installer('user',apply=True).execute();n=len(runs);i.Installer('user',apply=True).execute()
            self.assertEqual((home/'.local/bin/hyprv-shell').read_bytes(),b'ELF')
            self.assertEqual([r for r in runs[n:]if 'daemon-reload'in r],[],'no-op rerun reloads nothing')

class Cut7Inventory(unittest.TestCase):
    def test_every_listed_namespace_has_a_layer_rule(self):
        rules=(HYPR/'hyprland-winrule.lua').read_text()
        for ns in json.loads((ROOT/'cutover/hypr-inputs.json').read_text())['namespaces']:self.assertIn(ns.split(':')[0].rstrip('.*'),rules)

class ReleaseAssembly(unittest.TestCase):
    def test_build_assembles_a_manifest_the_installer_verifies(self):
        b=load('fix_build','packaging/build_release.py')
        with tempfile.TemporaryDirectory()as tmp:
            root=pathlib.Path(tmp)
            for rel,src in b.RELEASE_FILES.items():
                path=root/src;path.parent.mkdir(parents=True,exist_ok=True)
                if not path.exists():path.write_bytes(('content of '+src).encode())
            shutil.copytree(ROOT/'packaging/assets',root/'packaging/assets')
            catalog=root/'crates/hyprv-shell/src/runtime-assets.json';catalog.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(ROOT/'crates/hyprv-shell/src/runtime-assets.json',catalog)
            with mock.patch.object(b,'ROOT',root),mock.patch.dict(os.environ,HYPRV_DEPLOY_WORKSPACE=str(ROOT)):rid=b.assemble(root/'packaging/dist',{'a':'b'})
            release=root/'packaging/dist/release'
            self.assertEqual(i.verify_release(release,rid)[0],rid)
            names={str(p.relative_to(release))for p in release.rglob('*')if p.is_file()}
            self.assertTrue({'packaging/install.py','cutover/control.py','release.json',*(r for r,_,_ in i.FILES_ROOT),*(r for r,_,_ in i.FILES_USER)}<=names)
            self.assertEqual(json.loads((release/'release.json').read_text())['workspace'],str(ROOT))

class Cut8RehearsalFindings(unittest.TestCase):
    def test_hung_ipc_ping_cannot_outlive_the_readiness_deadline(self):
        def run(cmd,**k):
            self.assertIn('timeout',k);self.assertLessEqual(k['timeout'],5.)
            raise subprocess.TimeoutExpired(cmd,k['timeout'])
        clock=iter([0.,0.,30.,30.])
        with mock.patch.object(c.subprocess,'run',side_effect=run),mock.patch.object(c.time,'monotonic',side_effect=lambda:next(clock)),mock.patch.object(c.time,'sleep'),quiet():
            with self.assertRaisesRegex(RuntimeError,'ping timed out'):c.wait_native(timeout=20)

class CutR2Fixes(unittest.TestCase):
    """docs/reviews/packaging-cutover-r2.md CUT-R2-1..6."""
    def test_r2_6_install_and_root_handoff_share_one_lock(self):
        import threading,time as t
        with tempfile.TemporaryDirectory()as tmp:
            state=pathlib.Path(tmp)/'state';order=[];held=threading.Event()
            def installer():
                with i.orchestration_lock(state):order.append('install-in');held.set();t.sleep(.3);order.append('install-out')
            def handoff():
                held.wait()
                with c.journal(state):order.append('handoff-in')
            a,b=threading.Thread(target=installer),threading.Thread(target=handoff);a.start();b.start();a.join();b.join()
        self.assertEqual(order,['install-in','install-out','handoff-in'])
    def test_r2_6_root_install_holds_the_lock_for_its_whole_run(self):
        events=[]
        @contextlib.contextmanager
        def lock(state=None):
            events.append('lock');yield;events.append('unlock')
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp,'native',{'load':'loaded','enabled':'enabled','active':True})
            real=box.sd.run
            def run(cmd,*a,**k):events.append(' '.join(map(str,cmd[1:])));return real(cmd,*a,**k)
            box.sd.run=run
            with mock.patch.object(i,'orchestration_lock',lock):box.install()
        self.assertEqual((events[0],events[-1]),('lock','unlock'))
        self.assertTrue(any('restart'in e for e in events[1:-1]))
    def test_r2_6_root_rollback_never_restores_legacy_while_helper_runs(self):
        class Stubborn(FakeSystemd):
            def run(self,cmd,check=False,**kw):
                r=super().run(cmd,check,**kw)
                if str(cmd[-1])==c.HELPER:self.unit(c.HELPER)['active']=True  # a concurrent restart keeps it alive
                return r
        sd=Stubborn({c.HELPER:{'load':'loaded','enabled':'enabled','active':True}})
        data={'phase':'native','units':{'hyprv-fan-control.service':{'enabled':'enabled','active':True,'links':[]}},'attributes':[]}
        with mock.patch.object(c.subprocess,'run',sd.run),mock.patch.object(c.os,'geteuid',return_value=0),mock.patch.object(c,'verify_staged'),\
             mock.patch.object(c,'native_running',return_value=False),mock.patch.object(c,'attributes',return_value=[]),\
             mock.patch.object(c,'set_permissions')as perms,mock.patch.object(c,'restore_units')as restore,mock.patch.object(c,'journal',journal_with(data)),quiet():
            with self.assertRaisesRegex(RuntimeError,'helper still active'):c.root_handoff('rollback')
        perms.assert_not_called();restore.assert_not_called()
        self.assertEqual(data['phase'],'rollback-incomplete')
        self.assertIn(['/usr/bin/systemctl','mask','--now',c.HELPER],sd.log)
    def test_r2_1_install_killed_after_commit_is_completed_on_retry(self):
        class Killed(BaseException):pass
        with tempfile.TemporaryDirectory()as tmp:
            box=RootInstallSandbox(tmp,'native',{'load':'loaded','enabled':'enabled','active':True})
            real=box.sd.run;armed=[True]
            def run(cmd,*a,**k):
                if armed[0]and 'daemon-reload'in cmd:armed[0]=False;raise Killed()
                return real(cmd,*a,**k)
            box.sd.run=run
            with self.assertRaises(Killed):box.install()
            self.assertEqual(json.loads((box.state/'install.json').read_text())['phase'],'committed')
            self.assertFalse((box.tmp/'installed.json').exists())
            box.sd.log.clear();self.assertEqual(box.install(),0)
            verbs=[' '.join(cmd[1:])for cmd in box.sd.log]
            self.assertIn('daemon-reload',verbs);self.assertIn('restart '+i.HELPER,verbs)
            self.assertEqual(json.loads((box.state/'install.json').read_text())['phase'],'done')
            self.assertEqual(json.loads((box.tmp/'installed.json').read_text())['id'],box.rid)
    def test_r2_2_no_legacy_ui_while_native_survives_stop(self):
        data={'patch_owned':True};launched=[]
        with tempfile.TemporaryDirectory()as tmp,mock.patch.object(c,'USER_STATE',pathlib.Path(tmp)),\
             mock.patch.object(c,'call',return_value=subprocess.CompletedProcess([],1)),mock.patch.object(c,'native_active',return_value=True),\
             mock.patch.object(c.time,'sleep'),mock.patch.object(c.time,'monotonic',side_effect=iter(range(0,10000,10))),\
             mock.patch.object(c,'revert_hypr')as revert,mock.patch.object(c,'start_quickshell',side_effect=lambda:launched.append(1)or True),quiet(),contextlib.redirect_stderr(io.StringIO()):
            self.assertFalse(c.fallback(data,lambda:None,'boom'))
        self.assertEqual(launched,[]);revert.assert_not_called()
        self.assertEqual(data['phase'],'fallback-blocked');self.assertTrue(data['patch_owned'])
    def test_r2_3_incomplete_rollback_stays_retryable_until_resolved(self):
        data={'phase':'native-stopped','units':{'x.service':{'enabled':'enabled','active':True,'links':[]}},'patch_owned':True}
        results=iter([(['hypr: revert by hand: X'],True),(['hypr files already in legacy state'],False)])
        restores=iter([['x.service: start failed'],[]]);calls=[]
        def common():
            return contextlib.ExitStack()
        with tempfile.TemporaryDirectory()as tmp,mock.patch.object(c,'USER_STATE',pathlib.Path(tmp)),mock.patch.object(c.os,'geteuid',return_value=1000),\
             mock.patch.object(c,'journal',journal_with(data)),mock.patch.object(c,'stop_native',return_value=True),\
             mock.patch.object(c,'call',return_value=subprocess.CompletedProcess([],0)),mock.patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],3)),\
             mock.patch.object(c,'revert_hypr',side_effect=lambda d:calls.append('revert')or next(results)),\
             mock.patch.object(c,'restore_units',side_effect=lambda *a:calls.append('restore')or next(restores)),\
             mock.patch.object(c,'start_quickshell',return_value=True),quiet(),contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(c.user_rollback(False),2);self.assertEqual(data['phase'],'rollback-incomplete')
            self.assertEqual(c.user_rollback(False),0);self.assertEqual(data['phase'],'legacy')
            self.assertEqual(c.user_rollback(False),0)
        self.assertEqual(calls,['revert','restore','revert','restore'])
    def test_r2_3_root_rollback_with_problems_is_not_final(self):
        sd=FakeSystemd({c.HELPER:{'load':'loaded','enabled':'enabled','active':True}})
        data={'phase':'native','units':{},'attributes':[]}
        with mock.patch.object(c.subprocess,'run',sd.run),mock.patch.object(c.os,'geteuid',return_value=0),mock.patch.object(c,'verify_staged'),\
             mock.patch.object(c,'native_running',return_value=False),mock.patch.object(c,'attributes',return_value=[]),mock.patch.object(c,'set_permissions'),\
             mock.patch.object(c,'restore_units',return_value=['hyprv-fan-control.service: start failed']),mock.patch.object(c,'qs_running',return_value=False),mock.patch.object(c,'journal',journal_with(data)),quiet(),contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(c.root_handoff('rollback'),2)
            self.assertEqual(data['phase'],'rollback-incomplete')
            with self.assertRaisesRegex(RuntimeError,'incomplete'):c.root_handoff('switch')
    def test_r2_4_restoration_errors_never_block_the_legacy_ui(self):
        snap={'u.service':{'enabled':'enabled','active':True,'links':[{'path':'/nonexistent/x.wants/u.service','target':'/t'}]}}
        def call(cmd,check=True,timeout=None):
            if 'daemon-reload'in cmd:raise subprocess.CalledProcessError(1,cmd)
            return subprocess.CompletedProcess(cmd,0)
        with mock.patch.object(c,'call',side_effect=call),mock.patch.object(c.os,'symlink',side_effect=OSError(28,'No space left on device')),\
             mock.patch.object(c.pathlib.Path,'mkdir'),mock.patch.object(c,'quiet',return_value=subprocess.CompletedProcess([],0,'enabled','')),quiet():
            problems=c.restore_units('user',snap)
        self.assertTrue(any('No space left'in p for p in problems));self.assertTrue(any('daemon-reload'in p for p in problems))
        data={'phase':'native-stopped','units':{},'patch_owned':False};launched=[]
        with tempfile.TemporaryDirectory()as tmp,mock.patch.object(c,'USER_STATE',pathlib.Path(tmp)),mock.patch.object(c.os,'geteuid',return_value=1000),\
             mock.patch.object(c,'journal',journal_with(data)),mock.patch.object(c,'stop_native',return_value=True),\
             mock.patch.object(c,'call',return_value=subprocess.CompletedProcess([],0)),mock.patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],3)),\
             mock.patch.object(c,'restore_units',side_effect=OSError(5,'I/O error')),\
             mock.patch.object(c,'start_quickshell',side_effect=lambda:launched.append(1)or True),quiet(),contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(c.user_rollback(False),2)
        self.assertEqual(launched,[1]);self.assertEqual(data['phase'],'rollback-incomplete')
    def test_r2_5_every_surface_query_is_bounded_by_the_deadline(self):
        now=[0.]
        def run(cmd,**k):
            if 'ipc'in cmd:return subprocess.CompletedProcess(cmd,0,'{}','')
            self.assertIsNotNone(k.get('timeout'),cmd);self.assertLessEqual(k['timeout'],20.)
            now[0]+=k['timeout'];raise subprocess.TimeoutExpired(cmd,k['timeout'])
        with mock.patch.object(c.subprocess,'run',side_effect=run),mock.patch.object(c.time,'monotonic',side_effect=lambda:now[0]),mock.patch.object(c.time,'sleep'),quiet():
            with self.assertRaisesRegex(RuntimeError,'query timed out'):c.wait_native(timeout=20)
        self.assertLessEqual(now[0],21.)
    def test_r2_5_slow_success_after_the_deadline_is_not_accepted(self):
        now=[0.]
        def surfaces(deadline):now[0]+=30;return True,'bar mapped on X'
        with mock.patch.object(c.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'{}','')),mock.patch.object(c,'native_surfaces',side_effect=surfaces),\
             mock.patch.object(c.time,'monotonic',side_effect=lambda:now[0]),mock.patch.object(c.time,'sleep'),quiet():
            with self.assertRaisesRegex(RuntimeError,'only after the deadline'):c.wait_native(timeout=20)

class Cut9BackupFreshness(unittest.TestCase):
    def test_hypr_backup_is_refreshed_on_every_new_application(self):
        with tempfile.TemporaryDirectory()as tmp:
            tmp=pathlib.Path(tmp);(tmp/'hypr').mkdir()
            names=c.patch_files()
            for n in names:(tmp/n).parent.mkdir(parents=True,exist_ok=True);(tmp/n).write_text('generation 1 '+n)
            with mock.patch.object(c,'HYPRV',tmp),mock.patch.object(c,'USER_STATE',tmp/'state'):
                (tmp/'state').mkdir();c.backup_hypr()
                for n in names:(tmp/n).write_text('generation 2 '+n)
                d=c.backup_hypr()
            for n in names:self.assertEqual((d/n.replace('/','__')).read_text(),'generation 2 '+n)
            self.assertEqual(oct((d/names[0].replace('/','__')).stat().st_mode&0o777),'0o600')

class PatchCoversLegacyScripts(unittest.TestCase):
    def test_patch_ports_the_three_legacy_script_entry_points(self):
        text=(ROOT/'cutover/hypr.patch').read_text()
        removed=[l[1:]for l in text.splitlines()if l.startswith('-')and not l.startswith('---')]
        added=[l[1:]for l in text.splitlines()if l.startswith('+')and not l.startswith('+++')]
        for legacy in ('quickshell/scripts/set-variant.sh','quickshell/scripts/deskflow-target.sh','quickshell/StartupConfirm.qml'):
            self.assertTrue(any(legacy in l for l in removed),legacy)
        self.assertFalse([l for l in added if 'quickshell/' in l or 'qs --path' in l],'added lines still call legacy Quickshell')
        for native in ('"$hyprv_shell" desktop setVariant','"$hyprv_shell" desktop deskflowReflow','hyprv-shell" startup-confirm'):
            self.assertTrue(any(native in l for l in added),native)
        self.assertEqual(sorted(json.loads((ROOT/'cutover/hypr-inputs.json').read_text())['files']),
                         sorted({l.split(' b/')[-1]for l in text.splitlines()if l.startswith('diff --git')}))

class DryRuns(unittest.TestCase):
    def test_plans_touch_nothing(self):
        boom=AssertionError('dry-run executed or wrote something')
        with mock.patch.object(c.subprocess,'run',side_effect=boom),mock.patch.object(c.os,'open',side_effect=boom),mock.patch.object(i.subprocess,'run',side_effect=boom),\
             mock.patch.object(i.os,'open',side_effect=boom),mock.patch.object(i.os,'replace',side_effect=boom),quiet():
            for d in ('switch','rollback'):
                for s in ('user','system'):c.plan(d,s,False,None)
                c.plan(d,'user',True,None)
            i.Installer('root').execute();i.Installer('user').execute()
if __name__=='__main__':unittest.main()
