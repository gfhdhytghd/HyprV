#!/usr/bin/env python3
"""Render a staged ELF only in private compositors; publish strict parity evidence."""
import argparse
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def collect(root, run, binary, sources):
    consumer = load('fullscreen_consumer', root / 'cutover/parity_gate.py')
    policy = consumer.policy
    comparer = load('fullscreen_comparer', root / 'parity/fullscreen/tools/compare.py')
    corners = load('fullscreen_corners', root / 'crates/hyprv-shell/tools/verify_corners.py').verify(run)
    matrix = json.loads((run / 'fullscreen-results.json').read_text())
    if not matrix.get('jobs') or any(j.get('runtime_ok') is not True for j in matrix['jobs']):
        raise ValueError('assembled matrix has missing/failed jobs')
    entries = json.loads((root / 'parity/fullscreen/reference/manifest.json').read_text())['images']
    images = []
    for job in matrix['jobs']:
        manifest = Path(job['manifest'])
        data = json.loads(manifest.read_text())
        base = manifest.parent / Path(data['capture_manifest']).parent
        if data.get('error') or not data.get('captures'):
            raise ValueError('assembled job has no successful captures: ' + str(manifest))
        for capture in data['captures']:
            matches = [e for e in entries if all(e.get(k, '') == capture.get(k, '') for k in ('frame', 'theme', 'scale'))
                       and e['state'] == capture['fixture'] and e['output']['name'] == capture['output']]
            if len(matches) != 1:
                raise ValueError('assembled capture does not uniquely match a canonical reference')
            entry = matches[0]
            reference = root / 'parity/fullscreen/reference' / entry['file']
            if Path(capture['comparison']['reference']).resolve() != reference.resolve():
                raise ValueError('assembled capture used a different reference')
            actual = base / capture['file']
            gate = comparer.gate(comparer.premul(actual), comparer.premul(reference),
                                 entry.get('distance_field_text', False), entry['scale'])
            native = capture.get('comparison', {})
            if native.get('pass') is not True:
                gate['pass'] = False
                gate['native_failure'] = native.get('status', 'missing native comparison')
            crops = [r for r in corners['rows'] if r['file'] == capture['file'] and r['output'] == capture['output']
                     and r['scale'] == capture['scale']]
            if len(crops) != 4:
                raise ValueError('ambiguous/missing exact corner results')
            images.append({'reference': entry['file'], 'actual': str(actual), 'actual_sha256': policy.sha(actual),
                           'gate': gate, 'native': native, 'corners': crops})
    return {'schema': 1, 'status': 'complete', 'backend': 'production-gl', 'epoch': time.time(), 'sources': sources, 'binary_sha256': policy.sha(binary),
            'images': images, 'masked_pixels': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'packaging/dist/hyprv-shell')
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=ROOT / 'cutover/evidence/fullscreen.json')
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with (args.out.parent / '.fullscreen.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another assembled verification is running') from error
        execute(args)


def execute(args):
    run, binary = args.run.resolve(), args.binary.resolve()
    if run.exists() and any(run.iterdir()):
        raise ValueError('use an empty private capture directory')
    gate = load('fullscreen_gate', ROOT / 'cutover/parity_gate.py')
    before, binary_hash = gate.policy.inputs(ROOT, assembled=True), gate.policy.sha(binary)
    env = dict(os.environ, TMPDIR='/tmp')
    env['HYPRV_ASSET_ROOT'] = str(ROOT / 'packaging/dist/release/runtime')
    for key in ('HYPRV_STATS_SHM_EXPERIMENT', 'HYPRV_SHM_PRESENT', 'HYPRV_UI_PROFILE', 'HYPRV_SERVICE_TRACE'):
        env.pop(key, None)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps({'schema': 1, 'status': 'running', 'sources': before, 'binary_sha256': binary_hash}) + '\n')
    command = [str(binary), '--nested-test', str(run), '--fixtures', str(ROOT / 'parity/fullscreen/fixtures'),
               '--references', str(ROOT / 'parity/fullscreen/reference'), '--capture-only']
    subprocess.run(command, cwd=ROOT, env=env, check=True, timeout=1800)
    data = collect(ROOT, run, binary, before)
    if before != gate.policy.inputs(ROOT, assembled=True) or binary_hash != gate.policy.sha(binary):
        raise ValueError('assembled source or ELF changed during rendering/scoring')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(data, indent=2) + '\n')
    gate.fullscreen_gate(ROOT, data)
    print('PASS: complete assembled matrix, strict pixels and exact unmasked corners')


if __name__ == '__main__':
    main()
