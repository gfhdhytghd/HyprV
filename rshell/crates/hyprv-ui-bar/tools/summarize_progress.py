#!/usr/bin/env python3
"""Compare immutable local GPU gate receipts; semantic attribution is spatial."""
from pathlib import Path
import sys,json
before,after,out=map(Path,sys.argv[1:4]);a=json.loads(before.read_text());b=json.loads(after.read_text());aa={r['file']:r for r in a['images_detail']};bb={r['file']:r for r in b['images_detail']}
def summary(g):
 rows=[r for r in g['images_detail'] if not r['distance_field_text'] and 'tray-menu-submenu--dark@2' not in r['file']]
 return {k:dict(error=sum(r['categories'][k]['absolute_error'] for r in rows),max=max(r['categories'][k]['max'] for r in rows),nonexact_images=sum(r['categories'][k]['max']>0 for r in rows)) for k in ['rect_edge_aa','text','svg_icons','compositing']}
r={'before':{k:a[k] for k in ['images','passed','exact','painted_mae']},'after':{k:b[k] for k in ['images','passed','exact','painted_mae']},'new_passes':[n for n in bb if bb[n]['gate'] and not aa[n]['gate']],'lost_passes':[n for n in bb if aa[n]['gate'] and not bb[n]['gate']],'increased_max':[n for n in bb if bb[n]['max_channel']>aa[n]['max_channel']],'increased_mean':[n for n in bb if bb[n]['painted_mae']>aa[n]['painted_mae']+1e-12],'native_categories_before':summary(a),'native_categories_after':summary(b)}
out.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
