import json, sys
from pathlib import Path
import numpy as np
from PIL import Image
sys.path.insert(0, "/home/wilf/.config/HyprV/rshell/parity/fullscreen/tools")
import importlib.util
spec = importlib.util.spec_from_file_location("cmp", "/home/wilf/.config/HyprV/rshell/parity/fullscreen/tools/compare.py")
cmp = importlib.util.module_from_spec(spec); spec.loader.exec_module(cmp)
run = Path(sys.argv[1]); REF = cmp.REF
rows = json.loads((run / "compare/compare.json").read_text())
man = {e["file"]: e for e in json.loads((REF / "manifest.json").read_text())["images"]}
TRAY = (2850, 16, 3012, 42)  # logical: tray icon strip (known cause T1)
out = []
for r in rows:
    if "gate" not in r: 
        out.append(r); continue
    e = man[r["reference"]]; sc = e["scale"]
    jobdir = next(run.glob(f"{r['state']}--{r['theme']}--*"))
    act = cmp.premul(next(jobdir.glob(f"run-*/fixture/{r['file']}")))
    ref = cmp.premul(REF / r["reference"])
    x0, y0, x1, y1 = (int(v * sc) for v in TRAY)
    if r["state"] != "tray-overflow":
        act[y0:y1, x0:x1] = 0; ref[y0:y1, x0:x1] = 0
    g = cmp.gate(act, ref, e["distance_field_text"], sc)
    d = np.abs(act - ref).max(axis=2)
    regs = cmp.regions(d > 8)[:8]
    for rg in regs:
        rg["box_logical"] = [round(v / sc, 1) for v in rg["box_px"]]
        rg["layers"] = cmp.layers_at(rg["box_px"], e["layers"], sc)
        sub = (act - ref)[rg["box_px"][1]:rg["box_px"][3], rg["box_px"][0]:rg["box_px"][2]]
        rg["mean_signed_alpha"] = round(float(sub[..., 3].mean()), 2)
        rg["shift"] = cmp.best_shift(act, ref, rg["box_px"])
    r["residual_gate"] = g; r["residual_regions"] = regs
    out.append(r)
    print(f"{r['file']:46s} residual {'PASS' if g['pass'] else 'FAIL'} max {g['max_channel_diff']:3d} mae {g['painted_mae']:.3f} iou {g['painted_iou']:.4f} >8px {g['over8_px']:6d}  " +
          "; ".join(f"{','.join(x['layers'][:1])}@{x['box_logical'][:2]}({x['px']})" for x in regs[:3]))
(run / "compare/residual.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
