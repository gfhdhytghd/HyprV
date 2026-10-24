"""User visual sign-off pins the exact shell and historical PASS evidence."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('visual_preflight',Path(__file__).resolve().parents[1]/'preflight.py')
g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)

class VisualAcceptance(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name);self.digest='a'*64
        binary=self.root/'packaging/dist/hyprv-shell';binary.parent.mkdir(parents=True);binary.write_bytes(b'test artifact')
        self.preflight={'ready':True,'checks':[{'check':'parity gate','ok':True,'detail':'production GL/fullscreen/corners pass'}]}
        self.release={'release_id':'b'*64,'shell_sha256':self.digest,'all_sources_and_manifest_and_three_ELFs_verified':True}
        self.acceptance={'schema':1,'accepted_by':'user','date':'2026-10-24','note':'user accepted visual parity; no re-render required','binary_sha256':self.digest,'evidence_release_id':'b'*64,'last_passing_evidence':{}}
        self.write_evidence()
    def write_evidence(self):
        for key,data in [('preflight',self.preflight),('release',self.release)]:
            p=self.root/(key+'.json');p.write_text(json.dumps(data))
            self.acceptance['last_passing_evidence'][key]={'path':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
    def gate(self):return g.accepted_parity_gate(self.acceptance,self.digest,self.root)
    def test_historical_fully_passing_evidence_is_sufficient_without_new_images(self):
        self.assertIn('accepted: user 2026-10-24',self.gate())
        self.assertFalse((self.root/'parity').exists())
    def test_wrong_user_date_schema_note_and_binary_fail_closed(self):
        for field,value in [('schema',0),('accepted_by','agent'),('date','not-a-date'),('note',''),('binary_sha256','c'*64)]:
            old=self.acceptance[field];self.acceptance[field]=value
            with self.subTest(field=field),self.assertRaises(ValueError):self.gate()
            self.acceptance[field]=old
    def test_missing_or_tampered_evidence_is_not_accepted(self):
        (self.root/'preflight.json').write_text('{}')
        with self.assertRaisesRegex(ValueError,'changed'):self.gate()
        (self.root/'preflight.json').unlink()
        with self.assertRaises(OSError):self.gate()
    def test_red_or_ambiguous_old_preflight_is_not_accepted(self):
        for prior in [{'ready':False,'checks':self.preflight['checks']},{'ready':True,'checks':[]},{'ready':True,'checks':[{'check':'parity gate','ok':False}]},{'ready':True,'checks':self.preflight['checks']*2}]:
            self.preflight=prior;self.write_evidence()
            with self.assertRaises(ValueError):self.gate()
    def test_wrong_historical_shell_or_release_is_rejected(self):
        for field,value in [('shell_sha256','c'*64),('release_id','d'*64),('all_sources_and_manifest_and_three_ELFs_verified',False)]:
            old=self.release[field];self.release[field]=value;self.write_evidence()
            with self.subTest(field=field),self.assertRaises(ValueError):self.gate()
            self.release[field]=old
    def test_reference_cannot_escape_workspace(self):
        self.acceptance['last_passing_evidence']['preflight']['path']='../outside.json'
        with self.assertRaises(ValueError):self.gate()
    def test_tooling_remains_strict_on_demand_without_acceptance(self):
        with patch.object(g,'ROOT',self.root),patch.object(g.parity,'inspect',return_value='strict render gate')as inspect:
            self.assertEqual(g.validate_parity(),'strict render gate');inspect.assert_called_once_with(self.root)
    def test_invalid_record_cannot_silently_fall_back_to_render_results(self):
        (self.root/'cutover').mkdir();(self.root/'cutover/parity-acceptance.json').write_text('{}')
        with patch.object(g,'ROOT',self.root),patch.object(g.parity,'inspect')as inspect:
            with self.assertRaises(ValueError):g.validate_parity()
            inspect.assert_not_called()
