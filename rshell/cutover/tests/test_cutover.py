import contextlib, copy, importlib.util, io, json, pathlib, subprocess, tempfile, unittest
from unittest import mock
ROOT=pathlib.Path(__file__).resolve().parents[2]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
c=load('cutover_control','cutover/control.py');g=load('cutover_gates','cutover/preflight.py');installer=load('cutover_install','packaging/install.py')
class Plans(unittest.TestCase):
    def test_all_dry_plans_and_ab_have_no_effects(self):
        with mock.patch.object(c.subprocess,'run',side_effect=AssertionError('dry-run executed a command')),mock.patch.object(c.os,'open',side_effect=AssertionError('dry-run wrote a file')),contextlib.redirect_stdout(io.StringIO()):
            for direction in ('switch','rollback'):
                for scope in ('user','system'):self.assertEqual(c.plan(direction,scope,False,None),0)
                self.assertEqual(c.plan(direction,'user',True,None),0)
            self.assertEqual(c.plan('switch','user',False,'DP-4'),4)
    def test_install_dry_plans_do_not_invoke_commands(self):
        with mock.patch.object(installer.subprocess,'run',side_effect=AssertionError('install applied')),contextlib.redirect_stdout(io.StringIO()):
            installer.Installer('root').execute();installer.Installer('user').execute()
    def test_root_plan_lists_exact_installed_units_and_failsafe(self):
        out=io.StringIO()
        with contextlib.redirect_stdout(out):c.plan('switch','system',False,None)
        for unit in c.LEGACY['system']:self.assertIn('disable --now '+unit,out.getvalue())
        self.assertIn('/usr/libexec/hyprv-cooling-failsafe',out.getvalue())
        self.assertNotIn('pkill',out.getvalue())
class Gates(unittest.TestCase):
    def green(self):
        row={'state':'one','file':'one.png','result':'pass','df':True,'max':8,'mean':.5,'structural':False}
        return {'results':{'island-gl':[row]},'renders':{'island-gl':{'status':'ok','exit':0,'backend':'gl'}}}
    def test_exact_parity_threshold_passes(self):self.assertTrue(g.parity_gate(self.green(),{'island':[{'file':'one.png','state':'one','distance_field_text':True}]}))
    def test_missing_empty_duplicate_and_structural_parity_fail(self):
        for case in ('missing','empty','duplicate','structural','stats'):
            data=self.green();expected={'island':[{'file':'one.png','state':'one','distance_field_text':True}]}
            if case=='missing':expected['island'].append({'file':'two.png','state':'two','distance_field_text':True})
            if case=='empty':data['results']['island-gl']=[];expected['island']=[]
            if case=='duplicate':data['results']['island-gl']*=2
            if case=='structural':data['results']['island-gl'][0]['structural']=True
            if case=='stats':data['results']['island-gl'][0]['max']=9
            with self.subTest(case=case),self.assertRaises(ValueError):g.parity_gate(data,expected)
    def test_non_text_must_be_bit_exact(self):
        data=self.green();data['results']['island-gl'][0].update(df=False,max=1,mean=.001)
        with self.assertRaises(ValueError):g.parity_gate(data,{'island':[{'file':'one.png','state':'one','distance_field_text':False}]})
    def perf(self):return {'mode':'release-latest','probe':{'cpu_percent':.05,'seconds':60,'task_counters':{}},'state_events':{},'build_snapshot':{'command':['build','--release']}}
    def test_live_full_release_perf_passes(self):self.assertTrue(g.perf_gate(self.perf()))
    def test_red_debug_partial_short_and_frozen_perf_fail(self):
        cases=[('cpu',.35),('cpu',float('nan')),('seconds',59),('excludes',['agent_island']),('agent_inputs_frozen',True),('mode','release-power-after'),('debug',True)]
        for key,value in cases:
            data=self.perf()
            if key in ('cpu','seconds'):data['probe']['cpu_percent'if key=='cpu'else'seconds']=value
            elif key=='debug':data['build_snapshot']['command']=['build']
            else:data[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):g.perf_gate(data)
    def real_shell(self):
        idle={'cpu_percent':.05,'seconds':60,'instrumented':False,'task_counters':{},'pid_namespace':'host','agent_inputs_frozen':False,'hook_file_events':0}
        return {'mode':'real-shell-release','build_snapshot':{'command':['cargo','build','--release']},'live_idle':idle,'activity':dict(idle,cpu_percent=5.)}
    def test_real_shell_accepts_idle_budget_without_an_activity_ceiling(self):
        self.assertTrue(g.shell_perf_gate(self.real_shell()))
    def test_real_shell_controlled_failure_blocks_but_controlled_pass_cannot_mask_live_activity(self):
        data=self.real_shell();data['controlled_idle']=dict(data['live_idle'],cpu_percent=.416,pid_namespace='private')
        with self.assertRaisesRegex(ValueError,'controlled quiet'):g.shell_perf_gate(data)
        data['controlled_idle']['cpu_percent']=.01;data['live_idle']['hook_file_events']=1
        with self.assertRaisesRegex(ValueError,'UNVERIFIED'):g.shell_perf_gate(data)
    def test_real_shell_rejects_short_debug_frozen_namespace_and_hook_activity(self):
        for key,value in [('seconds',59),('instrumented',True),('agent_inputs_frozen',True),('pid_namespace','private'),('hook_file_events',1),('cpu_percent',.11),('cpu_percent',float('nan'))]:
            data=self.real_shell();data['live_idle'][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):g.shell_perf_gate(data)
        data=self.real_shell();data['activity']['seconds']=59
        with self.assertRaises(ValueError):g.shell_perf_gate(data)
    def test_preflight_failure_prevents_stop_and_config_mutation(self):
        with mock.patch.object(c.os,'geteuid',return_value=1000),mock.patch.object(c,'preflight',side_effect=RuntimeError('red gate')),mock.patch.object(c,'journal')as journal,mock.patch.object(c,'stop_legacy')as stop:
            with self.assertRaises(RuntimeError):c.user_switch(False)
            journal.assert_not_called();stop.assert_not_called()
class Reversal(unittest.TestCase):
    def test_restore_recreates_journalled_links_and_starts_active_units(self):
        calls=[];made=[]
        saved={'off.service':{'enabled':'disabled','active':False,'links':[]},'manual.service':{'enabled':'disabled','active':True,'links':[]},
               'runtime.service':{'enabled':'enabled-runtime','active':True,'links':[{'path':'/run/x/default.target.wants/runtime.service','target':'/etc/x/runtime.service'}]}}
        state={'off.service':'disabled','manual.service':'disabled','runtime.service':'enabled-runtime'}
        def quiet(cmd):return subprocess.CompletedProcess(cmd,0,state[cmd[-1]],'')
        with mock.patch.object(c,'call',side_effect=lambda cmd,check=True,timeout=None:calls.append(cmd)or subprocess.CompletedProcess(cmd,0)),mock.patch.object(c,'quiet',side_effect=quiet),\
             mock.patch.object(c.pathlib.Path,'is_symlink',return_value=False),mock.patch.object(c.pathlib.Path,'exists',return_value=False),mock.patch.object(c.pathlib.Path,'mkdir'),\
             mock.patch.object(c.os,'symlink',side_effect=lambda t,p:made.append((t,str(p)))),contextlib.redirect_stdout(io.StringIO()):
            problems=c.restore_units('user',saved)
        self.assertEqual(problems,[])
        self.assertEqual(made,[('/etc/x/runtime.service','/run/x/default.target.wants/runtime.service')])
        self.assertEqual(calls,[[c.SYSTEMCTL,'--user','daemon-reload'],[c.SYSTEMCTL,'--user','start','manual.service'],[c.SYSTEMCTL,'--user','start','runtime.service']])
    def test_journal_survives_reentry_without_losing_original_snapshot(self):
        directory=ROOT/'cutover/.check';directory.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=directory)as tmp:
            with c.journal(pathlib.Path(tmp))as(data,save):data.update(phase='switching',units={'old':{'active':True}});save()
            with c.journal(pathlib.Path(tmp))as(data,save):self.assertEqual(data['units'],{'old':{'active':True}});data['phase']='native';save()
            with c.journal(pathlib.Path(tmp))as(data,_):self.assertEqual(data['phase'],'native')
    def test_journal_symlink_is_rejected_without_following(self):
        directory=ROOT/'cutover/.check';directory.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=directory)as tmp:
            tmp=pathlib.Path(tmp);canary=tmp/'canary';canary.write_text('unchanged');(tmp/'state.json').symlink_to(canary)
            with self.assertRaises(RuntimeError):
                with c.journal(tmp):pass
            self.assertEqual(canary.read_text(),'unchanged')
    def test_root_rollback_refuses_while_native_shell_exists(self):
        with mock.patch.object(c.os,'geteuid',return_value=0),mock.patch.object(c,'verify_staged'),mock.patch.object(c,'native_running',return_value=True),mock.patch.object(c,'call')as run:
            with self.assertRaises(RuntimeError):c.root_handoff('rollback')
            run.assert_not_called()
    def test_interrupted_switch_uses_recovery_without_reentering_stop(self):
        data={'phase':'switching','units':{'hyprv-fan-control.service':{'active':True,'enabled':'enabled'}},'attributes':[]}
        @contextlib.contextmanager
        def journal(_):yield data,lambda:None
        with mock.patch.object(c.os,'geteuid',return_value=0),mock.patch.object(c,'verify_staged'),mock.patch.object(c,'journal',journal),mock.patch.object(c,'recover_system',return_value=0)as recover,mock.patch.object(c,'stop_legacy')as stop,contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(c.root_handoff('switch'),2)
        recover.assert_called_once();stop.assert_not_called()
    def test_quickshell_selector_does_not_match_dankcal(self):
        uid=1234
        with mock.patch.object(c,'processes',return_value=[(1,uid,[b'quickshell',b'-p',b'/other/dankcal'])]):self.assertFalse(c.qs_running(uid))
        args=[b'quickshell',b'-p',str((c.ROOT.parent/'quickshell').resolve()).encode()]
        with mock.patch.object(c,'processes',return_value=[(1,uid,args)]):self.assertTrue(c.qs_running(uid));self.assertFalse(c.qs_running(uid+1))

class PatchInventory(unittest.TestCase):
    def test_patch_preimage_and_no_remaining_quickshell_commands(self):
        import hashlib,re
        inputs=json.loads((ROOT/'cutover/hypr-inputs.json').read_text())['files']
        fixture=ROOT/'cutover/tests/fixtures/legacy'
        for name,entry in inputs.items():
            self.assertEqual(hashlib.sha256((fixture/name).read_bytes()).hexdigest(),entry['before_sha256'])
        # Exercise both exact patch directions on immutable legacy inputs, never
        # the user's current (possibly already native or locally edited) files.
        with tempfile.TemporaryDirectory() as tmp:
            import shutil
            tree=pathlib.Path(tmp)/'HyprV'
            shutil.copytree(fixture,tree)
            for reverse,side in ((False,'after_sha256'),(True,'before_sha256')):
                subprocess.run([c.GIT,'apply',*(['--reverse']if reverse else[]),str(ROOT/'cutover/hypr.patch')],cwd=tree,check=True)
                for name,entry in inputs.items():
                    self.assertEqual(hashlib.sha256((tree/name).read_bytes()).hexdigest(),entry[side])
        patch=(ROOT/'cutover/hypr.patch').read_text()
        added='\n'.join(line[1:]for line in patch.splitlines()if line.startswith('+')and not line.startswith('+++'))
        self.assertNotRegex(added,r'quickshell\s+(kill|ipc|-p)')
        self.assertIn('showBrightnessLevel',(fixture/'hypr/controls.lua').read_text())
        self.assertIn('namespace = "hyprv-quickshell"',(fixture/'hypr/hyprland-winrule.lua').read_text())

class NativeReadiness(unittest.TestCase):
    def test_waits_for_ipc_and_bar_surfaces_instead_of_process_start(self):
        from types import SimpleNamespace
        replies=[SimpleNamespace(returncode=1,stderr='no outputs'),SimpleNamespace(returncode=0,stderr=''),SimpleNamespace(returncode=0,stderr='')]
        surfaces=[(False,'bar surfaces missing on DP-4'),(True,'bar mapped on DP-4')]
        with mock.patch.object(c.subprocess,'run',side_effect=replies)as run,mock.patch.object(c,'native_surfaces',side_effect=surfaces),mock.patch.object(c.time,'sleep'),contextlib.redirect_stdout(io.StringIO()):c.wait_native()
        self.assertEqual(run.call_count,3)
        self.assertEqual(run.call_args.args[0][-4:],['ipc','call','bar','dataStatus'])
    def test_surfaces_require_bar_layer_of_native_pid_on_every_monitor(self):
        layers={'DP-4':{'levels':{'2':[{'namespace':'hyprv-quickshell','pid':42}]}},'HDMI-A-1':{'levels':{'2':[{'namespace':'hyprv-quickshell','pid':7}]}}}
        monitors=[{'name':'DP-4'},{'name':'HDMI-A-1'},{'name':'DP-1','disabled':True}]
        def output(cmd,timeout=None):return '42'if 'MainPID'in ' '.join(map(str,cmd))else json.dumps(monitors if 'monitors'in cmd else layers)
        with mock.patch.object(c,'output',side_effect=output):ready,detail=c.native_surfaces()
        self.assertFalse(ready);self.assertIn('HDMI-A-1',detail)
if __name__=='__main__':unittest.main()

class CandidateAdmission(unittest.TestCase):
    def test_only_explicit_candidate_mode_replaces_installed_gate(self):
        with mock.patch.object(g,'validate_helper',side_effect=ValueError('installed mismatch'))as installed, mock.patch.object(g,'validate_helper_install_set',return_value='candidate inputs verified')as bundled:
            for flag in (False,True):
                rows=g.inspect(candidate=flag)
                self.assertEqual(rows[-1]['check'],'privhelper install set'if flag else'privhelper installed')
                self.assertEqual(rows[-1]['ok'],flag)
            installed.assert_called_once();bundled.assert_called_once()
        with self.assertRaises(ValueError):g.inspect(installed=True,candidate=True)

    def test_new_cpu_policy_files_are_installed_and_manifest_verified(self):
        rows={source:destination for source,destination,_ in installer.FILES_ROOT}
        self.assertEqual(rows['files/org.hyprv.Shell.PrivHelper.conf'],'/etc/dbus-1/system.d/org.hyprv.Shell.PrivHelper.conf')
        self.assertEqual(rows['files/org.hyprv.shell.fan-control.policy'],'/usr/share/polkit-1/actions/org.hyprv.shell.fan-control.policy')
        import xml.etree.ElementTree as ET
        conf=ET.parse(ROOT/'packaging/org.hyprv.Shell.PrivHelper.conf')
        self.assertTrue({'SubscribeCpuPower','UnsubscribeCpuPower'} <= {x.get('send_member')for x in conf.findall('.//allow')})
        policy=ET.parse(ROOT/'packaging/org.hyprv.shell.fan-control.policy')
        self.assertIn('org.hyprv.shell.cpu-power',{x.get('id')for x in policy.findall('action')})
        actions={x.get('id'):x for x in policy.findall('action')}
        for name,active in [('fan-control','yes'),('egpu-remove','auth_admin'),('cpu-power','yes')]:
            defaults=actions['org.hyprv.shell.'+name].find('defaults')
            self.assertEqual(defaults.findtext('allow_any'),'no')
            self.assertEqual(defaults.findtext('allow_inactive'),'no')
            self.assertEqual(defaults.findtext('allow_active'),active)
        self.assertIn('manifest-verified',g.validate_helper_install_set())

    def test_candidate_install_set_rejects_wrong_build_release_id(self):
        receipt=json.loads((ROOT/'packaging/dist/build.json').read_text())
        receipt['release_id']='0'*64
        original=pathlib.Path.read_text
        def read(path,*args,**kwargs):
            return json.dumps(receipt)if path==ROOT/'packaging/dist/build.json'else original(path,*args,**kwargs)
        with mock.patch.object(pathlib.Path,'read_text',read):
            with self.assertRaisesRegex(ValueError,'manifest differs'):g.validate_helper_install_set()
