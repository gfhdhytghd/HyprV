#!/usr/bin/env python3
"""Independent input intervention: identify frozen capture's stale hover state."""
from pathlib import Path
import json,os,subprocess,numpy as np
from PIL import Image
crate=Path(__file__).resolve().parents[1];root=crate.parents[1];out=crate/'artifacts/hover-intervention';out.mkdir(exist_ok=True)
name='music-expanded-playing';fixture=json.loads((root/f'parity/fixtures/island/{name}.json').read_text());fixture['steps'][-1:-1]=[{'hover':{'text':'\U000f04ae'}},{'advance':200}];f=out/f'{name}.json';f.write_text(json.dumps(fixture,indent=2)+'\n')
subprocess.run(['python3',str(crate/'tools/observe.py'),name,'2'],env={**os.environ,'HYPRV_OBSERVER_FIXTURE':str(f),'HYPRV_OBSERVER_SUFFIX':'-hover-intervention'},check=True)
def pm(p):
 a=np.array(Image.open(p).convert('RGBA'),dtype=np.int32);a[:,:,:3]=(a[:,:,:3]*a[:,:,3:4]+127)//255;return a
n=f'{name}--dark@2.png';actual=crate/f'artifacts/observe-{name}-hover-intervention'/n;d=abs(pm(root/'parity/reference/island'/n)-pm(actual));result=dict(max=int(d.max()),error=int(d.sum()),pixels=int(np.any(d,2).sum()),intervention='move actual Qt pointer onto previous control, then advance200ms');(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
