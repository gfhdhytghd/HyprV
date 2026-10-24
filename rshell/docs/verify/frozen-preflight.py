#!/usr/bin/env python3
"""One frozen, non-installing release-candidate verification after owner handoff."""
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
STAMP = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
OUT = ROOT / 'docs/verify/raw/final-candidate' / STAMP
PARENT = Path('/tmp') / ('hrc-' + str(os.getpid()))
FROZEN = PARENT / 'rshell'
ARCHIVE = ROOT / 'packaging/.build' / ('final-candidate-' + STAMP)
ENV = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target'), CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='4',
           PYTHONDONTWRITEBYTECODE='1', TMPDIR='/tmp')
for key in ('HYPRV_STATS_SHM_EXPERIMENT', 'HYPRV_SHM_PRESENT', 'HYPRV_UI_PROFILE', 'HYPRV_SERVICE_TRACE',
            'HYPRV_PARITY_BACKEND', 'HYPRV_BAR_ACTUAL_DIR', 'HYPRV_BAR_PARITY_REPORT_DIR',
            'HYPRV_IDLE_TEST', 'HYPRV_IDLE_RAW_STATS_CONTROL', 'HYPRV_FUNCTIONAL_TEST',
            'HYPRV_FUNCTIONAL_FILTER', 'HYPRV_SHELL_DIAGNOSTIC', 'HYPRV_SHELL_BUILD_PROFILE'):
    ENV.pop(key, None)
STATE = {'started': dt.datetime.now(dt.timezone.utc).isoformat(), 'snapshot': str(FROZEN),
         'archive': str(ARCHIVE), 'phases': {}, 'installations': False}


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def save():
    (OUT / 'result.json').write_text(json.dumps(STATE, indent=2) + '\n')


def run(name, command, retry=False, env=None, cwd=FROZEN, timeout=1800):
    row = {'command': command, 'attempts': []}
    for attempt in range(2 if retry else 1):
        log = OUT / (name.replace(' ', '-') + f'-{attempt+1}.log')
        print(dt.datetime.now(dt.timezone.utc).isoformat(), 'RUN', name, flush=True)
        if (command[0] == 'cargo' or command[0] == 'packaging/build-release.sh') and shutil.disk_usage('/').free < 30 * 1024**3:
            log.write_text('INFRASTRUCTURE: root free space below 30 GiB\n')
            code = 125
        else:
            with log.open('w') as output:
                try:
                    child = subprocess.Popen(command, cwd=cwd, env=env or ENV, stdout=output,
                                             stderr=subprocess.STDOUT, start_new_session=True)
                    code = child.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGTERM)
                    try:
                        child.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                    code = 124
        row['attempts'].append({'code': code, 'log': log.name})
        row['code'] = code
        STATE['phases'][name] = row
        save()
        print(dt.datetime.now(dt.timezone.utc).isoformat(), 'DONE', name, code, flush=True)
        if not code:
            break
    return row


def freeze(ci):
    attempts = []
    while True:
        try:
            before = ci.copy_inputs(ROOT)
            siblings = {name: ci.manifest(ROOT.parent / name) for name in ('hypr', 'quickshell')}
            ci.clone(ROOT, FROZEN)
            for name in siblings:
                ci.clone(ROOT.parent / name, PARENT / name)
            copied = ci.copy_inputs(FROZEN)
            after = ci.copy_inputs(ROOT)
            changed = [name for name in set(before) | set(copied) | set(after)
                       if before.get(name) != copied.get(name) or copied.get(name) != after.get(name)]
            for name, values in siblings.items():
                if values != ci.manifest(ROOT.parent / name) or values != ci.manifest(PARENT / name):
                    changed.append(name + '/')
            attempts.append({'changed_inputs': sorted(changed)})
            if not changed:
                receipt = {'rshell': ci.manifest(FROZEN), **siblings}
                STATE['fingerprint'] = hashlib.sha256(json.dumps(receipt, sort_keys=True).encode()).hexdigest()
                (OUT / 'snapshot-manifest.json').write_text(json.dumps(receipt, indent=2) + '\n')
                (OUT / 'copy-consistency.json').write_text(json.dumps(attempts, indent=2) + '\n')
                return
        except (ValueError, OSError, KeyError) as error:
            attempts.append({'incomplete_inputs': str(error)})
        (OUT / 'copy-consistency.json').write_text(json.dumps(attempts, indent=2) + '\n')
        shutil.rmtree(PARENT, ignore_errors=True)
        time.sleep(5)


def main():
    OUT.mkdir(parents=True)
    save()
    ci = module('frozen_ci', ROOT / 'docs/verify/ci-snapshot-loop.py')
    handoffs = {name: {'sha256': ci.digest(ROOT / name), 'tail': (ROOT / name).read_text()[-6000:]} for name in ('docs/requests/hyprv-svc-desktop.md', 'docs/requests/hyprv-ui-bar.md', 'docs/requests/hyprv-ui.md')}
    (OUT / 'owner-handoffs.json').write_text(json.dumps(handoffs, indent=2) + '\n')
    freeze(ci)
    gate = module('frozen_preflight', FROZEN / 'cutover/preflight.py')
    before = gate.build.sources()
    ci.refresh_workspace_mtimes(FROZEN)
    save()
    stage = run('stage release', ['packaging/build-release.sh'], retry=True)
    if stage['code'] == 0:
        ENV['HYPRV_ASSET_ROOT'] = str(FROZEN / 'packaging/dist/release/runtime')
    tests = []
    for i, command in enumerate(gate.test_commands()):
        row = run(f'test suite {i}', command, retry=True)
        tests.append({'command': command, 'returncode': row['code'], 'attempts': row['attempts']})
    receipt_dir = FROZEN / 'cutover/evidence'
    receipt_dir.mkdir(exist_ok=True)
    (receipt_dir / 'tests.json').write_text(json.dumps({'schema': 1, 'epoch': time.time(), 'sources': before,
        'results': tests, 'status': 'pass' if all(r['returncode'] == 0 for r in tests) and before == gate.build.sources() else 'fail'}, indent=2) + '\n')
    run('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'], retry=True)
    run('GL components', ['python3', '-B', 'parity/verify/verify.py', '--backend', 'gl'])
    if stage['code'] == 0:
        ARCHIVE.mkdir(parents=True)
        shutil.copytree(FROZEN / 'packaging/dist', ARCHIVE / 'dist')
        binary = ARCHIVE / 'dist/hyprv-shell'
        STATE['binary_snapshot_marker'] = str(FROZEN / 'crates/hyprv-shell')
        if STATE['binary_snapshot_marker'].encode() not in binary.read_bytes():
            raise RuntimeError('shared-target ELF lacks this frozen source marker; restage before validation')
        run('scratch HOME assets', ['python3', '-B', 'packaging/rehearse-assets.py',
                                   '--out', str(receipt_dir / 'assets-rehearsal.json')])
        run('pinned functional replay', [str(binary), '--nested-functional-test',
                                      str(receipt_dir / 'candidate-functional')])
        run('pinned idle regression', [str(binary), '--nested-idle-test',
                                      str(receipt_dir / 'candidate-idle')])
        run('assembled full-screen', ['python3', '-B', 'cutover/verify_fullscreen.py', '--binary', str(binary),
                                     '--run', str(receipt_dir / 'fullscreen-run')])
        inputs = ARCHIVE / 'performance-inputs.json'
        build = json.loads((FROZEN / 'packaging/dist/build.json').read_text())
        inputs.write_text(json.dumps({'command': build['command'], 'source_root': str(FROZEN),
              'files': gate.shell_sources(), 'ci_snapshot_fingerprint': STATE['fingerprint']}, indent=2) + '\n')
        perf_env = dict(ENV, HYPRV_SHELL_BINARY=str(binary), HYPRV_SHELL_SOURCE_ROOT=str(FROZEN),
                        HYPRV_SHELL_INPUTS_JSON=str(inputs), HYPRV_SHELL_LEGACY_SOURCE=str(ROOT.parent / 'quickshell'))
        tag = 'rc-' + STAMP.lower()
        for suffix, options in [('quiet', ['--quiet-agents', '--idle-only']), ('no-hooks', ['--quiet-hooks', '--idle-only']), ('live', [])]:
            run('performance ' + suffix, ['python3', '-B', 'docs/verify/measure-shell.py', '--label', tag + '-' + suffix, *options], env=perf_env)
        run('performance report', ['python3', '-B', 'docs/verify/report-shell.py', '--tag', tag])
    row = run('preflight', ['cutover/preflight.sh', '--json'])
    STATE['preflight'] = json.loads((OUT / row['attempts'][-1]['log']).read_text())
    after = gate.build.sources()
    STATE['changed_snapshot_sources'] = [name for name in set(before) | set(after) if before.get(name) != after.get(name)]
    STATE['root_sources_changed'] = before != module('moving_build', ROOT / 'packaging/build_release.py').sources()
    STATE['moving_parity_inputs_changed'] = gate.parity.policy.inputs(FROZEN, assembled=True) != gate.parity.policy.inputs(ROOT, assembled=True)
    STATE['finished'] = dt.datetime.now(dt.timezone.utc).isoformat()
    if ARCHIVE.exists():
        # Preserve the exact gate inputs, not hundreds of diagnostic raw EGL dumps.
        image_paths = set()
        full = receipt_dir / 'fullscreen.json'
        if full.exists():
            (OUT / 'fullscreen-receipt-original.json').write_bytes(full.read_bytes())
            image_paths.update(Path(r['actual']) for r in json.loads(full.read_text()).get('images', []))
        matrix = receipt_dir / 'fullscreen-run/fullscreen-results.json'
        if matrix.exists():
            for job in json.loads(matrix.read_text()).get('jobs', []):
                manifest = Path(job['manifest'])
                if manifest.is_file():
                    detail = json.loads(manifest.read_text())
                    base = manifest.parent / Path(detail.get('capture_manifest', 'manifest.json')).parent
                    image_paths.update(base / r['file'] for r in detail.get('captures', []))
        for relative in ('cutover/evidence', 'parity/verify/out', 'docs/verify/raw/perf-shell'):
            source = FROZEN / relative
            for path in source.rglob('*'):
                keep_png = path in image_paths or (relative == 'parity/verify/out' and path.parent.name == 'actual')
                if path.is_file() and (path.suffix in ('.json', '.log', '.csv', '.lock') or path.suffix == '.png' and keep_png):
                    target = ARCHIVE / relative / path.relative_to(source)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, target)
        for relative in ('parity/reference', 'parity/fullscreen/reference', 'parity/fullscreen/fixtures'):
            ci.clone(FROZEN / relative, ARCHIVE / relative)
        for relative in ('parity/fullscreen/tools/compare.py', 'crates/hyprv-shell/tools/verify_corners.py',
                         'docs/verify/perf-shell.md', 'docs/verify/raw/perf-shell-latest.json', 'docs/verify/parity.md'):
            source = FROZEN / relative
            if source.exists():
                target = ARCHIVE / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
        # Only artifact paths are relocated. Original source provenance/commands
        # remain in OUT and the copied build/test/performance receipts.
        relocatable = list((ARCHIVE / 'cutover/evidence/fullscreen-run').rglob('*.json'))
        relocatable += [ARCHIVE / 'cutover/evidence/fullscreen.json']
        for path in relocatable:
            if path.is_file():
                path.write_text(path.read_text().replace(str(FROZEN), str(ARCHIVE)))
        (ARCHIVE / 'artifact-path-relocation.json').write_text(json.dumps({'from': str(FROZEN), 'to': str(ARCHIVE),
            'original_fullscreen_receipt': str(OUT / 'fullscreen-receipt-original.json'),
            'removed': 'unreferenced diagnostic raw EGL PNGs and source checkout; all actual/reference gate images retained'}, indent=2) + '\n')
    save()
    shutil.rmtree(PARENT)
    STATE['snapshot_removed'] = True
    save()
    print('FINISHED', json.dumps(STATE['preflight']), flush=True)


if __name__ == '__main__':
    main()
