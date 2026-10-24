#!/usr/bin/env python3
"""Non-installing integration monitor. All evidence is local; never starts services."""
import datetime as dt
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'docs/verify/raw/ci'
OUT.mkdir(parents=True, exist_ok=True)
START = time.time()
DEADLINE = START + 7200

def utc(epoch=None):
    return dt.datetime.fromtimestamp(epoch or time.time(), dt.timezone.utc).isoformat(timespec='seconds')

spec = importlib.util.spec_from_file_location('ci_release_inputs', ROOT / 'packaging/build_release.py')
release_inputs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_inputs)

def snapshot():
    return {name: {'sha256': digest, 'mtime': utc((ROOT / name).stat().st_mtime)}
            for name, digest in release_inputs.sources().items()}

meta = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT))
PACKAGES = {p['id']: p['name'] for p in meta['packages'] if p['id'] in meta['workspace_members']}
CRATES = sorted(PACKAGES.values())
seen_path = OUT / 'reported.json'
seen = set(json.loads(seen_path.read_text())) if seen_path.exists() else set()

def request(crate, title, details, key):
    if key in seen:
        return
    path = ROOT / 'docs/requests' / (crate + '.md')
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a') as f:
        f.write('\n\n### Integration CI — ' + utc() + ' — ' + title + '\n\n' + details + '\n')
    seen.add(key)
    seen_path.write_text(json.dumps(sorted(seen), indent=2) + '\n')

def run(command, log, deadline=DEADLINE):
    print(utc(), 'RUN', ' '.join(command), flush=True)
    with log.open('w') as f:
        proc = subprocess.Popen(command, cwd=ROOT, stdout=f, stderr=subprocess.STDOUT, start_new_session=True)
        while proc.poll() is None:
            if time.time() >= deadline:
                import signal
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                return 124
            time.sleep(1)
    print(utc(), 'DONE', proc.returncode, log.relative_to(ROOT), flush=True)
    return proc.returncode

def diagnose(phase, log, code, before, after):
    statuses = {name: ('PASS' if code == 0 else 'BLOCKED') for name in CRATES}
    errors = []
    artifacts = set()
    executables = {}
    for line in log.read_text(errors='replace').splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        crate = PACKAGES.get(item.get('package_id'))
        if item.get('reason') == 'compiler-artifact' and crate:
            artifacts.add(crate)
            if item.get('executable'):
                executables[Path(item['executable']).name] = crate
        msg = item.get('message', {})
        if item.get('reason') == 'compiler-message' and msg.get('level') == 'error':
            errors.append((crate or 'hyprv-shell', msg))
    if phase != 'tests':
        for crate in artifacts:
            statuses[crate] = 'PASS'
    for crate, msg in errors:
        statuses[crate] = 'FAIL'
        spans = [s for s in msg.get('spans', []) if s.get('is_primary')]
        locations = []
        evidence = []
        for s in spans:
            name = s['file_name']
            locations.append(f"{name}:{s['line_start']}:{s['column_start']}")
            info = after.get(name, {})
            evidence.append(f"`{name}`: mtime {info.get('mtime', 'unavailable')}; SHA-256 `{info.get('sha256', 'unavailable')}`; changed during command: {before.get(name) != after.get(name)}.")
        title = phase + ': ' + msg['message']
        details = ('Reproduce: `' + ' '.join(COMMANDS[phase]) + '`. Exit ' + str(code) + '.\n\n'
                   + 'Primary location(s): ' + ', '.join('`' + x + '`' for x in locations) + '.\n\n'
                   + '\n'.join(evidence) + '\n\n```text\n' + msg.get('rendered', msg['message'])[-10000:] + '\n```\n\n'
                   + 'Full log: `' + str(log.relative_to(ROOT)) + '`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.')
        key = hashlib.sha256((crate + phase + msg['message'] + str([(x, after.get(x.split(':')[0], {}).get('sha256')) for x in locations])).encode()).hexdigest()
        request(crate, title, details, key)
    if code and not errors:
        # A runtime test failure follows the last cargo Running line, whose executable identifies its package.
        text = log.read_text(errors='replace')
        current = None
        failed = set()
        for line in text.splitlines():
            if 'Running ' in line or 'Doc-tests ' in line:
                current = next((owner for executable, owner in executables.items() if executable in line), None) or next((name for name in CRATES if name.replace('-', '_') in line or name in line), None)
            if ('test result: FAILED' in line or 'panicked at' in line) and current:
                failed.add(current)
        for crate in failed or {'hyprv-shell'}:
            statuses[crate] = 'FAIL'
            paths = sorted(set(re.findall(r'crates/[A-Za-z0-9_-]+/[A-Za-z0-9_./-]+\.rs', text)))
            fingerprints = '\n'.join(f'`{name}`: {after.get(name, {})}' for name in paths if name.startswith('crates/' + crate + '/'))
            request(crate, phase + ' failed without compiler diagnostic', f"Command `{' '.join(COMMANDS[phase])}` exited {code}. See `{log.relative_to(ROOT)}`. Tail:\n\n```text\n{text[-8000:]}\n```\n\nSource fingerprints at failure:\n{fingerprints}", hashlib.sha256((phase + crate + text[-8000:]).encode()).hexdigest())
    return statuses

COMMANDS = {
    'debug': ['cargo', 'build', '--workspace', '--locked', '--message-format=json'],
    'release': ['cargo', 'build', '--workspace', '--release', '--locked', '--message-format=json'],
    'tests': ['cargo', 'test', '--workspace', '--locked', '--message-format=json'],
    'clippy': ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format=json', '--', '-D', 'warnings'],
}

def report(state):
    lines = ['# Integration CI', '', 'Updated: ' + utc() + '. Start: ' + utc(START) + '; deadline: ' + utc(DEADLINE) + '.', '',
             'Non-installing checks only. No install, cutover, helper activation, or compositor reload is performed. Crate attribution uses compiler package IDs, primary error spans, source SHA-256 and mtimes; it does not claim a causal commit.', '',
             'Round: ' + str(state['round']) + ' (this monitoring window: ' + str(state.get('window_round', state['round'])) + '); status: **' + state['status'] + '**.', '',
             '| Crate | Debug | Release | Tests | Clippy |', '|---|---|---|---|---|']
    for crate in CRATES:
        lines.append('| ' + crate + ' | ' + ' | '.join(state['crates'][crate].get(p, 'PENDING') for p in COMMANDS) + ' |')
    lines += ['', 'PASS means the command succeeded for the full workspace, or (build/clippy only) Cargo emitted that crate’s successful artifact. BLOCKED means this command did not establish a pass; it is not a failing-crate attribution. Tests only receive PASS after the full workspace test command succeeds.', '', '| Check | Result | Evidence |', '|---|---|---|']
    for phase, row in state.get('phases', {}).items():
        lines.append(f"| {phase} | {'PASS' if row['code'] == 0 else 'FAIL (' + str(row['code']) + ')'} | [{Path(row['log']).name}](../../{row['log']}) |")
    for row in state.get('preflight', []):
        lines.append('| Preflight: ' + row['check'] + ' | ' + ('PASS' if row['ok'] else 'BLOCKED') + ' | ' + row['detail'].replace('|', '\\|') + ' |')
    lines += ['', 'Related verification: [real release shell performance](perf-shell.md) and [supplementary CI diagnostics](ci-diagnostics.md).', '', 'Source snapshots, changed files, command results and previous rounds: [raw CI evidence](raw/ci/).', '',
              'Helper installation is explicitly exempt from the requested CI completion condition. Parity/performance gates are unchanged; stale, isolated, or failed evidence remains blocked.', '']
    (ROOT / 'docs/verify/ci.md').write_text('\n'.join(lines))
    (OUT / 'latest.json').write_text(json.dumps(state, indent=2) + '\n')

round_no = max((int(p.name.split('-')[1]) for p in OUT.glob('round-*') if p.is_dir()), default=0)
if round_no and (OUT / 'latest.json').exists():
    previous = json.loads((OUT / 'latest.json').read_text())
    next_start = dt.datetime.fromisoformat(previous['started']).timestamp() + 600
    while time.time() < min(next_start, DEADLINE):
        time.sleep(min(20, min(next_start, DEADLINE) - time.time()))
first_round = round_no
last_perf_refresh = 0
while time.time() < DEADLINE:
    round_no += 1
    began = time.time()
    directory = OUT / f'round-{round_no:02d}'
    directory.mkdir(exist_ok=True)
    before = snapshot()
    (directory / 'sources-before.json').write_text(json.dumps(before, indent=2) + '\n')
    state = {'round': round_no, 'window_round': round_no - first_round, 'started': utc(began), 'status': 'RUNNING', 'crates': {name: {} for name in CRATES}, 'phases': {}}
    report(state)
    for phase, command in COMMANDS.items():
        if time.time() >= DEADLINE:
            break
        log = directory / (phase + '.log')
        code = run(command, log)
        after = snapshot()
        statuses = diagnose(phase, log, code, before, after)
        for name in CRATES:
            state['crates'][name][phase] = statuses[name]
        state['phases'][phase] = {'code': code, 'log': str(log.relative_to(ROOT))}
        report(state)
    if time.time() < DEADLINE:
        # Build staging is local only; it creates the provenance preflight requires.
        if state['phases'].get('release', {}).get('code') == 0:
            log = directory / 'stage-release.log'
            inspection = subprocess.run(['cutover/preflight.sh', '--json'], cwd=ROOT, capture_output=True, text=True)
            try:
                reusable = next(row['ok'] for row in json.loads(inspection.stdout)['checks'] if row['check'] == 'release binaries')
            except (ValueError, KeyError, StopIteration):
                reusable = False
            if reusable:
                log.write_text('PASS: existing production release receipt and staged ELF digests match all current candidate sources. No rebuild necessary.\n')
                code = 0
            else:
                code = run(['packaging/build-release.sh'], log)
            if code:
                content = log.read_text(errors='replace')
                owners = set(re.findall(r'crates/(hyprv-[A-Za-z0-9-]+)/', content)) or {'hyprv-shell'}
                for owner in owners:
                    request(owner, 'production release staging failed', f'Local, non-installing `packaging/build-release.sh` exited {code}. See `{log.relative_to(ROOT)}`. Build output:\n\n```text\n{content[-8000:]}\n```', hashlib.sha256(('stage-release' + owner + content[-8000:]).encode()).hexdigest())
            state['phases']['stage-release'] = {'code': code, 'log': str(log.relative_to(ROOT))}
        # New real-shell evidence replaces the probe. Refresh only a stable staged
        # candidate, while no Cargo check is running and with enough deadline margin.
        real_path = ROOT / 'docs/verify/raw/perf-shell-latest.json'
        if state['phases'].get('stage-release', {}).get('code') == 0 and real_path.exists() and time.time() - last_perf_refresh >= 1800 and DEADLINE - time.time() > 900:
            gate_spec = importlib.util.spec_from_file_location('ci_real_gate', ROOT / 'cutover/preflight.py')
            gate = importlib.util.module_from_spec(gate_spec)
            gate_spec.loader.exec_module(gate)
            data = json.loads(real_path.read_text())
            if data.get('build_snapshot', {}).get('files') != gate.shell_sources():
                last_perf_refresh = time.time()
                refresh_log = directory / 'real-shell-refresh.log'
                refresh_code = run(['python3', '-B', 'docs/verify/refresh-shell-perf.py', '--tag', 'ci-' + str(round_no)], refresh_log)
                state['phases']['real-shell refresh'] = {'code': refresh_code, 'log': str(refresh_log.relative_to(ROOT))}
                if refresh_code:
                    request('hyprv-shell', 'real-shell performance refresh failed', f'Non-installing production-candidate measurement exited {refresh_code}; see `{refresh_log.relative_to(ROOT)}` and matching raw/perf-shell/ci-{round_no}-*-run.log. This can be a staging/compositor harness failure and is not automatically a crate regression.', 'perf-refresh-' + str(round_no))
        log = directory / 'preflight.log'
        code = run(['cutover/preflight.sh', '--run-tests', '--json'], log)
        state['phases']['preflight'] = {'code': code, 'log': str(log.relative_to(ROOT))}
        content = log.read_text(errors='replace')
        try:
            start = content.index('{\n  "ready"')
            state['preflight'] = json.loads(content[start:])['checks']
        except (ValueError, KeyError):
            state['preflight'] = [{'check': 'preflight execution', 'ok': False, 'detail': content[-2000:]}]
        for row in state['preflight']:
            if row['ok'] or row['check'] == 'privhelper installed':
                continue
            owners = {'parity gate': ['hyprv-ui-island', 'hyprv-ui-bar', 'hyprv-ui-panels'], 'performance gate': ['hyprv-svc-probe'], 'release binaries': ['hyprv-shell'], 'automated tests': ['hyprv-shell']}.get(row['check'], ['hyprv-shell'])
            for owner in owners:
                request(owner, 'Preflight ' + row['check'], f"Non-installing `cutover/preflight.sh --run-tests --json`: {row['detail']}. See `{log.relative_to(ROOT)}`. Please regenerate valid current evidence after the underlying owner changes pass; do not relax the gate. This is gate routing, not proof that every routed crate caused the failure.", row['check'] + owner + row['detail'])
    after = snapshot()
    (directory / 'sources-after.json').write_text(json.dumps(after, indent=2) + '\n')
    state['changed_during_round'] = [p for p in sorted(set(before) | set(after)) if before.get(p) != after.get(p)]
    state['finished'] = utc()
    green = len(state['phases']) >= 5 and all(state['phases'].get(p, {}).get('code') == 0 for p in COMMANDS) and bool(state.get('preflight')) and all(row['ok'] for row in state['preflight'] if row['check'] != 'privhelper installed')
    state['status'] = 'GREEN (helper installation exempt)' if green else 'WAITING FOR OWNERS / GATES'
    if time.time() >= DEADLINE:
        state['status'] = 'TWO-HOUR LIMIT; NOT GREEN'
    (directory / 'result.json').write_text(json.dumps(state, indent=2) + '\n')
    report(state)
    print(utc(), state['status'], 'round', round_no, flush=True)
    if green:
        break
    next_round = began + 600
    while time.time() < min(next_round, DEADLINE):
        time.sleep(min(20, min(next_round, DEADLINE) - time.time()))
else:
    state['status'] = 'TWO-HOUR LIMIT; NOT GREEN'
    report(state)
