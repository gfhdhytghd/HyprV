"""Deployment metadata cannot refer to ephemeral build inputs; no host writes."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
ROOT=Path(__file__).resolve().parents[2]
def load(name,relative):
    spec=importlib.util.spec_from_file_location(name,ROOT/relative)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
p=load('test_deployment_paths','packaging/release_paths.py')
b=load('test_paths_builder','packaging/build_release.py')
i=load('test_paths_installer','packaging/install.py')
g=load('test_paths_preflight','cutover/preflight.py')
c=load('test_paths_control','cutover/control.py')
class ReleasePaths(unittest.TestCase):
    def metadata(self,workspace=ROOT):
        return dict(schema=1,workspace=str(workspace),owner=__import__('pwd').getpwuid(os.getuid()).pw_name,sources_sha256='a'*64)
    def test_real_workspace_and_required_paths_exist(self):
        self.assertEqual(p.validate_metadata(self.metadata()),ROOT.resolve())
    def test_shared_build_target_is_not_a_deployment_workspace(self):
        target=ROOT/'target'
        self.assertEqual(p.stable_path(str(target),allow_build_target=True),target.resolve())
        with self.assertRaises(RuntimeError):p.stable_path(str(target))
        with self.assertRaises(RuntimeError):p.stable_path('/tmp/snapshot/target',allow_build_target=True)
    def test_temp_snapshot_relative_and_missing_paths_rejected(self):
        for value in ('/tmp/hrc-1342242/rshell','/var/tmp/release','/run/user/1000/repo','/dev/shm/repo',str(ROOT/'target/repo'),str(ROOT/'snapshots/repo'),str(ROOT/'snapshot-123/repo'),'relative',str(ROOT/'does-not-exist')):
            with self.subTest(path=value),self.assertRaises(RuntimeError):p.validate_metadata(self.metadata(value))
    def test_symlink_to_temporary_tree_rejected(self):
        # Stable lexical path must not conceal a /tmp target.
        with mock.patch.object(Path,'resolve',return_value=Path('/tmp/hidden')):
            with self.assertRaisesRegex(RuntimeError,'temporary/snapshot'):p.stable_path(str(ROOT))
    def test_missing_preflight_blocks_metadata(self):
        with mock.patch.object(Path,'is_file',return_value=False),self.assertRaisesRegex(RuntimeError,'file missing'):
            p.validate_metadata(self.metadata())
    def test_unknown_metadata_path_is_not_silently_accepted(self):
        data=self.metadata();data['asset_root']='/tmp/assets'
        with self.assertRaisesRegex(RuntimeError,'metadata fields'):p.validate_metadata(data)
    def test_installer_checks_metadata_even_with_valid_manifest(self):
        with tempfile.TemporaryDirectory()as tmp:
            release=Path(tmp);data=json.dumps(self.metadata('/tmp/hrc-old/rshell')).encode()
            (release/'release.json').write_bytes(data)
            (release/'MANIFEST.sha256').write_text(__import__('hashlib').sha256(data).hexdigest()+'  release.json\n')
            with self.assertRaisesRegex(RuntimeError,'temporary/snapshot'):i.verify_release(release)
    def test_assembler_refuses_snapshot_default_and_accepts_explicit_deployment(self):
        with tempfile.TemporaryDirectory()as tmp:
            root=Path(tmp)
            for source in b.RELEASE_FILES.values():
                path=root/source;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'test')
            with mock.patch.object(b,'ROOT',root),mock.patch.object(b,'runtime_files',return_value={}):
                with mock.patch.dict(os.environ,{},clear=True),self.assertRaisesRegex(RuntimeError,'temporary/snapshot'):b.assemble(root/'packaging/dist',{})
                with mock.patch.dict(os.environ,HYPRV_DEPLOY_WORKSPACE=str(ROOT)):b.assemble(root/'packaging/dist',{})
            self.assertEqual(json.loads((root/'packaging/dist/release/release.json').read_text())['workspace'],str(ROOT))
    def test_system_switch_uses_validated_workspace_for_user_preflight(self):
        with mock.patch.object(c,'STAGED',True),mock.patch.object(c,'RELEASE',self.metadata()),mock.patch.object(c,'owner_manager',return_value=['runuser','-u','wilf','--']):
            self.assertEqual(c.owner_preflight_command()[-3:],[ '-B',ROOT/'cutover/preflight.py','--installed'])
            with mock.patch.object(c,'RELEASE',self.metadata('/tmp/snapshot')),self.assertRaises(RuntimeError):c.owner_preflight_command()
    def test_preflight_checks_deployment_paths(self):
        with mock.patch.object(g.build.paths,'validate_metadata',side_effect=RuntimeError('snapshot path')):
            with self.assertRaisesRegex(RuntimeError,'snapshot path'):g.validate_release_paths()
    def test_installed_record_workspace_and_paths_are_validated(self):
        record=dict(workspace=str(ROOT),owner=self.metadata()['owner'],dir='a'*16,files={str(ROOT/'Cargo.toml'):'a'*64})
        p.validate_installed_record(record,self.metadata())
        for key,value in [('workspace','/tmp/old'),('dir','../outside'),('files',{'/tmp/missing':'a'*64})]:
            old=record[key];record[key]=value
            with self.assertRaises(RuntimeError):p.validate_installed_record(record,self.metadata())
            record[key]=old
    def test_repackage_rejects_changed_runtime_and_binary(self):
        with tempfile.TemporaryDirectory()as tmp:
            root=Path(tmp);dist=root/'packaging/dist';dist.mkdir(parents=True)
            receipt=dict(profile='release',sources={'crates/hyprv-shell/src/main.rs':'old'},binaries={})
            (dist/'build.json').write_text(json.dumps(receipt))
            with mock.patch.object(b,'ROOT',root),mock.patch.object(b,'sources',return_value={'crates/hyprv-shell/src/main.rs':'changed'}):
                with self.assertRaisesRegex(RuntimeError,'non-packaging inputs changed'):b.repackage()
            receipt['sources']={};receipt['binaries']={b.BINS[0]:'incorrect'};(dist/'build.json').write_text(json.dumps(receipt));(dist/b.BINS[0]).write_bytes(b'changed')
            with mock.patch.object(b,'ROOT',root),mock.patch.object(b,'sources',return_value={}):
                with self.assertRaisesRegex(RuntimeError,'ELF changed'):b.repackage()
if __name__=='__main__':unittest.main()
