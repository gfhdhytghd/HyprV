from pathlib import Path
import json
b=Path(__file__).resolve().parent;c=json.loads((b/'cases.json').read_text());out='fn cases()->Vec<Case>{vec![\n'
for mode in ['menu','list']:
 for name,events in c.items():
  if mode=='list' and name in ['drag-only','overshoot-only']:continue
  if mode=='list' and name=='shrink':events=[dict(e,key='model',value=2) if e.get('key')=='contentHeight' else e for e in events]
  out+=f'("{name}",{str(mode=="list").lower()},vec!['
  for e in events:
   typ=e['type'];t=e['t']
   if typ=='set':ev=f'Event::Set("{e["key"]}",{float(e["value"])})'
   elif typ=='wheel':ev='Event::Wheel(Wheel{angle_delta:%s,pixel_delta:%s,phase:ScrollPhase::%s,synthesized:%s})'%(float(e.get('ay',0)),float(e.get('py',0)),{0:'None',1:'Begin',2:'Update',3:'End',4:'Momentum'}[e.get('phase',0)],str(e.get('pixel',False)).lower())
   elif typ=='move':ev=f'Event::Move({float(e["y"])},'+('Some('+str(float(e['v']))+')' if 'v' in e else 'None')+')'
   else:ev=f'Event::{typ.title()}({float(e["y"])})'
   out+=f'({t},{ev}),'
  out+=']),\n'
(b/'cases.rs').write_text(out+']}\n')
