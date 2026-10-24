"""Fail-closed production GL and assembled-scene parity consumer."""
import importlib.util
import fcntl
from contextlib import contextmanager
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('parity_policy', ROOT / 'parity/verify/policy.py')
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


def image(path):
    from PIL import Image
    with Image.open(path) as png:
        if png.format != 'PNG':
            raise ValueError('not a PNG: ' + str(path))
        size = png.size
        png.verify()
        return size


def rows_gate(data, expected):
    for name, entries in expected.items():
        slot = name + '-gl'
        rows = data.get('results', {}).get(slot, [])
        render = data.get('renders', {}).get(slot, {})
        files = [e['file'] for e in entries]
        if any(Path(f).is_absolute() or '..' in Path(f).parts for f in files):
            raise ValueError(slot + ': unsafe image filename')
        if name == 'bar':
            hosts = {(e['state'], e.get('theme'), e.get('scale')) for e in entries if e['state'] in policy.DELEGATED}
            required = {(state, theme, scale) for state in policy.DELEGATED for theme in ('light', 'dark') for scale in (1, 2)}
            if hosts != required:
                raise ValueError('bar-gl: all eight delegated host references are required')
        if not files or render.get('backend') != 'gl' or render.get('status') != 'ok' or render.get('exit') not in ((0, 1) if name == 'bar' else (0,)):
            raise ValueError(slot + ': no successful nonempty production GL render')
        if len(set(files)) != len(files) or len(rows) != len(files) or {r.get('file') for r in rows} != set(files):
            raise ValueError(slot + ': missing/extra/duplicate images')
        canonical = {e['file']: e for e in entries}
        for row in rows:
            entry = canonical[row['file']]
            if not isinstance(entry.get('distance_field_text', False), bool):
                raise ValueError(slot + ': invalid canonical text policy')
            if row.get('state') != entry['state'] or row.get('df') is not entry.get('distance_field_text', False):
                raise ValueError(slot + ': canonical state/text policy differs: ' + row['file'])
            if entry['state'] in policy.EXCLUDED.get(name, ()):
                if row.get('result') != 'excluded' or row.get('cause') != 'live-unreachable':
                    raise ValueError(slot + ': unreachable-state policy differs: ' + row['file'])
                continue
            if any(isinstance(row.get(k), bool) or not isinstance(row.get(k), (int, float)) for k in ('max', 'mean')):
                raise ValueError(slot + ': invalid pixel statistics')
            maximum = float(row.get('max', math.inf))
            mean = float(row.get('mean', math.inf))
            good = maximum <= 8 and mean <= .5 if entry.get('distance_field_text', False) else maximum == 0 and mean == 0
            if not all(math.isfinite(v) and v >= 0 for v in (maximum, mean)) or row.get('result') != 'pass' or row.get('structural') is not False or not good:
                raise ValueError(slot + ': parity gate red at ' + row['file'])
    return True


def fullscreen_keys(root):
    """Derive coverage from replay fixtures, not only a possibly partial manifest."""
    keys = set()
    for path in sorted((root / 'parity/fullscreen/fixtures').glob('*.json')):
        fixture = json.loads(path.read_text())
        if not isinstance(fixture.get('scene'), str):
            continue
        frames = []
        for step in fixture.get('steps', []):
            if 'capture' in step:
                frames.append(step['capture'])
            if 'frames' in step:
                sequence = step['frames']
                frames += [f"{sequence['name']}-t{int(t):04}" for t in sequence['at']]
        outputs = fixture.get('outputs') or fixture.get('services', {}).get('screens', [])
        if not frames or not outputs:
            raise ValueError('assembled fixture has no declared captures/outputs: ' + path.name)
        for output in outputs:
            for theme in ('light', 'dark'):
                for frame in frames:
                    key = (path.stem, frame, theme, float(output['scale']), output['name'])
                    if key in keys:
                        raise ValueError('duplicate assembled fixture capture: ' + repr(key))
                    keys.add(key)
    if not keys:
        raise ValueError('assembled fixture matrix is empty')
    return keys


def fullscreen_gate(root, data):
    """Require the complete canonical matrix, actual files and exact unmasked corners."""
    if data.get('schema') != 1 or data.get('status') != 'complete' or data.get('backend') != 'production-gl' or data.get('masked_pixels') != 0 or data.get('sources') != policy.inputs(root, assembled=True):
        raise ValueError('assembled full-screen evidence missing or stale')
    receipt = json.loads((root / 'packaging/dist/build.json').read_text())
    if data.get('binary_sha256') != receipt['binaries']['hyprv-shell']:
        raise ValueError('assembled full-screen ELF differs from staged release')
    entries = json.loads((root / 'parity/fullscreen/reference/manifest.json').read_text())['images']
    keys = {(e['state'], e.get('frame', ''), e['theme'], float(e['scale']), e['output']['name']) for e in entries}
    if len(keys) != len(entries) or keys != fullscreen_keys(root):
        raise ValueError('assembled reference manifest is missing/duplicating fixture captures')
    rows = data.get('images', [])
    if not entries or len({e['file'] for e in entries}) != len(entries) or len(rows) != len(entries) or {r.get('reference') for r in rows} != {e['file'] for e in entries}:
        raise ValueError('assembled full-screen missing/extra/duplicate images')
    by_file = {e['file']: e for e in entries}
    for row in rows:
        entry = by_file[row['reference']]
        if not isinstance(entry.get('distance_field_text', False), bool):
            raise ValueError('assembled invalid canonical text policy')
        actual = Path(row['actual'])
        width, height = image(actual)
        if policy.sha(actual) != row.get('actual_sha256'):
            raise ValueError('assembled full-screen image changed: ' + str(actual))
        if image(root / 'parity/fullscreen/reference' / entry['file']) != (width, height):
            raise ValueError('assembled image size differs from reference')
        gate = row.get('gate', {})
        maximum, mean = gate.get('max_channel_diff', math.inf), gate.get('painted_mae', math.inf)
        if not all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) and v >= 0 for v in (maximum, mean)):
            raise ValueError('assembled invalid pixel statistics')
        good = maximum <= 8 and mean <= .5 if entry.get('distance_field_text', False) else maximum == 0 and mean == 0
        corners = row.get('corners', [])
        cw, top, bottom = (math.ceil(n * entry['scale']) for n in (27, 42, 27))
        crops = {'top-left': [0, 0, cw, top], 'top-right': [width - cw, 0, cw, top],
                 'bottom-left': [0, height - bottom, cw, bottom],
                 'bottom-right': [width - cw, height - bottom, cw, bottom]}
        if (not all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) and v >= 0 for v in (maximum, mean))
                or not good or gate.get('pass') is not True or row.get('native', {}).get('pass') is not True or gate.get('footprint_ok') is not True
                or not isinstance(gate.get('painted_iou'), (int, float)) or not .98 <= gate['painted_iou'] <= 1
                or len(corners) != 4 or {c.get('corner') for c in corners} != {'top-left', 'top-right', 'bottom-left', 'bottom-right'}
                or any(c.get('max_channel_diff') != 0 or c.get('different_pixels') != 0
                       or c.get('crop') != crops[c['corner']] for c in corners)):
            raise ValueError('assembled full-screen strict gate red at ' + entry['file'])
    for state in policy.DELEGATED.values():
        for theme in ('light', 'dark'):
            if not any(e['state'] == state and e['theme'] == theme and not e.get('frame') for e in entries):
                raise ValueError('assembled delegated host coverage missing: ' + state + '/' + theme)
    return True


@contextmanager
def evidence_lock(path):
    # Readiness is read-only: the producer must already have created its lock.
    with path.open('rb') as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_SH | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('parity verification is still in progress: ' + str(path)) from error
        yield


def inspect(root):
    root = Path(root)
    with evidence_lock(root / 'parity/verify/out/.verify.lock'), evidence_lock(root / 'cutover/evidence/.fullscreen.lock'):
        return inspect_locked(root)


def inspect_locked(root):
    root = Path(root)
    data = json.loads((root / 'parity/verify/out/results.json').read_text())
    current = policy.inputs(root)
    expected = {}
    for name in ('bar', 'island', 'panels'):
        entries = json.loads((root / 'parity/reference' / name / 'manifest.json').read_text())['images']
        expected[name] = entries
        slot = name + '-gl'
        render = data.get('renders', {}).get(slot, {})
        if render.get('sources') != current or render.get('inputs_changed'):
            raise ValueError(slot + ': render dependency hashes stale or changed during render/diff')
        actual = root / 'parity/verify/out' / slot / 'actual'
        hashes = render.get('actual_sha256', {})
        if set(hashes) != {e['file'] for e in entries}:
            raise ValueError(slot + ': incomplete rendered-image hashes')
        for entry in entries:
            path = actual / entry['file']
            image(root / 'parity/reference' / name / entry['file'])
            image(path)
            if policy.sha(path) != hashes[entry['file']]:
                raise ValueError(slot + ': image changed: ' + entry['file'])
    rows_gate(data, expected)
    if (root / 'docs/verify/parity.md').stat().st_mtime_ns < (root / 'parity/verify/out/results.json').stat().st_mtime_ns:
        raise ValueError('parity report not regenerated after results')
    # Even if the bar harness now paints the eight host crops, assembly is a
    # separate obligation. Its own matrix defines the available output scales;
    # no DPR-1 full-screen coverage is inferred from the component DPR-1 crops.
    full = json.loads((root / 'cutover/evidence/fullscreen.json').read_text())
    fullscreen_gate(root, full)
    if current != policy.inputs(root) or full['sources'] != policy.inputs(root, assembled=True):
        raise ValueError('dependencies changed during parity gate inspection')
    return 'production GL components and complete assembled full-screen matrix pass; battery QUIRK exclusions only; unmasked corners exact'
