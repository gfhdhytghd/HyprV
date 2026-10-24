"""Regression coverage for admitting complete, stable reference captures."""
import base64
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('snapshot_ci', Path(__file__).with_name('ci-snapshot-loop.py'))
CI = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CI)
PNG = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6zL8AAAAASUVORK5CYII=')


class SnapshotAdmission(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='ci-copy-')
        self.addCleanup(self.temp.cleanup)
        deadline = mock.patch.object(CI, 'DEADLINE', time.time() + 120)
        deadline.start()
        self.addCleanup(deadline.stop)
        self.base = Path(self.temp.name)
        self.root = self.base / 'live/rshell'
        self.root.mkdir(parents=True)
        (self.root / 'Cargo.toml').write_text('generation one')
        for component in ('bar', 'island', 'panels'):
            directory = self.root / 'parity/reference' / component
            directory.mkdir(parents=True)
            (directory / 'frame.png').write_bytes(PNG)
            (directory / 'manifest.json').write_text(json.dumps({'images': [{'file': 'frame.png'}]}))
        for sibling in ('hypr', 'quickshell'):
            directory = self.root.parent / sibling
            directory.mkdir()
            (directory / 'fixture').write_text('stable')
        self.evidence = self.base / 'evidence'
        self.evidence.mkdir()
        self.fake_module = mock.patch.object(CI, 'module', side_effect=lambda path, _: SimpleNamespace(
            build=SimpleNamespace(sources=lambda: {'Cargo.toml': CI.digest(path.parents[1] / 'Cargo.toml')}),
            shell_sources=lambda: {}))
        self.fake_module.start()
        self.addCleanup(self.fake_module.stop)

    def test_manifest_listed_missing_image_is_rejected(self):
        (self.root / 'parity/reference/panels/frame.png').unlink()
        with self.assertRaisesRegex(ValueError, 'Missing manifest-listed image'):
            CI.copy_inputs(self.root)

    def test_temporary_live_service_instrumentation_is_not_admitted(self):
        source = self.root / 'crates/example/src/lib.rs'
        source.parent.mkdir(parents=True)
        source.write_text('pub mod idle_trace; // IDLE_TRACE_EXPORT')
        with mock.patch.object(CI, 'module', return_value=SimpleNamespace(
                build=SimpleNamespace(sources=lambda: {'crates/example/src/lib.rs': CI.digest(source)}),
                shell_sources=lambda: {})):
            with self.assertRaisesRegex(ValueError, 'Temporary service profiling'):
                CI.copy_inputs(self.root)

    def test_partial_png_is_rejected(self):
        (self.root / 'parity/reference/panels/frame.png').write_bytes(PNG[:-12])
        with self.assertRaisesRegex(ValueError, 'Incomplete PNG'):
            CI.copy_inputs(self.root)

    def test_local_rust_inputs_are_newer_without_changing_their_bytes(self):
        source = self.root / 'crates/example/tests/path.rs'
        source.parent.mkdir(parents=True)
        source.write_text('const ROOT: &str = env!("CARGO_MANIFEST_DIR");')
        os.utime(source, (1, 1))
        before = CI.digest(source)
        started = time.time_ns()
        self.assertGreater(CI.refresh_workspace_mtimes(self.root), 0)
        self.assertEqual(CI.digest(source), before)
        self.assertGreaterEqual(source.stat().st_mtime_ns, started)

    def test_new_engine_idle_regression_is_discovered_without_enabling_live_tests(self):
        source = self.root / 'crates/hyprv-ui/src/diagnostics.rs'
        source.parent.mkdir(parents=True)
        source.write_text('#[test]\nfn idle_wakeup_regression() {}\n#[test]\n#[ignore = "live compositor"]\nfn idle_poll_live() {}\n#[test]\n#[ignore = "old smoke"]\nfn shared_egl_service_wakeup_and_idle() {}\nfn idle_wake_not_a_test() {}\n')
        rows = CI.engine_idle_regressions(self.root)
        self.assertEqual([row['name'] for row in rows], ['idle_wakeup_regression', 'idle_poll_live'])
        self.assertFalse(rows[0]['ignored'])
        self.assertTrue(rows[1]['ignored'])
        self.assertEqual(rows[0]['command'][-2:], ['--lib', 'idle_wakeup_regression'])
        self.assertNotIn('--ignored', rows[1]['command'])
        assembled = self.root / 'crates/hyprv-shell/tests/idle.rs'
        assembled.parent.mkdir(parents=True)
        assembled.write_text('#[test]\nfn quiet_nested_shell_has_no_repaints_or_extra_main_dispatches() {}')
        rows = CI.engine_idle_regressions(self.root)
        self.assertEqual(rows[-1]['command'][3], 'hyprv-shell')
        self.assertIn('idle', rows[-1]['command'])

    def test_recapture_and_source_change_force_a_fresh_copy(self):
        original = CI.clone
        first = True

        def recapture(src, dst):
            nonlocal first
            original(src, dst)
            if src == self.root and first:
                first = False
                directory = self.root / 'parity/reference/panels'
                (directory / 'new.png').write_bytes(PNG)
                (directory / 'manifest.json').write_text(json.dumps({'images': [{'file': 'new.png'}]}))
                (self.root / 'Cargo.toml').write_text('generation two')

        parent = self.base / 'snapshot'
        with mock.patch.object(CI, 'ROOT', self.root), mock.patch.object(CI, 'clone', side_effect=recapture), mock.patch.object(CI, 'report'), mock.patch.object(CI.time, 'sleep'):
            self.assertTrue(CI.consistent_copy(parent, {}, self.evidence))
        rows = json.loads((self.evidence / 'copy-consistency.json').read_text())
        self.assertEqual(len(rows), 2)
        self.assertIn('Cargo.toml', rows[0]['changed_during_copy'])
        self.assertIn('parity/reference/panels/manifest.json', rows[0]['changed_during_copy'])
        self.assertEqual(rows[1]['changed_during_copy'], [])
        self.assertTrue((parent / 'rshell/parity/reference/panels/new.png').is_file())
        self.assertEqual(CI.copy_inputs(parent / 'rshell'), CI.copy_inputs(self.root))


if __name__ == '__main__':
    unittest.main()
