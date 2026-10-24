"""Laptop release/install/cutover regressions: temp HOME/hypr, mocked service commands."""
import contextlib, hashlib, importlib.util, io, json, os, pathlib, shutil, subprocess, tempfile, unittest
from unittest import mock
ROOT=pathlib.Path(__file__).resolve().parents[2]
def load(name,path):
    s=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
b=load('lap_build','packaging/build_release.py');i=load('lap_install','packaging/install.py');c=load('lap_control','cutover/laptop/control.py');g=load('lap_gate','cutover/laptop/preflight.py')
def sha(data):return hashlib.sha256(data).hexdigest()
def fixture(base,cpu=False):
    tree=base/'source'
    for dest,src in b.release_files('laptop',cpu).items():
        target=tree/src;target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes((ROOT/src).read_bytes()if not dest.startswith('bin/')else b'\x7fELFfixture')
    asset=tree/'asset';asset.write_bytes(b'asset')
    catalog=tree/'catalog';catalog.write_text(json.dumps({'schema':1,'files':{'assets/test.svg':sha(b'asset')}}))
    with mock.patch.object(b,'ROOT',tree),mock.patch.object(b,'runtime_files',return_value={'runtime/runtime-assets.json':'catalog','runtime/assets/test.svg':'asset'}),mock.patch.dict(os.environ,HYPRV_DEPLOY_WORKSPACE=str(ROOT)):
        rid=b.assemble(tree/'packaging/dist',{},'laptop',cpu)
    return tree/'packaging/dist/release',rid
class ReleaseSelection(unittest.TestCase):
    def test_desktop_default_is_unchanged_and_laptop_never_ships_combined_helper(self):
        self.assertEqual(b.binaries_for(),b.BINS);self.assertEqual(b.release_files(),b.RELEASE_FILES)
        with self.assertRaises(ValueError):b.binaries_for('desktop',True)
        for cpu in (False,True):
            selected=b.release_files('laptop',cpu)
            for name in selected:
                self.assertNotIn('PrivHelper',name);self.assertNotIn('failsafe',name);self.assertNotIn('fan-control',name);self.assertNotIn('egpu',name)
            self.assertEqual('bin/hyprv-cpu-power'in selected,cpu)
            self.assertEqual(selected['cutover/hypr.patch'],'cutover/laptop/hypr.patch')
    def test_manifest_options_and_exact_optional_install_set(self):
        for cpu in (False,True):
            with self.subTest(cpu=cpu),tempfile.TemporaryDirectory()as tmp:
                release,rid=fixture(pathlib.Path(tmp),cpu);self.assertEqual(i.verify_release(release)[0],rid)
                meta=json.loads((release/'release.json').read_text());self.assertEqual(i.paths.deployment_options(meta),('laptop',cpu))
                # A hash-valid but profile-inappropriate file still must fail admission.
                f=release/'bin/hyprv-privhelper';f.write_bytes(b'bad')
                with (release/'MANIFEST.sha256').open('a')as manifest:manifest.write(sha(b'bad')+'  bin/hyprv-privhelper\n')
                with self.assertRaisesRegex(RuntimeError,'install set'):i.verify_release(release)
    def test_scratch_home_installs_laptop_unit_assets_and_profile_receipt(self):
        with tempfile.TemporaryDirectory()as tmp:
            base=pathlib.Path(tmp);release,rid=fixture(base);home=base/'home';home.mkdir()
            install=i.Installer('user',True);commands=[]
            with mock.patch.object(i,'candidate',return_value=release),mock.patch.object(pathlib.Path,'home',return_value=home),mock.patch.object(install,'systemctl',side_effect=lambda *a,**k:commands.append(a)),mock.patch.object(install,'probe',return_value='inactive'),contextlib.redirect_stdout(io.StringIO()):install.execute()
            unit=(home/'.config/systemd/user/hyprv-shell.service').read_text()
            self.assertIn('Environment=HYPRV_PROFILE=laptop',unit)
            self.assertIn('Environment=HYPRV_ASSET_ROOT=%h/.local/share/hyprv-shell',unit)
            for src,dest,_ in i.user_files(release):self.assertEqual((release/src).read_bytes(),(home/dest).read_bytes())
            self.assertEqual(commands,[('daemon-reload',)])
            self.assertFalse((home/'.local/state/hyprv-cutover/native.enabled').exists())
    def test_user_only_root_plan_performs_no_commands_or_mutations(self):
        with tempfile.TemporaryDirectory()as tmp:
            release,_=fixture(pathlib.Path(tmp));install=i.Installer('root')
            with mock.patch.object(i,'candidate',return_value=release),mock.patch.object(install,'systemctl')as cmd,contextlib.redirect_stdout(io.StringIO())as out:
                install.execute();cmd.assert_not_called();self.assertIn('no root files',out.getvalue())
    def test_install_profile_mismatch_is_rejected_before_commands(self):
        with tempfile.TemporaryDirectory()as tmp:
            release,_=fixture(pathlib.Path(tmp));install=i.Installer('user',True,profile='desktop')
            with mock.patch.object(i,'candidate',return_value=release),mock.patch.object(install,'systemctl')as command:
                with self.assertRaisesRegex(RuntimeError,'profile differs'):install.execute()
                command.assert_not_called()
    def test_cpu_only_policy_and_unit_have_no_mutating_capabilities(self):
        unit=(ROOT/'packaging/hyprv-cpu-power-only.service').read_text();policy=(ROOT/'packaging/org.hyprv.shell.cpu-power.policy').read_text()
        for name in ('ExecStartPre','ExecStopPost','ReadWritePaths','WantedBy','WatchdogSec'):self.assertNotIn(name+'=',unit)
        self.assertIn('ExecCondition=/usr/libexec/hyprv-cpu-power --eligible',unit)
        self.assertIn('ReadOnlyPaths=/sys',unit);self.assertNotIn('fan-control',policy);self.assertNotIn('egpu',policy)
        for path in ('packaging/hyprv-privhelper.service','crates/hyprv-privhelper/packaging/hyprv-privhelper.service'):
            self.assertIn('ExecCondition=/usr/libexec/hyprv-privhelper --eligible-cooling',(ROOT/path).read_text())
    def test_cpu_only_root_upgrade_restores_old_files_on_restart_failure(self):
        with tempfile.TemporaryDirectory()as tmp:
            base=pathlib.Path(tmp);release,rid=fixture(base,True);dests=[]
            for src,dest,mode in i.FILES_CPU:
                target=base/'dest'/pathlib.Path(dest).name;target.parent.mkdir(exist_ok=True);target.write_bytes(b'old');dests.append((src,str(target),mode))
            install=i.Installer('root',True,rid);commands=[]
            def ctl(*args,**kw):
                commands.append(args)
                if args==('restart',i.CPU_HELPER)and kw.get('check',True):raise subprocess.CalledProcessError(1,args)
            with mock.patch.object(i,'ROOT',release),mock.patch.object(i,'FILES_CPU',dests),mock.patch.object(i,'CPU_STATE',base/'state'),mock.patch.object(i,'CPU_RECORD',base/'record.json'),mock.patch.object(i,'DESKTOP_POLICY',base/'absent'),mock.patch.object(i,'CPU_UNIT_DIRS',(base/'empty-system',)),mock.patch.object(i,'orchestration_lock',return_value=contextlib.nullcontext()),mock.patch.object(i,'require_root_context',return_value=(rid,{})),mock.patch.object(i.Transaction,'safe_dir',lambda self,d,mode=0o700:d.mkdir(parents=True,exist_ok=True,mode=mode)),mock.patch.object(i.os,'chown'),mock.patch.object(install,'systemctl',side_effect=ctl),mock.patch.object(install,'probe',return_value='active'),contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(subprocess.CalledProcessError):install.execute()
            self.assertTrue(all(pathlib.Path(dest).read_bytes()==b'old'for _,dest,_ in dests))
            self.assertTrue(all(i.HELPER not in cmd for cmd in commands))
class Cutover(unittest.TestCase):
    def test_patch_forward_reverse_and_all_ipc_mappings(self):
        with tempfile.TemporaryDirectory()as tmp:
            base=pathlib.Path(tmp);release,_=fixture(base);hypr=base/'hyprv';shutil.copytree(ROOT/'cutover/tests/fixtures/laptop',hypr)
            before={str(p.relative_to(hypr)):p.read_bytes()for p in hypr.rglob('*')if p.is_file()}
            scratch,changes=c.prepare(release,hypr)
            try:
                for source,dest,_ in changes:
                    content=source.read_text();self.assertNotIn('quickshell ipc',content);self.assertNotIn('quickshell -p',content);self.assertNotIn('qs --path',content)
                    if source.suffix=='.lua':subprocess.run(['luac','-p',str(source)],check=True)
                    else:subprocess.run(['bash','-n',str(source)],check=True)
            finally:scratch.cleanup()
            self.assertEqual(before,{str(p.relative_to(hypr)):p.read_bytes()for p in hypr.rglob('*')if p.is_file()})
            (hypr/'hypr/controls.lua').write_text('drift')
            with self.assertRaisesRegex(RuntimeError,'base drift'):c.prepare(release,hypr)
    def test_rollback_without_any_candidate_or_journal_is_already_done(self):
        with tempfile.TemporaryDirectory()as tmp:
            with mock.patch.object(c,'STATE',pathlib.Path(tmp)/'state'),mock.patch.object(c,'metadata')as metadata,mock.patch.object(c,'command')as command,contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(c.execute('rollback',True),0)
                metadata.assert_not_called();command.assert_not_called()
    def test_dry_run_switch_rollback_have_no_commands_or_state(self):
        with tempfile.TemporaryDirectory()as tmp:
            base=pathlib.Path(tmp);release,rid=fixture(base);metadata=json.loads((release/'release.json').read_text())
            with mock.patch.object(c,'metadata',return_value=(rid,metadata)),mock.patch.object(c,'STATE',base/'state'),mock.patch.object(c,'command')as cmd,contextlib.redirect_stdout(io.StringIO()):
                c.execute('switch');c.execute('rollback');cmd.assert_not_called()
            self.assertFalse((base/'state').exists())
    def test_switch_failure_and_rollback_restore_files_marker_and_legacy_intent(self):
        for fail in (False,True):
            with self.subTest(fail=fail),tempfile.TemporaryDirectory()as tmp:
                base=pathlib.Path(tmp);release,rid=fixture(base);tree=base/'hyprv';shutil.copytree(ROOT/'cutover/tests/fixtures/laptop',tree);(tree/'rshell').mkdir()
                original={str(p.relative_to(tree)):p.read_bytes()for p in tree.rglob('*')if p.is_file()};calls=[]
                def command(args):
                    calls.append(list(map(str,args)))
                    if fail and list(args)[-2:]==['start',c.UNIT]:raise subprocess.CalledProcessError(1,args)
                with mock.patch.object(c,'metadata',return_value=(rid,{'workspace':str(tree/'rshell')})),mock.patch.object(c,'RELEASE',release),mock.patch.object(c,'STATE',base/'state'),mock.patch.object(c,'MARKER',base/'native.enabled'),mock.patch.object(c,'HOME',base/'home'),mock.patch.object(c,'legacy_running',return_value=True),mock.patch.object(c,'command',side_effect=command),mock.patch.object(c,'native_running',return_value=False),mock.patch.object(c,'wait_native'),mock.patch.object(c.installer.paths,'stable_path',side_effect=pathlib.Path):
                    if fail:
                        with self.assertRaises(subprocess.CalledProcessError):c.execute('switch',True)
                    else:
                        c.execute('switch',True);self.assertTrue((base/'native.enabled').exists());c.execute('rollback',True)
                        count=len(calls);c.execute('rollback',True);self.assertEqual(len(calls),count)
                self.assertFalse((base/'native.enabled').exists())
                self.assertEqual(original,{str(p.relative_to(tree)):p.read_bytes()for p in tree.rglob('*')if p.is_file()})
                self.assertTrue(any(cmd[-1].endswith('quickshell/scripts/launch.sh')for cmd in calls))
                self.assertFalse(any('sudo'in cmd or(i.HELPER in cmd)for cmd in calls))
    def test_readiness_binds_ipc_to_profile_elf_and_owned_output_layers(self):
        with tempfile.TemporaryDirectory()as tmp:
            base=pathlib.Path(tmp);home=base/'home';binary=home/'.local/bin/hyprv-shell';binary.parent.mkdir(parents=True);binary.write_bytes(b'ELF')
            proc=base/'proc/42';proc.mkdir(parents=True);(proc/'exe').write_bytes(b'ELF');(proc/'environ').write_bytes(b'HYPRV_PROFILE=laptop\0')
            def output(args):
                if '--property=MainPID'in args:return '42'
                if args[-1]=='monitors':return '[{"name":"eDP-1"}]'
                if args[-1]=='layers':return json.dumps({'eDP-1':{'levels':{'0':[{'pid':42,'namespace':'hyprv-screen-corner-overlay-eDP-1'}]}}})
                self.assertEqual(args[-2:],['layoutStatus','eDP-1']);return '{"expanded":false}'
            with mock.patch.object(c,'HOME',home),mock.patch.object(c,'PROC',base/'proc'),mock.patch.object(c,'output',side_effect=output):
                c.wait_native(.1)
                (proc/'environ').write_bytes(b'HYPRV_PROFILE=desktop\0')
                with self.assertRaisesRegex(RuntimeError,'profile'):c.wait_native(.01)
class LaptopEvidence(unittest.TestCase):
    def test_desktop_acceptance_cannot_supply_laptop_proof(self):
        with tempfile.TemporaryDirectory()as tmp:
            root=pathlib.Path(tmp);(root/'cutover/evidence').mkdir(parents=True)
            data={'schema':1,'profile':'desktop','binary_sha256':'a','sources_sha256':g.input_digest({}),'epoch':__import__('time').time()}
            (root/'cutover/evidence/laptop-e2e.json').write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError,'mismatch'):g.bound_report(root,'laptop-e2e.json','a',{})
            with self.assertRaises(OSError):g.bound_report(root,'laptop-perf.json','a',{})

    def test_missing_or_changed_image_fails_even_with_fresh_bound_receipt(self):
        from PIL import Image
        with tempfile.TemporaryDirectory()as tmp:
            root=pathlib.Path(tmp);oracle=root/'parity/laptop/reference';oracle.mkdir(parents=True);(root/'cutover/evidence').mkdir(parents=True)
            image=root/'reference.png';Image.new('RGBA',(2,2),(1,2,3,255)).save(image)
            actual=root/'actual.png';shutil.copyfile(image,actual)
            ref={'path':'reference.png','sha256':sha(image.read_bytes())};act={'path':'actual.png','sha256':sha(actual.read_bytes())}
            slots=[{'id':k,'component':k,'reference':ref}for k in ('bar','island','panels','fullscreen','corners')]
            (oracle/'manifest.json').write_text(json.dumps({'slots':slots}))
            receipt={'schema':1,'profile':'laptop','binary_sha256':'a','sources_sha256':g.input_digest({}),'epoch':__import__('time').time(),'backend':'production-gl','rows':[{'id':slot['id'],'reference':ref,'actual':act}for slot in slots]}
            path=root/'cutover/evidence/laptop-parity.json';path.write_text(json.dumps(receipt));self.assertIn('5 laptop',g.visual(root,'a',{}))
            actual.unlink()
            with self.assertRaises(OSError):g.visual(root,'a',{})
            Image.new('RGBA',(2,2),(1,2,4,255)).save(actual);act['sha256']=sha(actual.read_bytes());path.write_text(json.dumps(receipt))
            with self.assertRaisesRegex(ValueError,'exact image mismatch'):g.visual(root,'a',{})
    def test_performance_caps_and_idle_proof_are_not_waived(self):
        with tempfile.TemporaryDirectory()as tmp:
            root=pathlib.Path(tmp);(root/'cutover/evidence').mkdir(parents=True)
            report={'profile':'laptop','binary_sha256':'a','nested':True,'instrumented':False,'seconds':60,'hook_events':0,'cpu_percent':.08,'rss_mib':119,'hidden_timer_wakeups':0,'idle_gpu_submissions':0}
            def write():
                path=root/'perf.json';path.write_text(json.dumps(report))
                (root/'cutover/evidence/laptop-perf.json').write_text(json.dumps({'schema':1,'profile':'laptop','binary_sha256':'a','sources_sha256':g.input_digest({}),'epoch':__import__('time').time(),'report':{'path':'perf.json','sha256':sha(path.read_bytes())}}))
            write();g.performance(root,'a',{})
            for key,value in [('seconds',float('nan')),('seconds',float('inf')),('cpu_percent',.101),('rss_mib',120.01),('hidden_timer_wakeups',1),('idle_gpu_submissions',False),('hook_events',1)]:
                old=report[key];report[key]=value;write()
                with self.subTest(key=key),self.assertRaises(ValueError):g.performance(root,'a',{})
                report[key]=old
