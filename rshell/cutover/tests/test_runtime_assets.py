"""Packaged runtime asset integrity and transaction regressions; no host install."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

b = load('assets_build', 'packaging/build_release.py')
i = load('assets_install', 'packaging/install.py')
g = load('assets_preflight', 'cutover/preflight.py')

def digest(data):
    return hashlib.sha256(data).hexdigest()

def release(root):
    files = {'release.json': json.dumps({'schema':1,'workspace':str(ROOT),'owner':__import__('pwd').getpwuid(os.getuid()).pw_name,'sources_sha256':'a'*64}).encode(), 'bin/hyprv-shell': b'binary', 'files/hyprv-shell.service': b'unit',
             'runtime/assets/test.svg': b'asset',
             'runtime/runtime-assets.json': json.dumps({'schema': 1, 'files': {'assets/test.svg': digest(b'asset')}}).encode()}
    for name, data in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    (root / i.MANIFEST).write_text(''.join(f'{digest(data)}  {name}\n' for name, data in sorted(files.items())))
    return files

class RuntimeAssets(unittest.TestCase):
    def test_release_contains_the_embedded_catalog_and_every_payload(self):
        files = b.runtime_files()
        catalog = json.loads((ROOT / 'crates/hyprv-shell/src/runtime-assets.json').read_text())
        self.assertEqual(set(files), {'runtime/runtime-assets.json', *('runtime/' + n for n in catalog['files'])})
        self.assertGreater(len(files), 2)

    def test_source_payload_corruption_blocks_staging(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'crates/hyprv-shell/src').mkdir(parents=True)
            (root / 'crates/hyprv-shell/src/runtime-assets.json').write_text(json.dumps({'schema': 1, 'files': {'assets/test.svg': digest(b'expected')}}))
            (root / 'packaging/assets/assets').mkdir(parents=True)
            (root / 'packaging/assets/assets/test.svg').write_bytes(b'corrupt')
            with mock.patch.object(b, 'ROOT', root), self.assertRaisesRegex(RuntimeError, 'catalog mismatch'):
                b.runtime_files()

    def test_tampered_release_is_rejected_before_installing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); rel = root / 'release'; release(rel)
            (rel / 'runtime/assets/test.svg').unlink()
            with mock.patch.object(i, 'candidate', return_value=rel), mock.patch.dict(os.environ, HOME=str(root / 'home')):
                with self.assertRaisesRegex(RuntimeError, 'missing files'):
                    i.Installer('user', apply=True).execute()
            self.assertFalse((root / 'home').exists())

    def test_payload_catalog_rejects_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); release(root)
            (root / 'runtime/runtime-assets.json').write_text(json.dumps({'schema': 1, 'files': {'../outside': '0' * 64}}))
            with self.assertRaisesRegex(RuntimeError, 'unsafe runtime asset'):
                i.user_files(root)

    def test_postcopy_corruption_restores_binary_unit_and_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); rel = root / 'release'; release(rel); home = root / 'home'
            files = i.user_files(rel)
            for _, name, _ in files:
                p = home / name; p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(b'previous')
            original = i.Transaction.run
            def corrupt(tx, changes, tag):
                data = original(tx, changes, tag)
                (home / '.local/share/hyprv-shell/assets/test.svg').write_bytes(b'corrupt')
                return data
            with mock.patch.object(i, 'candidate', return_value=rel), mock.patch.dict(os.environ, HOME=str(home)), \
                 mock.patch.object(i.Transaction, 'run', corrupt), mock.patch.object(i.Installer, 'systemctl') as systemctl:
                with self.assertRaisesRegex(RuntimeError, 'installed digest mismatch'):
                    i.Installer('user', apply=True).execute()
                systemctl.assert_not_called()
            for _, name, _ in files:
                self.assertEqual((home / name).read_bytes(), b'previous')

    def test_missing_assets_are_a_named_preflight_failure(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(g, 'ROOT', Path(tmp)), \
             mock.patch.object(g, 'validate_assets', side_effect=ValueError('runtime assets missing')) as validate, \
             mock.patch.object(g, 'validate_helper', return_value=True), mock.patch.object(g.parity, 'inspect', return_value=True):
            result = next(row for row in g.inspect() if row['check'] == 'runtime assets')
            self.assertFalse(result['ok']); self.assertEqual(result['detail'], 'runtime assets missing')
            validate.assert_called_once_with(False)

if __name__ == '__main__':
    unittest.main()
