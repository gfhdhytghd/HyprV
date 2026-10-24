#!/usr/bin/env python3
"""Premultiplied RGBA gate + spatial (not causal) contributor ranking.
Usage: gate_metrics.py ACTUAL FROZEN_REFERENCE REPORT
The only excluded area is the separately owned island slot, in both images.
"""
from pathlib import Path
import json
import math
import sys
import numpy as np
from PIL import Image, ImageDraw

actual, reference, out = map(Path, sys.argv[1:4])
out.mkdir(parents=True, exist_ok=True)
coverage = json.loads((actual / 'coverage.json').read_text())
manifest = {e['file']: e for e in json.loads((reference / 'manifest.json').read_text())['images']}
categories = ['rect_edge_aa', 'svg_icons', 'text', 'compositing']
totals = {k: dict(error=0, painted=0, pixels_over8=0, max=0) for k in categories}
results = []

def premultiply(a):
    p = a.astype(np.int32)
    p[:, :, :3] = (p[:, :, :3] * p[:, :, 3:4] + 127) // 255
    return p

def bbox(mask):
    yy, xx = np.where(mask)
    return np.array([xx.min(), yy.min(), xx.max()+1, yy.max()+1]) if len(xx) else None

for entry in coverage['exported']:
    name = entry['file']
    a = np.array(Image.open(actual / name).convert('RGBA')).astype(np.int16)
    b = np.array(Image.open(reference / name).convert('RGBA')).astype(np.int16)
    if a.shape != b.shape:
        raise SystemExit(f'structural dimension mismatch: {name}')
    h, w = a.shape[:2]
    scale = entry['scale']
    foreign = Image.new('1', (w, h))
    x, y, bw, bh = entry['foreign_island_local_rect']
    if bw > 0 and bh > 0:
        ImageDraw.Draw(foreign).rectangle((math.floor(x*scale), math.floor(y*scale), math.ceil((x+bw)*scale)-1, math.ceil((y+bh)*scale)-1), fill=1)
    exclude = np.array(foreign)
    straight_max = int(np.abs(a-b)[~exclude].max())
    delta = np.abs(premultiply(a)-premultiply(b))
    delta[exclude] = 0
    am = (a[:, :, 3] > 0) & ~exclude
    bm = (b[:, :, 3] > 0) & ~exclude
    painted = am | bm
    count = int(painted.sum())
    iou = float((am & bm).sum()/count) if count else 1.
    ab, bb = bbox(am), bbox(bm)
    edge_error = float(np.max(np.abs(ab-bb))/scale) if ab is not None and bb is not None else (0. if ab is None and bb is None else max(w, h)/scale+1.)
    structural = iou >= .98 and edge_error <= 1.
    labels = np.full((h, w), 3, dtype=np.uint8)
    # Disjoint masks, icon/image boxes have highest priority.
    for category in [categories[0], categories[2], categories[1]]:
        mask = Image.new('1', (w, h))
        d = ImageDraw.Draw(mask)
        for semantic in entry.get('semantic', []):
            if semantic['category'] != category:
                continue
            x, y, bw, bh = semantic['bounds']
            box = [x*scale, y*scale, (x+bw)*scale, (y+bh)*scale]
            if category == 'rect_edge_aa' and 'radius' in semantic:
                r, pad, pen = semantic['radius']*scale, 1.1, semantic['border']*scale
                d.rounded_rectangle([box[0]-pad, box[1]-pad, box[2]+pad, box[3]+pad], radius=r+pad, fill=1)
                inset = pen+pad
                if bw*scale > 2*inset and bh*scale > 2*inset:
                    d.rounded_rectangle([box[0]+inset, box[1]+inset, box[2]-inset, box[3]-inset], radius=max(0, r-inset), fill=0)
            else:
                d.rectangle(box, fill=1)
        labels[np.array(mask)] = categories.index(category)
    maximum = int(delta.max())
    mae = float(delta[painted].mean()) if count else 0.
    distance = manifest[name].get('distance_field_text')
    v1 = maximum <= 8 and mae <= .5
    exact = maximum == 0
    # Unlabelled historical captures cannot establish the tightened gate.
    gate = structural and (exact if distance is False else v1 if distance is True else False)
    row = dict(file=name, distance_field_text=distance, max_channel=maximum,
               straight_max_channel=straight_max, painted_mae=mae,
               over8=int((delta.max(2)>8).sum()), painted_pixels=count,
               painted_mask_iou=iou, footprint_edge_error_logical_px=edge_error,
               structural=structural, gate_v1=v1, exact=exact, gate=gate, categories={})
    for i, k in enumerate(categories):
        m = (labels == i) & painted
        values = delta[m]
        n, error = int(m.sum()), int(values.sum())
        over = int(((delta.max(2)>8) & m).sum())
        mx = int(values.max()) if n else 0
        row['categories'][k] = dict(absolute_error=error, painted_pixels=n, mae=error/(n*4) if n else 0., over8=over, max=mx)
        t = totals[k]
        t['error'] += error
        t['painted'] += n
        t['pixels_over8'] += over
        t['max'] = max(t['max'], mx)
    results.append(row)
ranking = sorted([dict(category=k, **t, mae=t['error']/(4*t['painted']) if t['painted'] else 0.) for k, t in totals.items()], key=lambda r: r['error'], reverse=True)
missing = sorted(set(manifest) - {r['file'] for r in results})
summary = dict(pixel_space='premultiplied RGBA; (straight*alpha+127)//255',
               images=len(manifest), rendered=len(results), missing=len(missing), missing_files=missing,
               passed=sum(r['gate'] for r in results),
               gate_v1_passed=sum(r['gate_v1'] for r in results), exact=sum(r['exact'] for r in results),
               structural_passed=sum(r['structural'] for r in results),
               failed=sum(not r['gate'] for r in results), max_channel=max(r['max_channel'] for r in results),
               max_image_painted_mae=max(r['painted_mae'] for r in results),
               painted_mae=sum(t['error'] for t in totals.values())/(4*sum(t['painted'] for t in totals.values())),
               ranking=ranking, images_detail=results)
(out/'gate.json').write_text(json.dumps(summary, indent=2, allow_nan=False))
text = '# Bar parity gate\n\nPremultiplied RGBA. No distance-field text: exact; with distance-field text: max <=8 and painted MAE <=0.5. Painted-mask IoU >=0.98 and footprint edges within 1 logical pixel. Foreign island slot excluded. Mask checks do not prove text/content equality.\n\n'
text += f"Current gate {summary['passed']}/{summary['images']}; missing {summary['missing']}; interim v1 {summary['gate_v1_passed']}/{summary['images']}; exact {summary['exact']}; structural mask {summary['structural_passed']}; painted MAE {summary['painted_mae']:.6f}/255; worst channel {summary['max_channel']}.\n\n"
text += '| Spatial category | Absolute error | Painted MAE | Pixels above 8 | Max |\n|---|---:|---:|---:|---:|\n'
for r in ranking:
    text += f"| {r['category']} | {r['error']} | {r['mae']:.6f} | {r['pixels_over8']} | {r['max']} |\n"
text += '\n| Image | Gate | v1 | Exact | IoU | Max | Painted MAE |\n|---|---|---|---|---:|---:|---:|\n'
for r in results:
    text += f"| {r['file']} | {'PASS' if r['gate'] else 'FAIL'} | {r['gate_v1']} | {r['exact']} | {r['painted_mask_iou']:.5f} | {r['max_channel']} | {r['painted_mae']:.6f} |\n"
(out/'gate.md').write_text(text)
print(json.dumps({k:v for k,v in summary.items() if k!='images_detail'}, indent=2))
