"""P0 nct6775 handoff regression: entirely fake systemd, no root/sysfs writes."""
import contextlib, importlib.util, io, pathlib, subprocess, tempfile, unittest
from unittest import mock
spec=importlib.util.spec_from_file_location('safety_fixtures',pathlib.Path(__file__).with_name('test_review_fixes.py'))
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
c=f.c

class CoolingHandoff(unittest.TestCase):
    def setup_case(self,stack,phase='legacy'):
        sd=f.FakeSystemd({c.HELPER:{'load':'loaded','enabled':'masked','active':False},
            'hyprv-fan-control.service':{'load':'loaded','enabled':'enabled','active':True},
            'hyprv-fan-permissions.service':{'load':'loaded','enabled':'enabled','active':True},
            'hyprv-cpu-power.service':{'load':'loaded','enabled':'disabled','active':False}})
        data={'phase':phase,'units':{k:{'enabled':v['enabled'],'active':v['active']}for k,v in sd.units.items()if k!=c.HELPER},'attributes':[]}
        real=sd.run
        def run(cmd,**kw):
            if '--user'in cmd and 'show'in cmd:return subprocess.CompletedProcess(cmd,0,'not-found','')
            return real(cmd,**kw)
        sd.run=run
        stack.enter_context(mock.patch.object(c.subprocess,'run',side_effect=sd.run))
        stack.enter_context(mock.patch.object(c.os,'geteuid',return_value=0))
        stack.enter_context(mock.patch.object(c,'verify_staged'))
        stack.enter_context(mock.patch.object(c,'journal',f.journal_with(data)))
        stack.enter_context(mock.patch.object(c,'units',return_value=data['units'].copy()))
        stack.enter_context(mock.patch.object(c,'attributes',return_value=[]))
        stack.enter_context(mock.patch.object(c,'set_permissions'))
        stack.enter_context(mock.patch.object(c,'qs_running',return_value=False))
        stack.enter_context(mock.patch.object(c,'native_running',return_value=False))
        stack.enter_context(mock.patch.object(c,'processes',return_value=[]))
        stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
        stack.enter_context(contextlib.redirect_stderr(io.StringIO()))
        return sd,data
    def assert_restored(self,sd,data):
        self.assertEqual(data['phase'],'legacy')
        for name,old in data['units'].items():
            self.assertEqual((sd.unit(name)['active'],sd.unit(name)['enabled']),(old['active'],old['enabled']))
        self.assertFalse(sd.unit(c.HELPER)['active'])
    def test_capability_failure_does_not_touch_legacy(self):
        for which in ('failsafe-check','helper-check','failsafe-store'):
            with self.subTest(which=which),contextlib.ExitStack()as stack:
                sd,data=self.setup_case(stack);real=sd.run
                def run(cmd,**kw):
                    if (which=='failsafe-check'and cmd==['/usr/libexec/hyprv-cooling-failsafe','--check'])or(which=='helper-check'and cmd==['/usr/libexec/hyprv-privhelper','--check'])or(which=='failsafe-store'and cmd==['/usr/libexec/hyprv-cooling-failsafe']):raise subprocess.CalledProcessError(1,cmd)
                    return real(cmd,**kw)
                stack.enter_context(mock.patch.object(c.subprocess,'run',side_effect=run))
                with self.assertRaises(subprocess.CalledProcessError):c.root_handoff('switch')
                self.assertFalse(any(any(v in cmd for v in ('disable','stop','mask','start','enable'))for cmd in sd.log))
                self.assertEqual(data['phase'],'legacy')
    def test_every_post_stop_failure_restores_original_states(self):
        for fail in ('stop-first','stop-second','failsafe','permissions','helper-start','helper-ready','save'):
            with self.subTest(fail=fail),contextlib.ExitStack()as stack:
                sd,data=self.setup_case(stack);real=sd.run;armed=[True];stores=[0]
                def run(cmd,**kw):
                    if cmd==['/usr/libexec/hyprv-cooling-failsafe']:stores[0]+=1
                    trigger=(fail=='stop-first'and cmd[-2:]==['--now','hyprv-fan-control.service'])or(fail=='stop-second'and cmd[-2:]==['--now','hyprv-fan-permissions.service'])or(fail=='failsafe'and stores[0]==2 and cmd==['/usr/libexec/hyprv-cooling-failsafe'])or(fail=='helper-start'and cmd[-3:]==['enable','--now',c.HELPER])or(fail=='helper-ready'and cmd[-3:]==['is-active','--quiet',c.HELPER])
                    result=real(cmd,**kw)
                    if trigger and armed[0]:armed[0]=False;raise subprocess.CalledProcessError(1,cmd)
                    return result
                stack.enter_context(mock.patch.object(c.subprocess,'run',side_effect=run))
                if fail=='permissions':
                    def permissions(rows,restore):
                        if not restore:raise OSError('chown failed')
                    stack.enter_context(mock.patch.object(c,'set_permissions',side_effect=permissions))
                if fail=='save':
                    @contextlib.contextmanager
                    def journal(_):
                        def save():
                            if data['phase']=='native'and armed[0]:armed[0]=False;raise OSError('disk full')
                        yield data,save
                    stack.enter_context(mock.patch.object(c,'journal',journal))
                with self.assertRaisesRegex(RuntimeError,'legacy units restored'):c.root_handoff('switch')
                self.assert_restored(sd,data)
    def test_recover_switching_with_broken_old_failsafe_is_idempotent(self):
        with contextlib.ExitStack()as stack:
            sd,data=self.setup_case(stack,'switching');real=sd.run
            sd.unit('hyprv-fan-control.service').update(active=False,enabled='disabled')
            def run(cmd,**kw):
                if cmd==['/usr/libexec/hyprv-cooling-failsafe']:raise subprocess.CalledProcessError(1,cmd,stderr='EINVAL')
                return real(cmd,**kw)
            stack.enter_context(mock.patch.object(c.subprocess,'run',side_effect=run))
            baseline=data['units'].copy()
            self.assertEqual(c.root_handoff('recover'),0)
            self.assert_restored(sd,data);self.assertEqual(data['units'],baseline)
            self.assertTrue(any('failsafe:'in w for w in data['warnings']))
            self.assertEqual(c.root_handoff('recover'),0)
    def test_rollback_continues_when_failsafe_and_permission_restore_fail(self):
        with contextlib.ExitStack()as stack:
            sd,data=self.setup_case(stack,'native');real=sd.run
            sd.unit(c.HELPER).update(active=True,enabled='enabled')
            sd.unit('hyprv-fan-control.service').update(active=False,enabled='disabled')
            def run(cmd,**kw):
                if cmd==['/usr/libexec/hyprv-cooling-failsafe']:raise OSError('EINVAL')
                return real(cmd,**kw)
            stack.enter_context(mock.patch.object(c.subprocess,'run',side_effect=run))
            stack.enter_context(mock.patch.object(c,'set_permissions',side_effect=OSError('chmod failed')))
            self.assertEqual(c.root_handoff('rollback'),2)
            self.assertTrue(sd.unit('hyprv-fan-control.service')['active'])
            self.assertEqual(data['phase'],'rollback-incomplete')
    def test_failed_switch_recreates_exact_journalled_symlinks(self):
        with tempfile.TemporaryDirectory()as tmp,contextlib.ExitStack()as stack:
            sd,data=self.setup_case(stack)
            directory=pathlib.Path(tmp);wants=directory/'multi-user.target.wants';wants.mkdir()
            sd.dirs=[directory]
            baseline={}
            for name,old in data['units'].items():
                links=[]
                if old['enabled']=='enabled':
                    link=wants/name;target='/legacy/original-'+name
                    link.symlink_to(target);links=[{'path':str(link),'target':target}]
                baseline[name]={**old,'links':links}
            stack.enter_context(mock.patch.object(c,'units',return_value=baseline))
            def permissions(rows,restore):
                if not restore:raise OSError('permission change failed after stop')
            stack.enter_context(mock.patch.object(c,'set_permissions',side_effect=permissions))
            with self.assertRaisesRegex(RuntimeError,'legacy units restored'):c.root_handoff('switch')
            self.assertEqual(data['units'],baseline)
            self.assertEqual(data['phase'],'legacy')
            for name,old in baseline.items():
                self.assertEqual(sd.state(name),old['enabled'])
                self.assertEqual(sd.unit(name)['active'],old['active'])
                for link in old['links']:self.assertEqual(str(pathlib.Path(link['path']).readlink()),link['target'])

    def test_recovery_does_not_stop_manually_restored_legacy(self):
        with contextlib.ExitStack()as stack:
            sd,data=self.setup_case(stack,'switching')
            self.assertEqual(c.root_handoff('recover'),0)
            self.assert_restored(sd,data)
            self.assertFalse(any('stop'in cmd and cmd[-1]!=c.HELPER for cmd in sd.log))
    def test_switching_retry_recovers_instead_of_capturing_or_switching_again(self):
        with contextlib.ExitStack()as stack:
            sd,data=self.setup_case(stack,'switching')
            self.assertEqual(c.root_handoff('switch'),2)
            self.assert_restored(sd,data)
            self.assertFalse(any('disable'in cmd and cmd[-1]!=c.HELPER for cmd in sd.log))

class PermissionRecovery(unittest.TestCase):
    def test_one_attribute_failure_does_not_skip_other_channel_permissions(self):
        with tempfile.TemporaryDirectory()as tmp:
            chip=pathlib.Path(tmp);(chip/'name').write_text('nct6799')
            rows=[{'name':n,'device':'same','uid':1000,'gid':1000,'mode':0o660}for n in ('pwm1','pwm1_enable','pwm7','pwm7_enable')]
            changed=[];modes=[]
            def chown(path,uid,gid):
                changed.append(path.name)
                if path.name=='pwm1':raise OSError('attribute failed')
            with mock.patch.object(c,'attributes',return_value=rows),mock.patch.object(c.pathlib.Path,'glob',return_value=[chip]),mock.patch.object(c.os,'chown',side_effect=chown),mock.patch.object(c.os,'chmod',side_effect=lambda p,m:modes.append(p.name)),contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError,'attribute failed'):c.set_permissions(rows,True)
            self.assertEqual(changed,[r['name']for r in rows]);self.assertEqual(modes,changed)

class InterruptedInstall(unittest.TestCase):
    def test_install_in_switching_never_activates_helper(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=f.RootInstallSandbox(tmp,'switching',{'load':'loaded','enabled':'enabled','active':False})
            self.assertEqual(box.install(),0)
            self.assertEqual(box.sd.unit(c.HELPER)['enabled'],'masked')
            self.assertFalse(box.sd.unit(c.HELPER)['active'])
            self.assertFalse(any(any(x in cmd for x in ('start','restart'))for cmd in box.sd.log))
    def test_install_in_switching_with_running_helper_requires_recovery(self):
        with tempfile.TemporaryDirectory()as tmp:
            box=f.RootInstallSandbox(tmp,'switching',{'load':'loaded','enabled':'enabled','active':True})
            with self.assertRaisesRegex(RuntimeError,'running outside'):box.install()
            self.assertFalse(any('restart'in cmd for cmd in box.sd.log))

if __name__=='__main__':unittest.main()
