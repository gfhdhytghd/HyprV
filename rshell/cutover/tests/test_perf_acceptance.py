"""Binary-specific user acceptance fails closed on changed or incomplete evidence."""
import copy
import importlib.util
import pathlib
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('acceptance_preflight',pathlib.Path(__file__).resolve().parents[1]/'preflight.py')
g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)

class PerformanceAcceptance(unittest.TestCase):
    def setUp(self):
        self.digest='a'*64
        window=dict(seconds=60,instrumented=False,cpu_percent=.3,task_counters={},hook_file_events=0,pid_namespace='host',agent_inputs_frozen=False)
        self.data=dict(mode='real-shell-release',epoch=100000,binary_sha256=self.digest,build_snapshot=dict(command=['cargo','build','--release']),controlled_idle=copy.deepcopy(window),live_idle=dict(window,cpu_percent=.46,hook_file_events=7),activity=dict(window,cpu_percent=5))
        self.acceptance=dict(accepted_by='user',date='2026-10-23',reason='Deployment accepted by user',binary_sha256=self.digest,controlled_quiet_max_pct=.5,live_hook_idle_max_pct=2.,hook_events_allowed=True)
    def gate(self):return g.accepted_shell_perf_gate(self.data,self.acceptance,self.digest,now=100001)
    def test_performance_fingerprint_includes_transitive_helper_library(self):
        sources=g.shell_sources()
        self.assertIn('crates/hyprv-privhelper/Cargo.toml',sources)
        self.assertEqual(sources['crates/hyprv-privhelper/src/cooling.rs'],g.sha(g.ROOT/'crates/hyprv-privhelper/src/cooling.rs'))
    def test_acceptance_reports_both_measurements_and_hooks(self):
        detail=self.gate()
        self.assertTrue(detail.startswith('accepted: user 2026-10-23'))
        self.assertIn('0.300000%',detail);self.assertIn('hook events 7',detail)
    def test_exact_boundaries_pass(self):
        self.data['controlled_idle']['cpu_percent']=.5;self.data['live_idle']['cpu_percent']=2
        self.gate()
    def test_mismatched_waiver_or_measurement_fails(self):
        for obj in (self.acceptance,self.data):
            with self.subTest(obj=obj):
                obj['binary_sha256']='b'*64
                with self.assertRaisesRegex(ValueError,'hash'):self.gate()
                obj['binary_sha256']=self.digest
    def test_bounds_invalid_cpu_and_missing_evidence(self):
        for key in ('controlled_idle','live_idle'):
            for field,value in [('cpu_percent',2.01),('cpu_percent',float('nan')),('cpu_percent',True),('seconds',59),('instrumented',True),('hook_file_events',None),('hook_file_events',-1)]:
                with self.subTest(key=key,field=field,value=value):
                    old=self.data[key][field];self.data[key][field]=value
                    with self.assertRaises(ValueError):self.gate()
                    self.data[key][field]=old
            old=self.data.pop(key)
            with self.assertRaises(ValueError):self.gate()
            self.data[key]=old
    def test_quiet_hooks_frozen_live_and_debug_are_rejected(self):
        for key,field,value in [('controlled_idle','hook_file_events',1),('live_idle','agent_inputs_frozen',True),('live_idle','pid_namespace','private'),('activity','seconds',59)]:
            old=self.data[key][field];self.data[key][field]=value
            with self.assertRaises(ValueError):self.gate()
            self.data[key][field]=old
        self.data['build_snapshot']['command']=['cargo','build']
        with self.assertRaises(ValueError):self.gate()
    def test_stale_future_missing_and_invalid_time(self):
        for epoch in (0,100002,None,float('nan'),True):
            self.data['epoch']=epoch
            with self.assertRaises(ValueError):self.gate()
    def test_acceptance_cannot_expand_authorized_bounds(self):
        for key,value in [('controlled_quiet_max_pct',.51),('live_hook_idle_max_pct',2.01),('accepted_by','agent'),('date','2026-10-24'),('reason',''),('hook_events_allowed',False)]:
            old=self.acceptance[key];self.acceptance[key]=value
            with self.assertRaises(ValueError):self.gate()
            self.acceptance[key]=old
    def test_strict_policy_still_rejects_without_acceptance(self):
        with self.assertRaises(ValueError):g.shell_perf_gate(self.data)
    def test_runtime_source_changes_cannot_use_readiness_amendment(self):
        with self.assertRaisesRegex(ValueError,'source changed'):
            g.validate_candidate_sources(dict(sources={'crates/hyprv-shell/src/main.rs':'old'}),{'crates/hyprv-shell/src/main.rs':'new'})
    def test_readiness_amendment_requires_exact_hashes(self):
        receipt=dict(sources={'cutover/preflight.py':'old'},binaries={'hyprv-shell':self.digest})
        current={'cutover/preflight.py':'new'}
        amendment=dict(build_receipt_sha256='receipt',binary_sha256=self.digest,changed_files=current)
        with patch.object(g,'sha',return_value='receipt'),patch.object(g.json,'loads',return_value=amendment),patch.object(pathlib.Path,'read_text',return_value='{}'):
            g.validate_candidate_sources(receipt,current)
            amendment['changed_files']={'cutover/preflight.py':'different'}
            with self.assertRaises(ValueError):g.validate_candidate_sources(receipt,current)

if __name__=='__main__':unittest.main()
