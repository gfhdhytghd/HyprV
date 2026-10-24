#!/usr/bin/env python3
"""Own private compositors and pinned test ELF; mutate ONLY their native sockets/PIDs.
Buffers are read every frame, hashes retained, PNGs only for mismatches (disk policy).
"""
import argparse,hashlib,json,os,pathlib,shutil,signal,socket,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[3]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('name');p.add_argument('--backend',choices=['weston','hypr'],default='weston');p.add_argument('--epochs',type=int,default=3);p.add_argument('--frames',type=int,default=1200);p.add_argument('--rich',action='store_true');p.add_argument('--no-readback',action='store_true');p.add_argument('--no-audit',action='store_true');p.add_argument('--inject-copy',action='store_true');p.add_argument('--restart-live',action='store_true');p.add_argument('--dual-rates',action='store_true');p.add_argument('--output-kind',choices=['headless','wayland'],default='headless');a=p.parse_args()
assert a.name.replace('-','').isalnum()
out=ROOT/'crates/hyprv-ui-bar/artifacts'/a.name;out.mkdir(parents=True,exist_ok=True)
watched=[*sorted((ROOT/'crates/hyprv-ui/src').rglob('*.rs')),ROOT/'crates/hyprv-ui/examples/lifecycle_stress.rs',pathlib.Path(__file__),ROOT/'crates/hyprv-ui-bar/tools/stress_outputs.c']
def hashes():return {str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest()for f in watched}
def wait_socket(path,proc):
 for _ in range(300):
  if path.exists():return
  if proc.poll() is not None:raise RuntimeError(f'private compositor died:{proc.returncode}')
  time.sleep(.05)
 raise RuntimeError(f'private socket timeout:{path}')
rows=[];events=[];before=hashes();processes=[]
with tempfile.TemporaryDirectory(prefix='hv-lifecycle-')as tmp:
 runtime=pathlib.Path(tmp);home=runtime/'home';home.mkdir();binary=runtime/'lifecycle_stress';shutil.copy2(ROOT/'target/debug/examples/lifecycle_stress',binary)
 sha=hashlib.sha256(binary.read_bytes()).hexdigest()
 env={**os.environ,'XDG_RUNTIME_DIR':str(runtime),'HOME':str(home),'XDG_CONFIG_HOME':str(home/'.config'),'XDG_CACHE_HOME':str(home/'.cache'),'XDG_DATA_HOME':str(home/'.local/share'),'XDG_STATE_HOME':str(home/'.local/state'),'HYPRV_TEST_PRIVATE_WAYLAND':'1','HYPRV_LIFECYCLE_FRAMES':str(a.frames),'HYPRV_LIFECYCLE_TICK_US':'1000'}
 for k in ['DISPLAY','WAYLAND_DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','LD_PRELOAD','HYPRV_PARITY_BACKEND']:env.pop(k,None)
 if not a.no_audit:env.update(HYPRV_GPU_SELF_CHECK='1',HYPRV_GPU_SELF_CHECK_LOG='1')
 if a.rich:env['HYPRV_LIFECYCLE_RICH']='1'
 if a.no_readback:env['HYPRV_LIFECYCLE_NO_READBACK']='1'
 if a.inject_copy:env['HYPRV_GPU_SELF_CHECK_INJECT']='1'
 # A private bus isolates compositor activation from the user's services.
 bus=subprocess.Popen(['dbus-daemon','--session','--nofork','--print-address=1'],stdout=subprocess.PIPE,stderr=(out/'bus.log').open('w'))
 processes.append(bus);env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().decode().strip();env['DBUS_SYSTEM_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS']
 def spawn(cmd,e,log):
  f=(out/log).open('w');child=subprocess.Popen(cmd,env=e,stdout=f,stderr=f);processes.append(child);return child
 def ipc(signature,request):
  path=runtime/'hypr'/signature/'.socket.sock';assert path.is_relative_to(runtime)
  with socket.socket(socket.AF_UNIX)as s:
   s.settimeout(4);s.connect(str(path));s.sendall(request.encode());data=b''
   while b:=s.recv(65536):data+=b
  try:return json.loads(data)
  except ValueError:return data.decode(errors='replace')
 try:
  if a.backend=='hypr':
   parent=spawn(['kwin_wayland','--virtual','--socket','parent','--width','1024','--height','768','--no-lockscreen','--no-global-shortcuts','--no-kactivities'],{**env,'QT_QPA_PLATFORM':'offscreen'},'parent.log');wait_socket(runtime/'parent',parent)
   shim=runtime/'flush.so';subprocess.run(['cc','-shared','-fPIC',str(ROOT/'crates/hyprv-shell/src/nested-flush.c'),'-o',str(shim),'-ldl'],check=True)
   config=runtime/'hyprland.lua';config.write_text('hl.monitor({output="",mode="800x600@60",position="auto",scale=1})\nhl.config({general={gaps_in=0,gaps_out=0,border_size=0},animations={enabled=false},decoration={rounding=0,shadow={enabled=false},blur={enabled=false}},misc={disable_hyprland_logo=true,disable_splash_rendering=true},xwayland={enabled=false}})\n')
  if a.dual_rates:
   flags=subprocess.check_output(['pkg-config','--cflags','--libs','weston','libweston-15']).decode().split()
   subprocess.run(['cc','-shared','-fPIC',str(ROOT/'crates/hyprv-ui-bar/tools/stress_outputs.c'),'-o',str(runtime/'outputs.so'),*flags],check=True)
  for epoch in range(a.epochs):
   destination=out/f'epoch-{epoch}';destination.mkdir(exist_ok=True)
   if a.backend=='weston':
    display='audit';comp=spawn(['weston','--backend=headless','--renderer=gl','--shell=desktop','--no-config','--idle-time=0','--socket='+display,'--refresh-rate='+str(60000 if a.dual_rates else [60000,75000,144000][epoch%3])]+(['--modules='+str(runtime/'outputs.so')]if a.dual_rates else []),env,f'compositor-{epoch}.log');wait_socket(runtime/display,comp);signature=None
   else:
    old=set(runtime.glob('wayland-*'));ce={**env,'WAYLAND_DISPLAY':str(runtime/'parent'),'AQ_BACKEND':'headless','LD_PRELOAD':str(shim),'HYPRLAND_NO_SD_VARS':'1'}
    comp=spawn(['Hyprland','--config',str(config)],ce,f'compositor-{epoch}.log')
    for _ in range(300):
     sockets=[v for v in runtime.glob('wayland-*')if v not in old and not v.name.endswith('.lock')]
     if sockets:break
     if comp.poll()is not None:raise RuntimeError('private Hyprland died')
     time.sleep(.05)
    else:raise RuntimeError('private Hyprland socket timeout')
    display=str(sockets[0]);signature=next((runtime/'hypr').iterdir()).name
   se={**env,'WAYLAND_DISPLAY':display,'HYPRV_LIFECYCLE_SEED':str(epoch)}
   if epoch==0 and a.frames>1000:se['WAYLAND_DEBUG']='client'
   if epoch%2:se.update(HYPRV_GPU_STRESS_PURGE_AFTER_PAINT='1',HYPRV_GPU_ROBUST_CONTEXT='1')
   child=spawn([str(binary),str(destination)],se,f'epoch-{epoch}.log');last=-1;mutation=0;paused=False;disconnected=False;deadline=time.monotonic()+240
   try:
    while child.poll()is None:
     if time.monotonic()>deadline:raise RuntimeError('lifecycle child timeout')
     progress=destination/'progress.json'
     try:frame=json.loads(progress.read_text())['frame']
     except(FileNotFoundError,json.JSONDecodeError):frame=-1
     if frame>last:
      last=frame
      if frame>=99 and frame%400==99:
       # Only a Popen-owned compositor PID is ever stopped; always resume in finally.
       comp.send_signal(signal.SIGSTOP);paused=True;time.sleep(.08);comp.send_signal(signal.SIGCONT);paused=False;events.append({'epoch':epoch,'frame':frame,'event':'compositor-callback-pause','milliseconds':80})
      if a.restart_live and epoch==0 and frame>=399 and not disconnected:
       comp.terminate();comp.wait(timeout=10);disconnected=True;events.append({'epoch':epoch,'frame':frame,'event':'private-compositor-terminated-during-rendering'})
      if signature and not disconnected:
       monitors=ipc(signature,'j/monitors');name=monitors[0]['name']
       if mutation%4==0:
        response=ipc(signature,'/eval hl.monitor({output="AUDIT-SECOND",mode="800x600@144",position="800x0",scale=1});hl.monitor({output="'+name+'",mode="800x600@60",position="0x0",scale=1})');response2=ipc(signature,'/output create '+a.output_kind+' AUDIT-SECOND')
       elif mutation%4==1:response=ipc(signature,'/eval hl.monitor({output="'+name+'",mode="800x600@60",position="0x0",scale=1.25,transform=1});hl.monitor({output="AUDIT-SECOND",mode="800x600@144",position="800x0",scale=1.5})');response2=None
       elif mutation%4==2:response=ipc(signature,'/eval hl.monitor({output="'+name+'",mode="800x600@60",position="0x0",scale=2,transform=3})');response2=ipc(signature,'/output remove AUDIT-SECOND')
       else:response=ipc(signature,'/eval hl.monitor({output="'+name+'",mode="800x600@60",position="0x0",scale=1,transform=0})');response2=None
       events.append({'epoch':epoch,'frame':frame,'mutation':mutation,'response':response,'response2':response2,'monitors':ipc(signature,'j/monitors')});mutation+=1
     time.sleep(.008)
    code=child.wait();
    actual_code=code
    if disconnected:
     log=(out/f'epoch-{epoch}.log').read_text();assert code==1 and 'panicked' not in log
     completed=json.loads((destination/'frames-checkpoint.json').read_text());assert completed and all(r['lifecycle_control_exact']for r in completed)
     events.append({'epoch':epoch,'event':'disconnect-exits-cleanly','code':code,'last_verified_frames':len(completed)});code=0
    if a.inject_copy:
     log=(out/f'epoch-{epoch}.log').read_text();assert code!=0 and 'GPU self-check mismatch' in log;code=0
    rows.append({'epoch':epoch,'returncode':code,'actual_returncode':actual_code,'expected_disconnect':disconnected,'expected_negative_control':a.inject_copy,'result':json.loads((destination/'results.json').read_text())if(destination/'results.json').exists()else None})
   finally:
    if paused:comp.send_signal(signal.SIGCONT)
    if child.poll()is None:child.terminate();child.wait(timeout=10)
   if comp.poll() is None:comp.terminate();comp.wait(timeout=10)
   events.append({'epoch':epoch,'event':'private-compositor-destroyed; next epoch creates fresh connection/context'})
   # Own sockets only. Old Hyprland signatures do not describe the next epoch.
   for f in runtime.glob('wayland-*'):
    if f.is_socket()or f.name.endswith('.lock'):f.unlink(missing_ok=True)
   if (runtime/'audit').exists():(runtime/'audit').unlink()
   if (runtime/'audit.lock').exists():(runtime/'audit.lock').unlink()
   if (runtime/'hypr').exists():shutil.rmtree(runtime/'hypr')
 finally:
  for child in reversed(processes):
   if child.poll()is None:
    child.send_signal(signal.SIGCONT);child.terminate()
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:child.kill();child.wait()
 after=hashes();report={'binary_sha256':sha,'backend':a.backend,'args':vars(a),'epochs':rows,'events':events,'sources_before':before,'source_drift':[k for k in before if before[k]!=after[k]],'runtime':'private temporary directory removed; live sockets and compositor PIDs never used'}
 (out/'receipt.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:v for k,v in report.items()if k not in ['epochs','events','sources_before']},indent=2))
 if not all(r['returncode']==0 for r in rows):raise SystemExit(1)
