#!/usr/bin/env python3
"""Assert exact, unmasked RGBA corner crops in assembled whole-output captures."""
import argparse
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image


def premultiplied(image, crop):
    rgba = np.array(image.crop(crop), dtype=np.uint16)
    rgba[:, :, :3] = (rgba[:, :, :3] * rgba[:, :, 3:] + 127) // 255
    return rgba.astype(np.int16)


def verify(run):
    rows = []
    for manifest in sorted(run.glob('*/manifest.json')):
        job = json.loads(manifest.read_text())
        capture_manifest = job.get('capture_manifest')
        if not capture_manifest:
            continue
        base = manifest.parent / Path(capture_manifest).parent
        for cap in job.get('captures', []):
            reference = Path(cap['comparison']['reference'])
            actual = base / cap['file']
            ours = Image.open(actual).convert('RGBA')
            qt = Image.open(reference).convert('RGBA')
            if ours.size != qt.size:
                raise ValueError(f'dimension mismatch: {actual} / {reference}')
            width, height = ours.size
            scale = cap['scale']
            # Include the whole 27px shade and all 15px top reveal travel. These
            # are rectangular crops: no painted-pixel mask or tolerated error.
            cw, top, bottom = (math.ceil(n * scale) for n in (27, 42, 27))
            boxes = {
                'top-left': (0, 0, cw, top),
                'top-right': (width - cw, 0, width, top),
                'bottom-left': (0, height - bottom, cw, height),
                'bottom-right': (width - cw, height - bottom, width, height),
            }
            for corner, (x0, y0, x1, y1) in boxes.items():
                crop = (x0, y0, x1, y1)
                delta = abs(premultiplied(ours, crop) - premultiplied(qt, crop))
                rows.append(dict(file=cap['file'], output=cap['output'],
                                 scale=scale, corner=corner,
                                 crop=[x0, y0, x1 - x0, y1 - y0],
                                 max_channel_diff=int(delta.max()),
                                 different_pixels=int(np.any(delta, axis=2).sum())))
    if not rows:
        raise ValueError(f'no assembled captures in {run}')
    return {'pass': all(r['max_channel_diff'] == 0 for r in rows),
            'captures': len(rows) // 4, 'corners': len(rows), 'masked_pixels': 0,
            'max_channel_diff': max(r['max_channel_diff'] for r in rows), 'rows': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    result = verify(args.run.resolve())
    out = args.out or args.run / 'corner-comparison.json'
    out.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'rows'}))
    raise SystemExit(0 if result['pass'] else 1)


if __name__ == '__main__':
    main()
