#!/usr/bin/env python3
"""Actual user installer + staged ELF in disposable HOME; systemctl is a stub."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('rehearsal_installer', ROOT / 'packaging/install.py')
    installer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(installer)
    release = installer.candidate()
    rid, _ = installer.verify_release(release)
    receipt = {'release_id': rid, 'binary_sha256': installer.sha(release / 'bin/hyprv-shell'),
               'real_systemctl': False, 'host_install': False, 'checks': []}
    with tempfile.TemporaryDirectory(prefix='hyprv-assets-rehearsal-') as directory:
        scratch = Path(directory)
        home = scratch / 'home'
        home.mkdir()
        stub = scratch / 'systemctl'
        calls = scratch / 'systemctl.log'
        stub.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> ' + shlex.quote(str(calls)) + '\n'
                        'case "$*" in *is-active*) exit 3;; esac\nexit 0\n')
        stub.chmod(0o755)
        with mock.patch.dict(os.environ, HOME=str(home)), mock.patch.object(installer, 'SYSTEMCTL', str(stub)):
            installer.Installer('user', apply=True, release_id=rid).execute()
            count = calls.read_text().count('daemon-reload')
            installer.Installer('user', apply=True, release_id=rid).execute()
            assert calls.read_text().count('daemon-reload') == count == 1
            spec2 = importlib.util.spec_from_file_location('rehearsal_preflight', ROOT / 'cutover/preflight.py')
            preflight = importlib.util.module_from_spec(spec2)
            spec2.loader.exec_module(preflight)
            receipt['installed_preflight_assets'] = preflight.validate_assets(installed=True)
        unit = (home / '.config/systemd/user/hyprv-shell.service').read_text().splitlines()
        env = dict(os.environ, HOME=str(home))
        env.pop('HYPRV_ASSET_ROOT', None)
        for line in unit:
            if line.startswith('Environment='):
                for entry in shlex.split(line.split('=', 1)[1]):
                    key, value = entry.split('=', 1)
                    env[key] = value.replace('%h', str(home))
        executable = next(line.split('=', 1)[1] for line in unit if line.startswith('ExecStart='))
        executable = executable.replace('%h', str(home))
        root = Path(env['HYPRV_ASSET_ROOT'])
        assert root == home / '.local/share/hyprv-shell'

        def check(name, expected, error=None, use_default=False):
            selected = env.copy()
            if use_default:
                selected.pop('HYPRV_ASSET_ROOT', None)
            child = subprocess.run([executable, '--check-assets'], cwd='/', env=selected,
                                   text=True, capture_output=True, timeout=10)
            assert (child.returncode == 0) == expected, (name, child.stdout, child.stderr)
            if error:
                assert error in child.stderr, child.stderr
            receipt['checks'].append({'name': name, 'returncode': child.returncode,
                                      'stdout': child.stdout, 'stderr': child.stderr, 'passed': True})

        check('installed-unit-root', True)
        check('default-root-without-override', True, use_default=True)
        catalog = json.loads((root / 'runtime-assets.json').read_text())
        asset = root / next(iter(catalog['files']))
        original = asset.read_bytes()
        asset.unlink()
        check('missing-asset-fails-with-checkout-still-present', False, 'runtime assets:')
        asset.write_bytes(original + b'corrupt')
        check('corrupt-asset-fails', False, 'digest mismatch')
        asset.write_bytes(original)
        path = root / 'runtime-assets.json'
        original = path.read_bytes()
        path.unlink()
        check('missing-catalog-fails', False, 'runtime-assets.json')
        path.write_bytes(original)
        check('restored-install', True)
        receipt['payload_files'] = len(catalog['files'])
        receipt['systemctl_calls'] = calls.read_text().splitlines()
        assert all(line.startswith('--user ') for line in receipt['systemctl_calls'])
    receipt['scratch_home_removed'] = True
    receipt['status'] = 'pass'
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')
    print('PASS: installed unit resolves hash-verified assets in scratch HOME; missing/corrupt payload fails closed')


if __name__ == '__main__':
    main()
