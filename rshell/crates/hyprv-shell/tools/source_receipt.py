#!/usr/bin/env python3
"""Hash build inputs and canonical full-screen inputs without freezing a source tree."""
from pathlib import Path
import hashlib,json,sys
root=Path(__file__).resolve().parents[3]
paths=[root/'Cargo.toml',root/'Cargo.lock']
for crate in (root/'crates').iterdir():
 if not crate.is_dir():continue
 for f in ['Cargo.toml','build.rs']:
  if (crate/f).exists():paths.append(crate/f)
 for d in ['src','tests','examples','assets','csrc','vendor']:
  paths.extend(p for p in (crate/d).rglob('*') if p.is_file())
paths.extend(p for p in (root/'parity/fullscreen/fixtures').glob('*.json'))
paths.extend(p for p in (root/'parity/fullscreen/reference').glob('*') if p.suffix in ['.png','.json'])
Path(sys.argv[1]).write_text(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted(set(paths))},indent=2))
