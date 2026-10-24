#!/usr/bin/env python3
"""Verified side-hotspot/list/stats input, complementing IPC/calendar clock input.

The clock hover resets app expansion by design. Use the actual left/right
hotspots instead; every attempt records resulting layers. Only private socket.
"""
import json, os, select, socket, subprocess, sys, time
from pathlib import Path
run=Path(sys.argv[1]).resolve()
status=json.loads((run/'status.json').read_text());runtime=Path(status['runtime'])
assert str(runtime).startswith('/tmp/hv-soak-') and status['status']=='running'
wayland=next(p for p in runtime.glob('wayland-*') if p.is_socket())
command=next((runtime/'hypr').glob('*/.socket.sock'))
env={'PATH':'/usr/bin:/bin','HOME':str(runtime/'home'),'XDG_RUNTIME_DIR':str(runtime),'WAYLAND_DISPLAY':str(wayland),'DBUS_SESSION_BUS_ADDRESS':'unix:path='+str(runtime/'bus')}
log=(run/'extra-input.jsonl').open('w',buffering=1)
child=subprocess.Popen([str(run/'pointer'),str(wayland)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=(run/'extra-pointer.log').open('w'),text=True,env=env)
def send(line):
 child.stdin.write(line+'\n');child.stdin.flush()
 if not select.select([child.stdout],[],[],3)[0] or child.stdout.readline().strip()!='ok':raise RuntimeError('private pointer timeout')
def move(x,y):send(f'move {x} {y}')
def click(x,y):move(x,y);send('button 1');send('button 0')
def layers():
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(3);s.connect(str(command));s.sendall(b'j/layers');reply=b''
  while True:
   part=s.recv(65536)
   if not part:break
   reply+=part
 return json.loads(reply)
start=status['start_monotonic'];done=set()
log.write(json.dumps(dict(name='supplement-start',elapsed=time.monotonic()-start,reason='clock hover does not expand app UI; explicit side hotspots and stats click',private_wayland=str(wayland)))+'\n')
try:
 while True:
  status=json.loads((run/'status.json').read_text())
  if status['status']!='running':break
  elapsed=time.monotonic()-start;cycle=int(elapsed//180);phase=elapsed%180
  for at,name in [(66,'stats-click'),(73,'stats-outside-close'),(109,'agent-side-hover'),(119,'agent-list-scroll'),(141,'media-side-hover')]:
   if at<=phase<at+1 and (cycle,name) not in done:
    done.add((cycle,name));row=dict(name=name,elapsed=elapsed,cycle=cycle,monotonic_ns=time.monotonic_ns())
    try:
     if name=='stats-click':click(355,29)
     elif name=='stats-outside-close':click(850,600)
     elif name=='agent-side-hover':move(590,29)
     elif name=='agent-list-scroll':move(640,180);send('scroll 30');send('scroll -15')
     elif name=='media-side-hover':move(688,29)
     time.sleep(1)
     row['layers']=layers();row['ok']=True
     all_layers=[l for v in row['layers'].values() for level in v['levels'].values() for l in level]
     row['bar_height']=next((l['h'] for l in all_layers if l['namespace']=='hyprv-quickshell'),None)
     row['stats_mapped']=any('stats' in l['namespace'] for l in all_layers)
     if name.endswith('side-hover'):row['expanded_verified']=row['bar_height']>58
     if cycle<5 and name in ('agent-side-hover','media-side-hover','stats-click'):
      pic=run/(name+f'-cycle-{cycle}.png');result=subprocess.run(['grim','-o','WAYLAND-1',str(pic)],env=env,capture_output=True,text=True,timeout=5);row['screenshot_returncode']=result.returncode
    except Exception as e:row.update(ok=False,error=str(e))
    log.write(json.dumps(row)+'\n')
  time.sleep(.1)
finally:
 child.stdin.close()
 try:child.wait(timeout=3)
 except subprocess.TimeoutExpired:child.terminate();child.wait()
 log.close()
