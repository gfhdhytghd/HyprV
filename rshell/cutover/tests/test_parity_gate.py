"""FR-NET-2: production backend, content provenance, exclusions and assembly."""
import copy
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('strict_parity', ROOT / 'cutover/parity_gate.py')
G = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G)


class Parity(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix='parity-gate-')
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / 'Cargo.toml').write_text('[workspace]')
        (self.root / 'Cargo.lock').write_text('lock')
        for name in ('hyprv-ui', 'hyprv-widgets', 'hyprv-core'):
            p = self.root / 'crates' / name / 'src/lib.rs'
            p.parent.mkdir(parents=True)
            p.write_text('original dependency')
        self.data = {'results': {}, 'renders': {}}
        for name in ('bar', 'island', 'panels'):
            entries = [{'file': 'idle.png', 'state': 'idle', 'distance_field_text': False}]
            if name == 'bar':
                for state in (*sorted(G.policy.EXCLUDED['bar']), *G.policy.DELEGATED):
                    for theme in ('light', 'dark'):
                        for scale in (1, 2):
                            entries.append({'file': f'{state}--{theme}@{scale}.png', 'state': state,
                                            'distance_field_text': True, 'theme': theme, 'scale': scale})
            self.write(f'parity/reference/{name}/manifest.json', {'images': entries})
            rows = []
            for entry in entries:
                self.png(f'parity/reference/{name}/' + entry['file'])
                self.png(f'parity/verify/out/{name}-gl/actual/' + entry['file'])
                excluded = entry['state'] in G.policy.EXCLUDED.get(name, ())
                rows.append({'file': entry['file'], 'state': entry['state'], 'df': entry['distance_field_text'],
                             'result': 'excluded' if excluded else 'pass', 'cause': 'live-unreachable' if excluded else '-',
                             'max': 0, 'mean': 0, 'structural': False})
            self.data['results'][name + '-gl'] = rows
            self.data['renders'][name + '-gl'] = {'status': 'ok', 'exit': 0, 'backend': 'gl'}
        fs_entries = []
        fs_images = []
        for state in G.policy.DELEGATED.values():
            self.write(f'parity/fullscreen/fixtures/{state}.json', {
                'scene': 'desktop.qml', 'outputs': [{'name': 'DP-test', 'scale': 2}], 'steps': [{'capture': ''}]})
            for theme in ('light', 'dark'):
                file = f'{state}--{theme}@2.png'
                fs_entries.append({'file': file, 'state': state, 'frame': '', 'theme': theme,
                                   'scale': 2, 'output': {'name': 'DP-test'}, 'distance_field_text': True})
                self.png('parity/fullscreen/reference/' + file)
                actual = self.png('cutover/evidence/fullscreen-run/' + file)
                fs_images.append({'reference': file, 'actual': str(actual), 'actual_sha256': G.policy.sha(actual),
                                  'native': {'pass': True},
                                  'gate': {'pass': True, 'max_channel_diff': 0, 'painted_mae': 0,
                                           'footprint_ok': True, 'painted_iou': 1},
                                  'corners': [{'corner': corner, 'max_channel_diff': 0, 'different_pixels': 0, 'crop': crop}
                                              for corner, crop in {'top-left': [0, 0, 54, 84], 'top-right': [146, 0, 54, 84],
                                                                   'bottom-left': [0, 146, 54, 54], 'bottom-right': [146, 146, 54, 54]}.items()]})
        self.write('parity/fullscreen/reference/manifest.json', {'images': fs_entries})
        self.write('packaging/dist/build.json', {'binaries': {'hyprv-shell': 'pinned-elf'}})
        self.full = {'schema': 1, 'status': 'complete', 'backend': 'production-gl', 'masked_pixels': 0,
                     'binary_sha256': 'pinned-elf', 'images': fs_images}
        (self.root / 'parity/verify/out/.verify.lock').touch()
        (self.root / 'cutover/evidence/.fullscreen.lock').touch()
        self.seal()

    def write(self, name, value):
        p = self.root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(value))
        return p

    def png(self, name):
        p = self.root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        Image.new('RGBA', (200, 200), (0, 0, 0, 0)).save(p)
        return p

    def publish(self):
        self.write('parity/verify/out/results.json', self.data)
        self.write('cutover/evidence/fullscreen.json', self.full)
        report = self.root / 'docs/verify/parity.md'
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text('generated component parity report')

    def seal(self):
        for name, render in self.data['renders'].items():
            render['sources'] = G.policy.inputs(self.root)
            render['actual_sha256'] = {p.name: G.policy.sha(p) for p in
                                       (self.root / 'parity/verify/out' / name / 'actual').glob('*.png')}
        self.full['sources'] = G.policy.inputs(self.root, assembled=True)
        self.publish()

    def rejects(self):
        self.publish()
        with self.assertRaises((ValueError, OSError)):
            G.inspect(self.root)

    def test_gl_all_images_and_assembly_pass_with_only_canonical_battery_exclusions(self):
        self.assertIn('production GL', G.inspect(self.root))

    def test_cpu_only_results_cannot_substitute_for_gl(self):
        self.data['renders']['bar'] = self.data['renders'].pop('bar-gl')
        self.data['results']['bar'] = self.data['results'].pop('bar-gl')
        self.rejects()

    def test_gl_slot_cannot_relabel_cpu_backend(self):
        self.data['renders']['bar-gl']['backend'] = 'cpu'
        self.rejects()

    def test_stale_human_readable_report_cannot_claim_readiness(self):
        os.utime(self.root / 'docs/verify/parity.md', (1, 1))
        with self.assertRaisesRegex(ValueError, 'report not regenerated'):
            G.inspect(self.root)

    def test_in_progress_component_or_assembled_writer_cannot_be_accepted(self):
        for name in ('parity/verify/out/.verify.lock', 'cutover/evidence/.fullscreen.lock'):
            with self.subTest(lock=name), (self.root / name).open('a') as handle:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaisesRegex(ValueError, 'still in progress'):
                    G.inspect(self.root)

    def test_source_change_during_inspection_cannot_leave_green_evidence(self):
        real_image = G.image
        changed = False

        def mutate(path):
            nonlocal changed
            size = real_image(path)
            if 'fullscreen-run' in path.parts and not changed:
                changed = True
                (self.root / 'crates/hyprv-ui/src/lib.rs').write_text('changed during full-screen inspection')
            return size

        with mock.patch.object(G, 'image', side_effect=mutate):
            with self.assertRaisesRegex(ValueError, 'changed during parity gate'):
                G.inspect(self.root)

    def test_all_eight_host_references_are_mandatory_even_when_manifest_is_partial(self):
        path = self.root / 'parity/reference/bar/manifest.json'
        manifest = json.loads(path.read_text())
        removed = manifest['images'].pop()
        self.write('parity/reference/bar/manifest.json', manifest)
        self.data['results']['bar-gl'] = [r for r in self.data['results']['bar-gl'] if r['file'] != removed['file']]
        (self.root / 'parity/verify/out/bar-gl/actual' / removed['file']).unlink()
        self.seal()
        self.rejects()

    def test_transitive_dependency_and_lock_changes_with_preserved_mtime_are_stale(self):
        for name in ('crates/hyprv-ui/src/lib.rs', 'crates/hyprv-widgets/src/lib.rs',
                     'crates/hyprv-core/src/lib.rs', 'Cargo.lock'):
            with self.subTest(name=name):
                path = self.root / name
                old, info = path.read_bytes(), path.stat()
                path.write_bytes(old + b' changed')
                os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns))
                self.rejects()
                path.write_bytes(old)

    def test_fixture_or_reference_change_invalidates_every_gl_slot(self):
        self.write('parity/fixtures/bar/new.json', {'new': True})
        self.rejects()

    def test_runtime_quickshell_assets_and_original_qml_are_dependencies(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / 'rshell'
            shutil.copytree(self.root, root)
            legacy = base / 'quickshell'
            (legacy / 'assets').mkdir(parents=True)
            image = legacy / 'assets/icon.svg'
            image.write_text('<svg/>')
            qml = legacy / 'shell.qml'
            qml.write_text('original QML')
            before = G.policy.inputs(root)
            self.assertIn('../quickshell/assets/icon.svg', before)
            self.assertIn('../quickshell/shell.qml', before)
            image.write_text('<svg changed="true"/>')
            self.assertNotEqual(G.policy.inputs(root), before)

    def test_external_test_oracles_are_also_dependency_inputs(self):
        for name in ('fixtures/flickable-implementation/cases.rs',
                     'fixtures/panels-flickable/scroller.csv', 'experiments/text/df/df-jb.json'):
            with self.subTest(name=name):
                path = self.root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('new oracle')
                self.rejects()
                path.unlink()

    def test_build_test_receipts_track_external_oracles_but_not_generated_results(self):
        spec = importlib.util.spec_from_file_location('build_inputs_test', ROOT / 'packaging/build_release.py')
        build = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(build)
        with mock.patch.object(build, 'ROOT', self.root):
            before = build.sources()
            self.write('parity/verify/out/results.json', {'generated': 'new render'})
            self.png('parity/verify/out/panels-gl/actual/new-output.png')
            self.assertEqual(build.sources(), before)
            path = self.root / 'fixtures/flickable-implementation/cases.rs'
            path.parent.mkdir(parents=True)
            path.write_text('externally stored regression code')
            self.assertNotEqual(build.sources(), before)
            self.assertIn(str(path.relative_to(self.root)), build.sources())

    def test_missing_rendered_images_include_excluded_battery_and_all_eight_hosts(self):
        base = self.root / 'parity/verify/out/bar-gl/actual'
        paths = [p for p in base.glob('*.png') if p.name != 'idle.png']
        self.assertEqual(len(paths), 20)
        for path in paths:
            with self.subTest(file=path.name):
                content = path.read_bytes()
                path.unlink()
                self.rejects()
                path.write_bytes(content)

    def test_missing_reference_cannot_be_sealed_as_valid(self):
        (self.root / 'parity/reference/bar/idle.png').unlink()
        self.seal()
        self.rejects()

    def test_changed_actual_and_incomplete_image_hashes_fail(self):
        path = self.root / 'parity/verify/out/bar-gl/actual/idle.png'
        Image.new('RGBA', (200, 200), (0, 0, 0, 1)).save(path)
        self.rejects()
        self.data['renders']['bar-gl']['actual_sha256'].pop('idle.png')
        self.rejects()

    def test_non_text_one_level_cannot_be_accepted_by_forging_df_or_excluded(self):
        row = self.data['results']['bar-gl'][0]
        row.update(max=0, mean=.0001)
        self.rejects()
        row.update(max=1, mean=.001)
        self.rejects()
        row['df'] = True
        self.rejects()
        row.update(df=False, result='excluded', cause='live-unreachable')
        self.rejects()

    def test_invalid_canonical_text_flag_cannot_enable_tolerance(self):
        path = self.root / 'parity/reference/island/manifest.json'
        manifest = json.loads(path.read_text())
        manifest['images'][0]['distance_field_text'] = 1
        self.write('parity/reference/island/manifest.json', manifest)
        self.data['results']['island-gl'][0].update(df=1, max=1, mean=.001)
        self.seal()
        self.rejects()

    def test_render_drift_and_missing_duplicate_or_aborted_results_fail(self):
        original = copy.deepcopy(self.data)
        for case in ('drift', 'missing', 'duplicate', 'aborted'):
            with self.subTest(case=case):
                self.data = copy.deepcopy(original)
                if case == 'drift': self.data['renders']['island-gl']['inputs_changed'] = True
                if case == 'missing': self.data['results']['island-gl'] = []
                if case == 'duplicate': self.data['results']['island-gl'] *= 2
                if case == 'aborted': self.data['renders']['island-gl']['exit'] = 101
                self.rejects()

    def test_delegated_hosts_cannot_pass_without_complete_assembled_matrix(self):
        self.full['images'].pop()
        self.rejects()

    def test_partial_fullscreen_reference_manifest_cannot_shrink_required_coverage(self):
        path = self.root / 'parity/fullscreen/reference/manifest.json'
        data = json.loads(path.read_text())
        data['images'].pop()
        self.write('parity/fullscreen/reference/manifest.json', data)
        self.full['images'].pop()
        self.seal()
        self.rejects()

    def test_current_real_replay_matrix_matches_all_canonical_reference_keys(self):
        entries = json.loads((ROOT / 'parity/fullscreen/reference/manifest.json').read_text())['images']
        keys = {(e['state'], e.get('frame', ''), e['theme'], float(e['scale']), e['output']['name']) for e in entries}
        self.assertEqual(G.fullscreen_keys(ROOT), keys)

    def test_assembled_missing_actual_or_duplicate_and_elf_mismatch_fail(self):
        original = copy.deepcopy(self.full)
        self.full['images'][0]['actual'] += '.missing'
        self.rejects()
        self.full = copy.deepcopy(original)
        self.full['images'][-1] = self.full['images'][0]
        self.rejects()
        self.full = copy.deepcopy(original)
        self.full['binary_sha256'] = 'different-release'
        self.rejects()

    def test_assembled_fixture_or_dependency_change_is_stale(self):
        self.write('parity/fullscreen/fixtures/calendar.json', {'changed': True})
        self.rejects()

    def test_assembled_runner_edits_invalidate_assembled_evidence_only(self):
        path = self.root / 'cutover/verify_fullscreen.py'
        path.write_text('original assembled producer')
        component = G.policy.inputs(self.root)
        assembled = G.policy.inputs(self.root, assembled=True)
        path.write_text('changed assembled producer')
        self.assertEqual(G.policy.inputs(self.root), component)
        self.assertNotEqual(G.policy.inputs(self.root, assembled=True), assembled)

    def test_exact_unmasked_corners_fail_even_inside_global_text_tolerance(self):
        self.full['images'][0]['gate'].update(max_channel_diff=1, painted_mae=.001)
        self.full['images'][0]['corners'][0].update(max_channel_diff=1, different_pixels=1)
        self.rejects()

    def test_corner_crops_cannot_be_shrunk_or_relocated_to_avoid_pixels(self):
        self.full['images'][0]['corners'][0]['crop'] = [1, 1, 1, 1]
        self.rejects()

    def test_assembled_non_text_must_be_exact(self):
        manifest = self.root / 'parity/fullscreen/reference/manifest.json'
        data = json.loads(manifest.read_text())
        data['images'][0]['distance_field_text'] = False
        self.write('parity/fullscreen/reference/manifest.json', data)
        self.seal()
        self.full['images'][0]['gate'].update(max_channel_diff=1, painted_mae=.001)
        self.rejects()

    def test_independent_pass_cannot_hide_a_failed_native_comparison(self):
        self.full['images'][0]['native']['pass'] = False
        self.rejects()

    def test_assembled_masks_and_non_gl_backends_are_ineligible(self):
        for key, value in (('masked_pixels', 1), ('backend', 'cpu')):
            with self.subTest(key=key):
                original = self.full[key]
                self.full[key] = value
                self.rejects()
                self.full[key] = original

    def test_fullscreen_producer_scores_real_pixels_and_cannot_tolerate_corners(self):
        def load(name, path):
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module

        producer = load('fullscreen_producer_test', ROOT / 'cutover/verify_fullscreen.py')
        comparer = load('independent_comparer_test', ROOT / 'parity/fullscreen/tools/compare.py')
        corners = load('independent_corners_test', ROOT / 'crates/hyprv-shell/tools/verify_corners.py')
        run = self.root / 'cutover/evidence/native-run'
        jobs = []
        actuals = []
        entries = json.loads((self.root / 'parity/fullscreen/reference/manifest.json').read_text())['images']
        for entry in entries:
            directory = run / entry['file'].removesuffix('.png')
            actual = directory / 'capture' / entry['file']
            actual.parent.mkdir(parents=True)
            reference = self.root / 'parity/fullscreen/reference' / entry['file']
            image = Image.new('RGBA', (200, 200), (0, 0, 0, 80))
            image.save(reference)
            image.save(actual)
            actuals.append(actual)
            capture = {'fixture': entry['state'], 'file': entry['file'], 'frame': '', 'theme': entry['theme'],
                       'scale': 2, 'output': 'DP-test', 'comparison': {'reference': str(reference), 'pass': True}}
            manifest = directory / 'manifest.json'
            manifest.write_text(json.dumps({'capture_manifest': 'capture/manifest.json', 'captures': [capture]}))
            jobs.append({'manifest': str(manifest), 'runtime_ok': True})
        (run / 'fullscreen-results.json').write_text(json.dumps({'jobs': jobs}))
        binary = self.root / 'packaging/dist/hyprv-shell'
        binary.write_bytes(b'\x7fELFtest-pinned')
        self.write('packaging/dist/build.json', {'binaries': {'hyprv-shell': G.policy.sha(binary)}})
        sources = G.policy.inputs(self.root, assembled=True)
        with mock.patch.object(producer, 'load', side_effect=lambda name, path: {
                'fullscreen_consumer': G, 'fullscreen_comparer': comparer, 'fullscreen_corners': corners}[name]):
            evidence = producer.collect(self.root, run, binary, sources)
            self.assertTrue(G.fullscreen_gate(self.root, evidence))
            changed = Image.open(actuals[0]).convert('RGBA')
            changed.putpixel((0, 0), (0, 0, 0, 81))
            changed.save(actuals[0])
            evidence = producer.collect(self.root, run, binary, sources)
            self.assertTrue(evidence['images'][0]['gate']['pass'], 'broad text tolerance is intentionally insufficient')
            with self.assertRaisesRegex(ValueError, 'strict gate red'):
                G.fullscreen_gate(self.root, evidence)


if __name__ == '__main__':
    unittest.main()
