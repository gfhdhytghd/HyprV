#!/usr/bin/env python3
"""Read-only, high-cadence observer of the private output-add window."""
import json,socket,sys,time
from pathlib import Path
run=Path(sys.argv[1]);status=json.loads((run/'status.json').read_text());runtime=Path(status['runtime']);assert str(runtime).startswith('/tmp/hv-soak-')
command=next((runtime/'hypr').glob('*/.socket.sock'));start=status['start_monotonic']
log=(run/'hotplug-watch.jsonl').open('w',buffering=1)
def query(text):
 with socket.socket(socket.AF_UNIX) as stream:
  stream.settimeout(2);stream.connect(str(command));stream.sendall(text.encode());data=b''
  while True:
   part=stream.recv(65536)
   if not part:break
   data+=part
 return json.loads(data)
while True:
 status=json.loads((run/'status.json').read_text())
 if status['status']!='running':break
 elapsed=time.monotonic()-start
 if 164<=elapsed%180<=174:
  before=time.monotonic();row=dict(elapsed=elapsed,cycle=int(elapsed//180))
  try:
   row['outputs']=[m['name'] for m in query('j/monitors')]
   row['bars']={name:[{'pid':l['pid'],'w':l['w'],'h':l['h']} for level in value['levels'].values() for l in level if l['namespace']=='hyprv-quickshell'] for name,value in query('j/layers').items()}
   row['query_ms']=(time.monotonic()-before)*1000
  except Exception as error:row['error']=str(error)
  log.write(json.dumps(row)+'\n');time.sleep(.05)
 else:time.sleep(.2)
log.close()
