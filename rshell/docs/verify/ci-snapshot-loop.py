#!/usr/bin/env python3
"""Two-hour snapshot CI. No installation, cutover, or live service actions."""
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target'
START = dt.datetime.fromisoformat(os.environ.get('HYPRV_CI_START', dt.datetime.now(dt.timezone.utc).isoformat())).timestamp()
DEADLINE = START + 7200
OUT = ROOT / 'docs/verify/raw/ci-snapshots'
OUT.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), PYTHONDONTWRITEBYTECODE='1', CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='4')
TEMP = Path('/tmp/hyprv-ci-sys-tmp')
TEMP.mkdir(mode=0o700, exist_ok=True)
# Unix compositor sockets include the temp path and a long instance signature.
# A nested TMPDIR can exceed sockaddr_un even when filesystem space is ample.
ENV['TMPDIR'] = '/tmp'
EXEMPT = {'privhelper installed', 'parity gate'}

def utc(t=None):
    return dt.datetime.fromtimestamp(t or time.time(), dt.timezone.utc).isoformat(timespec='seconds')

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def clone(src, dst):
    """Independent COW files, never hard links. Skip target caches at every depth."""
    dst.mkdir(parents=True, exist_ok=True)
    for base, dirs, files in os.walk(src, followlinks=False):
        dirs[:] = sorted(d for d in dirs if d not in ('target', '.git'))
        relative = Path(base).relative_to(src)
        for name in list(dirs) + files:
            old, new = Path(base) / name, dst / relative / name
            try:
                info = old.lstat()
                if stat.S_ISLNK(info.st_mode):
                    link = os.readlink(old)
                    if os.path.isabs(link):
                        try:
                            link = os.path.relpath(dst / Path(link).relative_to(src), new.parent)
                        except ValueError:
                            pass
                    new.symlink_to(link)
                    if name in dirs:
                        dirs.remove(name)
                elif stat.S_ISDIR(info.st_mode):
                    new.mkdir(exist_ok=True)
                elif stat.S_ISREG(info.st_mode):
                    with old.open('rb') as inp, new.open('wb') as out:
                        fcntl.ioctl(out.fileno(), 0x40049409, inp.fileno())
                    shutil.copystat(old, new)
            except FileNotFoundError:
                # A concurrently removed generated/live file is absent in this snapshot.
                continue

def manifest(root):
    rows = {}
    for base, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = sorted(d for d in dirs if d not in ('target', '.git'))
        for name in sorted(files + [d for d in dirs if (Path(base) / d).is_symlink()]):
            p = Path(base) / name
            if p.is_symlink():
                rows[str(p.relative_to(root))] = {'symlink': os.readlink(p)}
            elif p.is_file():
                rows[str(p.relative_to(root))] = {'sha256': digest(p), 'mode': stat.S_IMODE(p.stat().st_mode)}
    return rows

def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value

def copy_inputs(root):
    """Hash build/test inputs and complete manifest-listed reference generations."""
    gate = module(root / 'cutover/preflight.py', 'copy_gate_' + str(time.time_ns()))
    rows = gate.build.sources()
    rows.update(gate.shell_sources())
    for name in rows:
        if name.startswith('crates/') and name.endswith('.rs'):
            content = (root / name).read_text()
            if 'IDLE_TRACE_START' in content or 'IDLE_TRACE_EXPORT' in content:
                raise ValueError('Temporary service profiling instrumentation in ' + name + '; wait for uninstrumented source')
    for extra in ('README.md', 'xtask/Cargo.toml', 'xtask/Cargo.lock'):
        p = root / extra
        if p.is_file():
            rows[extra] = digest(p)
    for part in ('src', 'tests'):
        for p in (root / 'xtask' / part).rglob('*'):
            if p.is_file():
                rows[str(p.relative_to(root))] = digest(p)
    manifests = sorted((root / 'parity/reference').glob('*/manifest.json'))
    if {p.parent.name for p in manifests} != {'bar', 'island', 'panels'}:
        raise ValueError('Missing parity reference manifest(s)')
    for p in manifests:
        rows[str(p.relative_to(root))] = digest(p)
        data = json.loads(p.read_text())
        if not data.get('images'):
            raise ValueError(str(p) + ': empty image manifest')
        for image in data['images']:
            name = image['file']
            ref = p.parent / name
            if Path(name).is_absolute() or '..' in Path(name).parts:
                raise ValueError('Unsafe reference path: ' + name)
            if not ref.is_file():
                raise ValueError('Missing manifest-listed image: ' + str(ref))
            content = ref.read_bytes()
            if not content.startswith(b'\x89PNG\r\n\x1a\n') or not content.endswith(b'\x00\x00\x00\x00IEND\xaeB`\x82'):
                raise ValueError('Incomplete PNG capture: ' + str(ref))
            rows[str(ref.relative_to(root))] = hashlib.sha256(content).hexdigest()
    return rows

def consistent_copy(parent, state, evidence):
    records = []
    while time.time() < DEADLINE:
        try:
            before = copy_inputs(ROOT)
            for sibling in ('hypr', 'quickshell'):
                before[sibling + '/'] = manifest(ROOT.parent / sibling)
            clone(ROOT, parent / 'rshell')
            for sibling in ('hypr', 'quickshell'):
                clone(ROOT.parent / sibling, parent / sibling)
            after = copy_inputs(ROOT)
            copied = copy_inputs(parent / 'rshell')
            for sibling in ('hypr', 'quickshell'):
                after[sibling + '/'] = manifest(ROOT.parent / sibling)
                copied[sibling + '/'] = manifest(parent / sibling)
            changed = [name for name in sorted(set(before) | set(after) | set(copied)) if before.get(name) != after.get(name) or after.get(name) != copied.get(name)]
            records.append({'at': utc(), 'changed_during_copy': changed, 'input_count': len(copied)})
            if not changed:
                (evidence / 'copy-consistency.json').write_text(json.dumps(records, indent=2) + '\n')
                return True
        except (OSError, ValueError, KeyError) as error:
            records.append({'at': utc(), 'incomplete_or_changing_inputs': str(error)})
        (evidence / 'copy-consistency.json').write_text(json.dumps(records, indent=2) + '\n')
        if parent.exists():
            shutil.rmtree(parent)
        state['copy_attempts'] = len(records)
        state['copy_status'] = records[-1]
        report(state)
        time.sleep(min(10, max(0, DEADLINE - time.time())))
    return False

def engine_idle_regressions(root):
    """Discover newly landed offline engine idle/wakeup tests in each frozen tree.

    Never opt an ignored live-compositor test into the user's current session.
    Such a test is recorded for harness review instead of being claimed as run.
    """
    found = []
    crate = root / 'crates/hyprv-ui'
    for area in ('src', 'tests'):
        for path in sorted((crate / area).rglob('*.rs')):
            text = path.read_text()
            for match in re.finditer(r'((?:\s*#\[[^\]]*\]\s*)+)fn\s+(\w+)\s*\(', text):
                attrs, name = match.groups()
                if not re.search(r'#\[(?:tokio::)?test(?:\([^]]*\))?\]', attrs):
                    continue
                if 'idle' not in name or not any(part in name for part in ('wake', 'poll', 'sched')):
                    continue
                # This pre-existing opted-in smoke test is not the upcoming regression.
                if name == 'shared_egl_service_wakeup_and_idle':
                    continue
                target = ['--lib'] if area == 'src' else ['--test', path.relative_to(crate / 'tests').parts[0].removesuffix('.rs')]
                found.append({'name': name, 'path': str(path.relative_to(root)),
                              'ignored': '#[ignore' in attrs,
                              'command': ['cargo', 'test', '-p', 'hyprv-ui', '--locked', *target, name]})
    assembled = root / 'crates/hyprv-shell/tests/idle.rs'
    if assembled.is_file() and 'fn quiet_nested_shell_has_no_repaints_or_extra_main_dispatches(' in assembled.read_text():
        found.append({'name': 'quiet_nested_shell_has_no_repaints_or_extra_main_dispatches',
                      'path': str(assembled.relative_to(root)), 'ignored': False,
                      'command': ['cargo', 'test', '-p', 'hyprv-shell', '--locked', '--test', 'idle', '--', '--nocapture']})
    return found

def refresh_workspace_mtimes(root):
    # Cargo can consider identical copied files fresh although an existing ELF
    # embeds env!(CARGO_MANIFEST_DIR) from a now-deleted previous snapshot.
    # Recompile local targets while preserving the dependency cache and byte hashes.
    stamp = time.time_ns()
    count = 0
    for p in (root / 'crates').rglob('*'):
        if p.is_file() and not p.is_symlink() and (p.suffix == '.rs' or p.name == 'Cargo.toml') and not any(part in ('target', '.check', '__pycache__', 'artifacts') for part in p.relative_to(root / 'crates').parts):
            os.utime(p, ns=(stamp, stamp))
            count += 1
    for p in (root / 'xtask').rglob('*'):
        if p.is_file() and not p.is_symlink() and (p.suffix == '.rs' or p.name == 'Cargo.toml') and 'target' not in p.parts:
            os.utime(p, ns=(stamp, stamp))
            count += 1
    return count

def run(command, cwd, log, timeout=900, env=None):
    # Disk policy: conservatively treat every Cargo job as capable of >5 GiB.
    if command[0] in ('cargo', 'packaging/build-release.sh') and shutil.disk_usage('/').free < 30 * 1024**3:
        log.write_text('INFRASTRUCTURE: aborted before starting; root free space below the 30 GiB disk-policy minimum.\n')
        print(utc(), 'DISK POLICY ABORT', command, flush=True)
        return 125
    print(utc(), 'RUN', command, flush=True)
    with log.open('w') as out:
        proc = subprocess.Popen(command, cwd=cwd, env=env or ENV, stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
        until = min(DEADLINE, time.time() + timeout)
        while proc.poll() is None:
            if time.time() >= until:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                print(utc(), 'TIMEOUT', str(log), flush=True)
                return 124
            time.sleep(1)
    print(utc(), 'DONE', proc.returncode, str(log), flush=True)
    return proc.returncode

def diagnostics(log):
    errors = []
    text = log.read_text(errors='replace')
    for line in text.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        msg = item.get('message')
        if item.get('reason') == 'compiler-message' and isinstance(msg, dict) and msg.get('level') == 'error':
            errors.append(msg)
    return text, errors

def file_failure(phase, row, state):
    # A deadline or unconfirmed one-off does not establish a crate defect.
    if row['code'] == 124 or not row.get('attempts'):
        return
    text, errors = diagnostics(OUT / row['attempts'][-1]['log'])
    first_text, first_errors = diagnostics(OUT / row['attempts'][0]['log'])
    if any(any(reason in value for reason in ('No space left on device', 'Disk quota exceeded', 'INFRASTRUCTURE:')) for value in (first_text, text)) or any(r['code'] < 0 for r in row['attempts']):
        row['classification'] = 'INFRASTRUCTURE: storage exhaustion or interrupted command; no crate attribution'
        return
    flaky = row['code'] == 0
    if flaky:
        text, errors = first_text, first_errors
    owners = set()
    locations = []
    for msg in errors:
        for span in msg.get('spans', []):
            if span.get('is_primary'):
                name = os.path.normpath(span['file_name'])
                try:
                    name = str(Path(name).relative_to(state['snapshot']))
                except ValueError:
                    pass
                locations.append(f"{name}:{span['line_start']}")
                m = re.search(r'crates/(hyprv-[\w-]+)/', name)
                if m:
                    owners.add(m.group(1))
    for m in re.finditer(r'(?:panicked at |--> )(?:[^\n]*?/)?(crates/(hyprv-[\w-]+)/[^:\n]+):(\d+)', text):
        owners.add(m.group(2)); locations.append(m.group(1) + ':' + m.group(3))
    for m in re.finditer(r'File "[^"\n]*?/(crates/(hyprv-[\w-]+)/[^"\n]+)", line (\d+)', text):
        owners.add(m.group(2)); locations.append(m.group(1) + ':' + m.group(3))
    owners |= set(re.findall(r'test failed, to rerun pass `-p (hyprv-[\w-]+)', text))
    if not owners:
        owners = {'hyprv-shell'}
    # A second run can expose a different test failure; only repeated names are confirmed.
    failed1 = set(re.findall(r'^test (.+?) \.\.\. FAILED$', first_text, re.M))
    failed2 = set(re.findall(r'^test (.+?) \.\.\. FAILED$', text, re.M))
    if not flaky and failed2 and not (failed1 & failed2) and not errors:
        row['classification'] = 'different failures on retry; no repeated test confirmed'
        return
    label = 'FLAKY: first attempt failed, same-snapshot retry passed' if flaky else 'REPRODUCIBLE: failed twice on same snapshot'
    row['classification'] = label
    relevant = errors[0].get('rendered', '') if errors else '\n'.join(l for l in text.splitlines() if not l.startswith('{'))[-6000:]
    for owner in owners:
        paths = {s.split(':')[0] for s in locations if s.startswith('crates/' + owner + '/')}
        fingerprints = '\n'.join(f"`{p}`: `{state['inputs'].get(p, {}).get('sha256', 'unavailable')}`" for p in sorted(paths))
        key = hashlib.sha256((owner + phase + label + relevant + fingerprints).encode()).hexdigest()
        seen_path = OUT / 'filed.json'
        seen = set(json.loads(seen_path.read_text())) if seen_path.exists() else set()
        if key in seen:
            continue
        with (ROOT / 'docs/requests' / (owner + '.md')).open('a') as out:
            out.write(f"\n\n### Snapshot CI — {utc()} — {phase}\n\n{label}. Snapshot `{state['snapshot']}`; fingerprint `{state['fingerprint']}`. Concurrent live edits cannot change this source copy.\n\nCommand: `{row['command']}`. Attempts: `{row['attempts']}`. Locations: {', '.join(locations)}.\n\n{fingerprints}\n\n```text\n{relevant[-6000:]}\n```\n\nLogs relative to `docs/verify/raw/ci-snapshots/`. No causal commit inferred.\n")
        seen.add(key); seen_path.write_text(json.dumps(sorted(seen), indent=2) + '\n')

def report(state):
    save = {k: v for k, v in state.items() if k != 'inputs'}
    (OUT / 'latest.json').write_text(json.dumps(save, indent=2) + '\n')
    lines = ['# Snapshot integration CI', '', f"Updated {utc()}. Window {utc(START)}–{utc(DEADLINE)}.", '',
             f"Round {state['round']}: **{state['status']}**.", '',
             f"Snapshot: `{state.get('snapshot', 'copying')}`.", f"SHA-256 fingerprint: `{state.get('fingerprint', 'pending')}`.",
             f"Existing shared target cache: `{TARGET}`. No private target directory is created or cleaned. Incremental compilation and debug symbols are disabled for dev/test builds to fit storage; these remain unoptimized debug builds with debug assertions. Release profile is unchanged. Cargo uses four build jobs. Source files use copy-on-write reflinks, never hard links to live files. `target/` and Git metadata are excluded at every depth. Sibling Hypr/Quickshell fixture inputs are copied too.", '',
             'Snapshot admission requires equal build/test/reference hashes before and after copying and in the copied tree. Every manifest-listed reference PNG must exist and be complete; incomplete recaptures are recopied. A failing test command is rerun once on the same snapshot before filing; both outcomes and logs are retained. A retry pass is labelled flaky. Timeout/deadline alone is not a crate attribution.', '',
             '| Check | Result | Evidence |', '|---|---|---|']
    for phase, row in state['phases'].items():
        result = ('PASS (retry; flaky)' if phase != 'hourly performance' and len(row.get('attempts', [])) > 1 else 'PASS') if row['code'] == 0 else ('TIMEOUT / INCOMPLETE' if row['code'] == 124 else 'FAIL')
        if row.get('classification', '').startswith('INFRASTRUCTURE'):
            result = 'BLOCKED (infrastructure)' if row['code'] else 'PASS (infrastructure retry)'
        if row.get('ci_exempt_reason'):
            result = 'EXEMPT (parity; full failure retained)'
        links = ', '.join(f"[{'step' if phase == 'hourly performance' else 'attempt'} {i+1}](raw/ci-snapshots/{r['log']}) ({r['code']})" for i, r in enumerate(row.get('attempts', [])))
        lines.append(f'| {phase} | {result} | {links} |')
    for row in state.get('preflight', []):
        result = 'PASS' if row['ok'] else ('EXEMPT' if row['check'] in EXEMPT or row.get('ci_exempt_reason') else 'BLOCKED')
        lines.append('| Preflight: ' + row['check'] + ' | ' + result + ' | ' + row['detail'].replace('|', '\\|') + ' |')
    lines += ['', '| Crate | Debug | Release | Tests | Clippy |', '|---|---|---|---|---|']
    for crate in sorted(p.name for p in (ROOT / 'crates').iterdir() if (p / 'Cargo.toml').is_file()):
        cells = []
        for phase in ('debug', 'release', 'workspace tests', 'clippy'):
            row = state['phases'].get(phase)
            result = 'PENDING' if row is None else ('PASS' if row['code'] == 0 else 'BLOCKED')
            if phase == 'workspace tests' and row and row.get('ci_exempt_reason') and state['phases'].get('non-parity workspace tests', {}).get('code') == 0:
                result = 'PASS (non-parity)' if crate not in ('hyprv-ui-panels', 'hyprv-widgets') else 'PASS (non-parity); parity exempt'
            if row and row['code'] not in (0, 124) and not row.get('classification', '').startswith('INFRASTRUCTURE') and not row.get('ci_exempt_reason'):
                text, errors = diagnostics(OUT / row['attempts'][-1]['log'])
                owns = any(any(s.get('is_primary') and 'crates/' + crate + '/' in s.get('file_name', '') for s in msg.get('spans', [])) for msg in errors)
                owns |= bool(re.search(r'panicked at (?:[^\n]*?/)?crates/' + re.escape(crate) + '/', text))
                if owns:
                    result = 'FAIL'
            cells.append(result)
        lines.append('| ' + crate + ' | ' + ' | '.join(cells) + ' |')
    lines += ['', 'Engine idle-wakeup regression discovery: `' + json.dumps(state.get('engine_idle_regressions', 'PENDING: not present in this snapshot'), sort_keys=True) + '`. Ignored live tests require an isolated harness and are never enabled against the live session.']
    if state.get('performance'):
        lines += ['', 'Hourly real release performance: `' + json.dumps(state['performance'], sort_keys=True) + '`.', 'See [perf-shell.md](perf-shell.md). Live-hook notifications and controlled-quiet CPU remain separate. No inter-window CPU subtraction is treated as a causal event cost.']
    lines += ['', 'All workspace tests are required; no reference-image tests are exempt or skipped. BLOCKED is not a defect attribution.', '', 'Completion requires both builds, full workspace tests, doctests, Clippy with warnings denied, xtask/cutover/probe tests, staged release verification and performance preflight with idle <=0.1%. Only helper installation and the separate parity preflight gate are exempt; performance is required. No install or live actions are run.', '',
              'Full manifests, per-crate diagnostics and historical round results: [snapshot CI evidence](raw/ci-snapshots/). Source fingerprints identify the copied candidate, not the moving live workspace.']
    if state.get('snapshot_removed'):
        lines += ['', 'Disk policy: this completed source snapshot was removed after summarizing evidence. The complete input hash manifest, command logs, build receipt and release manifest remain in the evidence directory. No new target cache is created.']
    if state.get('parity_test_exemption'):
        lines += ['', state['parity_test_exemption'], 'The product preflight and full-test receipt remain failed; this is only the user-authorized integration-CI parity exemption, not full cutover readiness.']
    (ROOT / 'docs/verify/ci.md').write_text('\n'.join(lines) + '\n')
    if 'directory' in state:
        (OUT / state['directory'] / 'result.json').write_text(json.dumps(save, indent=2) + '\n')

def refresh_performance(root, evidence, state, gate):
    """Measure one immutable staged ELF, with no Cargo checks running concurrently."""
    tag = 'ci-' + dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dt%H%M%Sz')
    receipt = json.loads((root / 'packaging/dist/build.json').read_text())
    if receipt['sources'] != gate.build.sources():
        raise RuntimeError('Snapshot stage receipt does not match candidate')
    archive = ROOT / 'packaging/.build/perf-shell' / tag
    archive.mkdir(parents=True)
    binary = archive / 'hyprv-shell'
    shutil.copy2(root / 'packaging/dist/hyprv-shell', binary)
    if digest(binary) != receipt['binaries']['hyprv-shell']:
        raise RuntimeError('Archived candidate digest mismatch')
    inputs = archive / 'inputs.json'
    inputs.write_text(json.dumps({'command': receipt['command'], 'source_root': str(root), 'files': gate.shell_sources(), 'ci_snapshot_fingerprint': state['fingerprint']}, indent=2) + '\n')
    env = dict(ENV, TMPDIR='/tmp', HYPRV_SHELL_BINARY=str(binary), HYPRV_SHELL_SOURCE_ROOT=str(root), HYPRV_SHELL_INPUTS_JSON=str(inputs))
    attempts = []
    for suffix, options in [('quiet', ['--quiet-agents', '--idle-only']), ('no-hooks', ['--quiet-hooks', '--idle-only']), ('live', [])]:
        log = evidence / ('performance-' + suffix + '.log')
        code = run(['python3', '-B', 'docs/verify/measure-shell.py', '--label', tag + '-' + suffix, *options], ROOT, log, timeout=600, env=env)
        attempts.append({'code': code, 'log': str(log.relative_to(OUT))})
        if code:
            state['phases']['hourly performance'] = {'code': code, 'attempts': attempts}
            return False
    log = evidence / 'performance-report.log'
    code = run(['python3', '-B', 'docs/verify/report-shell.py', '--tag', tag], ROOT, log, timeout=60)
    attempts.append({'code': code, 'log': str(log.relative_to(OUT))})
    state['phases']['hourly performance'] = {'code': code, 'attempts': attempts}
    if code:
        return False
    data = json.loads((ROOT / 'docs/verify/raw/perf-shell-latest.json').read_text())
    state['performance'] = {'tag': tag, 'epoch': data['epoch'], 'snapshot_fingerprint': state['fingerprint'], 'controlled_idle_cpu': data['controlled_idle']['cpu_percent'], 'live_idle_cpu': data['live_idle']['cpu_percent'], 'live_hook_notifications': data['live_idle']['hook_file_events'], 'binary_sha256': data['binary_sha256']}
    (OUT / 'hourly-performance.json').write_text(json.dumps(state['performance'], indent=2) + '\n')
    (evidence / 'performance-summary.json').write_text(json.dumps(data, indent=2) + '\n')
    # Retain only the selected ELF; historical receipts and raw metrics stay small.
    for previous in archive.parent.glob('ci-*'):
        if previous != archive and previous.is_dir():
            (previous / 'hyprv-shell').unlink(missing_ok=True)
    return True

def main():
    round_no = max((int(p.name.split('-')[1]) for p in OUT.glob('round-*') if p.is_dir()), default=0)
    last_perf_slot = -1
    final_perf_done = False
    perf_state = OUT / 'hourly-performance.json'
    if perf_state.exists():
        previous_perf = json.loads(perf_state.read_text())
        if previous_perf.get('epoch', 0) >= START:
            last_perf_slot = int((previous_perf['epoch'] - START) // 3600)
    if (OUT / 'latest.json').exists():
        previous = json.loads((OUT / 'latest.json').read_text())
        next_start = dt.datetime.fromisoformat(previous['started']).timestamp() + 600 if previous.get('finished') else time.time()
        while time.time() < min(next_start, DEADLINE):
            time.sleep(min(20, min(next_start, DEADLINE) - time.time()))
    while time.time() < DEADLINE:
        round_no += 1
        began = time.time()
        stamp = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        directory = f'round-{round_no:02d}-{stamp}'
        evidence = OUT / directory; evidence.mkdir()
        # AF_UNIX tests bind under CARGO_MANIFEST_DIR; keep actual paths <108 bytes.
        parent = Path('/tmp/hci-sys') / directory
        root = parent / 'rshell'
        state = {'round': round_no, 'directory': directory, 'started': utc(), 'snapshot': str(root), 'status': 'COPYING', 'phases': {}}
        report(state)
        if not consistent_copy(parent, state, evidence):
            state['status'] = 'TWO-HOUR LIMIT; SNAPSHOT INPUTS INCOMPLETE OR CHANGING'
            report(state)
            return
        inputs = manifest(root)
        siblings = {s: manifest(parent / s) for s in ('hypr', 'quickshell')}
        receipt = {'rshell': inputs, **siblings}
        state['inputs'] = inputs
        state['fingerprint'] = hashlib.sha256(json.dumps(receipt, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        (evidence / 'snapshot-manifest.json').write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
        state['status'] = 'RUNNING'; report(state)
        gate = module(root / 'cutover/preflight.py', f'gate_{round_no}')
        original_build_inputs = gate.build.sources()
        state['workspace_mtimes_refreshed'] = refresh_workspace_mtimes(root)
        tests = gate.test_commands()
        commands = [('debug', ['cargo', 'build', '--workspace', '--locked', '--message-format=json'], False),
                    ('release', ['cargo', 'build', '--workspace', '--release', '--locked', '--message-format=json'], False),
                    ('stage release', ['packaging/build-release.sh'], False),
                    ('workspace tests', tests[0] + ['--message-format=json'], True),
                    ('doctests', tests[1] + ['--message-format=json'], True),
                    ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings'], False),
                    ('xtask tests', tests[2], True), ('cutover tests', tests[3], True), ('probe tests', tests[4], True),
                    ('snapshot guard tests', ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'docs/verify', '-p', 'test_ci_snapshot.py'], False)]
        discovered = engine_idle_regressions(root)
        state['engine_idle_regressions'] = discovered or 'PENDING: not present in this snapshot'
        for regression in discovered:
            if not regression['ignored']:
                commands.append(('engine idle-wakeup: ' + regression['name'], regression['command'], False))
        report(state)
        test_rows = {}
        for index, (phase, command, is_test) in enumerate(commands):
            if time.time() >= DEADLINE:
                break
            row = {'command': command, 'attempts': []}
            for attempt in range(2):
                log = evidence / f'{index:02d}-{phase.replace(" ", "-")}-{attempt+1}.log'
                code = run(command, root, log)
                row['code'] = code
                row['attempts'].append({'code': code, 'log': str(log.relative_to(OUT))})
                if code == 0 or time.time() >= DEADLINE:
                    break
                # Repeated source compilation is cheap and confirms non-test diagnostics too.
            state['phases'][phase] = row
            if len(row['attempts']) == 2:
                file_failure(phase, row, state)
            if is_test:
                test_rows[phase] = row
            report(state)
            if phase == 'stage release' and code == 0 and time.time() < DEADLINE - 480:
                slot = int((time.time() - START) // 3600)
                final_refresh_due = not final_perf_done and DEADLINE - time.time() <= 900
                if slot > last_perf_slot or final_refresh_due:
                    try:
                        if refresh_performance(root, evidence, state, gate):
                            last_perf_slot = slot
                            final_perf_done |= final_refresh_due
                    except (OSError, ValueError, RuntimeError, KeyError) as error:
                        log = evidence / 'performance-failure.log'; log.write_text(repr(error) + '\n')
                        state['phases']['hourly performance'] = {'code': 1, 'attempts': [{'code': 1, 'log': str(log.relative_to(OUT))}]}
                    if state['phases'].get('hourly performance', {}).get('code'):
                        with (ROOT / 'docs/verify/perf-shell.md').open('a') as out:
                            out.write('\nHourly CI refresh failed at ' + utc() + '; latest successful evidence remains historical. See ' + str(evidence.relative_to(ROOT)) + '/performance-*.log.\n')
                    report(state)
        # A newly green candidate must not be held behind another snapshot's stale
        # performance evidence until the next hourly slot.
        if all(state['phases'].get(name, {}).get('code') == 0 for name, _, _ in commands) and 'hourly performance' not in state['phases'] and time.time() < DEADLINE - 480:
            measured = json.loads((ROOT / 'docs/verify/raw/perf-shell-latest.json').read_text())
            if measured.get('build_snapshot', {}).get('files') != gate.shell_sources():
                try:
                    if refresh_performance(root, evidence, state, gate):
                        last_perf_slot = int((time.time() - START) // 3600)
                        state['performance']['reason'] = 'newly green CI candidate requires current idle evidence'
                except (OSError, ValueError, RuntimeError, KeyError) as error:
                    log = evidence / 'performance-failure.log'; log.write_text(repr(error) + '\n')
                    state['phases']['hourly performance'] = {'code': 1, 'attempts': [{'code': 1, 'log': str(log.relative_to(OUT))}]}
                report(state)
        if time.time() < DEADLINE:
            mapping = ['workspace tests', 'doctests', 'xtask tests', 'cutover tests', 'probe tests']
            receipt_dir = root / 'cutover/evidence'; receipt_dir.mkdir(exist_ok=True)
            results = [{'command': cmd, 'returncode': test_rows[name]['code'], 'log': test_rows[name]['attempts'][-1]['log'], 'attempts': test_rows[name]['attempts']} for cmd, name in zip(tests, mapping)]
            (receipt_dir / 'tests.json').write_text(json.dumps({'schema': 1, 'epoch': time.time(), 'sources': gate.build.sources(), 'results': results, 'status': 'pass' if all(r['returncode'] == 0 for r in results) else 'fail'}, indent=2) + '\n')
            for path in ('docs/verify/raw/perf-shell-latest.json', 'docs/verify/perf-shell.md'):
                shutil.copy2(ROOT / path, root / path)
            if 'performance' not in state and (OUT / 'hourly-performance.json').exists():
                state['performance'] = json.loads((OUT / 'hourly-performance.json').read_text())
            log = evidence / 'preflight.json'
            code = run(['cutover/preflight.sh', '--json'], root, log)
            try:
                state['preflight'] = json.loads(log.read_text())['checks']
            except (ValueError, KeyError):
                state['preflight'] = [{'check': 'preflight execution', 'ok': False, 'detail': log.read_text()[-2000:]}]
            state['preflight_exit'] = code
        # Verify no original source input changed inside the copied candidate.
        current_build_inputs = gate.build.sources()
        changed = [name for name in sorted(set(original_build_inputs) | set(current_build_inputs)) if original_build_inputs.get(name) != current_build_inputs.get(name)]
        state['changed_snapshot_inputs'] = changed
        green = not any(r['ignored'] for r in discovered) and all(name in state['phases'] for name, _, _ in commands) and all(r['code'] == 0 or r.get('ci_exempt_reason') for r in state['phases'].values()) and bool(state.get('preflight')) and all(r['ok'] or r.get('ci_exempt_reason') for r in state['preflight'] if r['check'] not in EXEMPT) and not changed
        state['status'] = 'GREEN (helper/parity exempt; idle budget passed)' if green else ('TWO-HOUR LIMIT; NOT GREEN' if time.time() >= DEADLINE else 'WAITING FOR NEXT SNAPSHOT')
        state['finished'] = utc(); report(state)
        # Summarized evidence survives; obsolete full frozen copies do not.
        for name in ('build.json', 'release/MANIFEST.sha256', 'release/release.json'):
            source = root / 'packaging/dist' / name
            if source.is_file():
                shutil.copy2(source, evidence / ('staged-' + name.replace('/', '-')))
        idle_reports = root / 'target/idle-regression'
        if idle_reports.is_dir():
            for path in idle_reports.rglob('*'):
                if path.is_file() and path.suffix in ('.json', '.csv', '.log') and path.stat().st_size <= 2 * 1024 * 1024:
                    destination = evidence / 'engine-idle' / path.relative_to(idle_reports)
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, destination)
        shutil.rmtree(parent)
        state['snapshot_removed'] = True
        report(state)
        print(utc(), state['status'], state['fingerprint'], flush=True)
        if green:
            return
        while time.time() < min(began + 600, DEADLINE):
            time.sleep(min(20, min(began + 600, DEADLINE) - time.time()))
    state['status'] = 'TWO-HOUR LIMIT; NOT GREEN'; report(state)

if __name__ == '__main__':
    main()
