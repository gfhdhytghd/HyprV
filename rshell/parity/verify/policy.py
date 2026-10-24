"""Shared acceptance policy and content provenance; no rendering side effects."""
import hashlib
from pathlib import Path

EXCLUDED = {"bar": {"battery-charging", "battery-critical", "battery-full"}}
DELEGATED = {"calendar-host": "calendar", "island-expanded-host": "island-expanded"}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs(root, assembled=False):
    """Include transitive Rust/native/assets, replay code, fixtures and references.

    Content hashes deliberately survive snapshot copying and preserved mtimes.
    Generated render output and build caches are never dependencies.
    """
    root = Path(root)
    paths = [root / 'Cargo.toml', root / 'Cargo.lock']
    for crate in (root / 'crates').iterdir():
        paths += [p for p in (crate / 'Cargo.toml', crate / 'build.rs') if p.is_file()]
        for part in ('src', 'tests', 'examples', 'benches', 'vendor', 'csrc', 'native', 'assets', 'scripts', 'tools'):
            paths += [p for p in (crate / part).rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    areas = ['parity/gpu', 'parity/fixtures', 'parity/reference', 'parity/verify', 'packaging/assets']
    if assembled:
        areas += ['parity/fullscreen/fixtures', 'parity/fullscreen/reference', 'parity/fullscreen/tools',
                  'cutover/parity_gate.py', 'cutover/verify_fullscreen.py']
    for name in areas:
        base = root / name
        paths += [base] if base.is_file() else [p for p in base.rglob('*') if p.is_file() and not any(
            part in ('target', 'out', '__pycache__', '.build', 'logs') for part in p.relative_to(base).parts)]
    for area in ('fixtures/flickable-implementation', 'fixtures/panels-flickable', 'experiments/text'):
        paths += [p for p in (root / area).rglob('*') if p.is_file() and p.suffix in ('.rs', '.json', '.csv', '.png', '.bin', '.qml')
                  and not any(x in ('target', '__pycache__') for x in p.relative_to(root / area).parts)]
    for crate in (root / 'crates').iterdir():
        paths += [p for p in (crate / 'qml-ref').rglob('*') if p.is_file() and p.suffix in ('.json', '.png', '.qml')]
    rows = {str(p.relative_to(root)): sha(p) for p in sorted(set(paths))}
    legacy = root.parent / 'quickshell'
    # Assembly resolves image assets here. Include the original QML/JS too:
    # a different source generation must not reuse a prior render receipt.
    for p in legacy.rglob('*'):
        if p.is_file() and ('assets' in p.relative_to(legacy).parts or p.suffix in ('.qml', '.js')):
            rows['../quickshell/' + str(p.relative_to(legacy))] = sha(p)
    return rows
