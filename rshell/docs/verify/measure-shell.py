#!/usr/bin/env python3
"""Real release shell in private virtual KWin + Hyprland; host shell is observed only.

After cutover, HYPRV_SHELL_HOST_PID selects the native live shell solely as an
environment source. It is never driven/stopped and has no legacy comparison row.
"""
import argparse, ctypes, datetime, hashlib, importlib.util, json, os
from pathlib import Path
import select, shutil, signal, socket, struct, subprocess, tempfile, time
import xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'docs/verify/raw/perf-shell';OUT.mkdir(parents=True,exist_ok=True)
spec=importlib.util.spec_from_file_location('verify',ROOT/'crates/hyprv-svc-probe/scripts/verify.py')
v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
BIN=Path(os.environ.get('HYPRV_SHELL_BINARY',str(ROOT/'packaging/dist/hyprv-shell')))

def env_from_live():
 native_pid=os.environ.get('HYPRV_SHELL_HOST_PID')
 if native_pid:
  if not native_pid.isdecimal()or int(native_pid)<=1:raise RuntimeError('Invalid native host PID')
  proc=Path('/proc')/native_pid
  if (proc/'comm').read_text().strip()!='hyprv-shell':raise RuntimeError('Native host PID is not hyprv-shell')
  raw=dict(pair.split(b'=',1)for pair in(proc/'environ').read_bytes().split(b'\0')if b'='in pair)
  env=os.environ.copy()
  for key in ('XDG_RUNTIME_DIR','WAYLAND_DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','DBUS_SESSION_BUS_ADDRESS','DISPLAY'):
   if key.encode()in raw:env[key]=raw[key.encode()].decode()
  return env,[]
 legacy_source=Path(os.environ.get('HYPRV_SHELL_LEGACY_SOURCE',str(ROOT.parent/'quickshell')))
 rows=v.processes();qs=[p for p,r in rows.items() if r['comm'] in ('quickshell','qs') and str(legacy_source) in r['argv']]
 if len(qs)!=1:raise RuntimeError('Expected exactly one main Quickshell config')
 raw=dict(pair.split(b'=',1) for pair in Path(f'/proc/{qs[0]}/environ').read_bytes().split(b'\0') if b'=' in pair)
 env=os.environ.copy()
 for key in ('XDG_RUNTIME_DIR','WAYLAND_DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','DBUS_SESSION_BUS_ADDRESS','DISPLAY'):
  if key.encode() in raw:env[key]=raw[key.encode()].decode()
 return env,qs

def gpu(pids):
 r=subprocess.run(['nvidia-smi','-q','-x'],capture_output=True,text=True,timeout=10)
 try:
  data=ET.fromstring(r.stdout);rows=[]
  for device in data.findall('gpu'):
   for process in device.findall('processes/process_info'):
    if int(process.findtext('pid','0')) in pids:rows.append({'pid':int(process.findtext('pid')), 'gpu':device.findtext('product_name'), 'type':process.findtext('type'),'used_memory':process.findtext('used_memory')})
  return {'rows':rows,'returncode':r.returncode,'method':'nvidia-smi -q -x (graphics and compute processes)'}
 except ET.ParseError:return {'unavailable':r.stderr}

def request(path,text):
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(3);s.connect(str(path));s.sendall(text);result=b''
  while True:
   part=s.recv(65536)
   if not part:break
   result+=part
  return result.decode()

def shell_request(runtime,target,function,args=()):
 # Core IPC is newline-framed JSON; activity does not spawn CLI clients.
 data=json.dumps({'target':target,'function':function,'args':list(args)}).encode()
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(5);s.connect(str(runtime/'hyprv/shell.sock'));s.sendall(data+b'\n')
  return s.recv(65536).decode()

class HookWatch:
 def __init__(self,path):
  self.lib=ctypes.CDLL(None,use_errno=True);self.fd=self.lib.inotify_init1(os.O_NONBLOCK|os.O_CLOEXEC)
  if self.fd<0:raise OSError(ctypes.get_errno(),'inotify_init1')
  self.path=path;self.previous=self.semantic();self.wd=self.lib.inotify_add_watch(self.fd,os.fsencode(path),0x8|0x80|0x100|0x200)
 def semantic(self):
  try:
   data=json.loads((self.path/'state.json').read_text());result={}
   for sid,row in data.get('sessions',{}).items():
    if not isinstance(row,dict):continue
    signature={k:row.get(k) for k in ('source','event','status','current_tool','detail','last_activity')}
    result[hashlib.sha256(sid.encode()).hexdigest()[:16]]=(hashlib.sha256(json.dumps(signature,sort_keys=True).encode()).hexdigest(),row.get('event')=='ProcessScan')
   return result
  except (OSError,ValueError):return {}
 def read(self):
  events=[]
  while True:
   try:data=os.read(self.fd,65536)
   except BlockingIOError:break
   offset=0
   while offset+16<=len(data):
    wd,mask,cookie,length=struct.unpack_from('iIII',data,offset);name=data[offset+16:offset+16+length].split(b'\0')[0].decode(errors='replace');offset+=16+length
    if name=='state.json':
     current=self.semantic();changed=set(k for k in set(current)|set(self.previous) if current.get(k)!=self.previous.get(k));scan=sum(current.get(k,self.previous.get(k,('',False)))[1] for k in changed)
     events.append({'epoch':time.time(),'mask':mask,'name':name,'sessions_changed':len(changed),'process_scan_only_changes':scan,'non_scan_changes':len(changed)-scan});self.previous=current
  return events
 def close(self):os.close(self.fd)

class Nested:
 def __init__(self,host):
  self.host=host;self.runtime=Path(tempfile.mkdtemp(prefix='hyprv-perf-'));self.children=[];self.logs=[];self.env=None
 def launch(self,cmd,env,name):
  log=(OUT/(name+'.log')).open('w');self.logs.append(log)
  child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True);self.children.append(child);return child
 def __enter__(self):
  try:
   self.setup();return self
  except BaseException:self.__exit__(None,None,None);raise
 def setup(self):
  home=self.runtime/'compositor-home';home.mkdir()
  (home/'.config').mkdir()
  (home/'.config/kwinrulesrc').write_text('[General]\ncount=1\n\n[1]\nDescription=Private performance viewport\nwmclass=aquamarine\nwmclassmatch=1\nsize=6144,3456\nsizerule=2\nposition=0,0\npositionrule=2\nnoborder=true\nnoborderrule=2\n')
  bus_env=self.host.copy()
  bus_env.update({'HOME':str(home),'XDG_RUNTIME_DIR':str(self.runtime),'XDG_CONFIG_HOME':str(home/'.config'),'XDG_CACHE_HOME':str(home/'.cache'),'XDG_STATE_HOME':str(home/'.local/state'),'XDG_DATA_HOME':str(home/'.local/share'),'QT_QPA_PLATFORM':'offscreen','DBUS_SYSTEM_BUS_ADDRESS':'unix:path='+str(self.runtime/'absent-system-bus')})
  for key in ('DBUS_SESSION_BUS_ADDRESS','DBUS_STARTER_ADDRESS','WAYLAND_DISPLAY','DISPLAY','HYPRLAND_INSTANCE_SIGNATURE'):bus_env.pop(key,None)
  buslog=(OUT/'private-bus.err').open('w');self.logs.append(buslog)
  bus=subprocess.Popen(['dbus-daemon','--session','--nofork','--print-address=1'],stdout=subprocess.PIPE,stderr=buslog,text=True,start_new_session=True,env=bus_env);self.children.append(bus)
  address=bus.stdout.readline().strip()
  if not address:raise RuntimeError('No private compositor bus')
  env=bus_env.copy();env.update({'XDG_RUNTIME_DIR':str(self.runtime),'HOME':str(home),'DBUS_SESSION_BUS_ADDRESS':address,'DBUS_SYSTEM_BUS_ADDRESS':address,'QT_QPA_PLATFORM':'offscreen','HYPRLAND_NO_SD_VARS':'1','HYPRLAND_NO_SD_TARGET':'1'})
  for key in ('WAYLAND_DISPLAY','DISPLAY','HYPRLAND_INSTANCE_SIGNATURE'):env.pop(key,None)
  env['XDG_CONFIG_HOME']=str(home/'.config')
  env['XDG_CACHE_HOME']=str(home/'.cache')
  env['XDG_STATE_HOME']=str(home/'.local/state')
  parent=self.launch(['kwin_wayland','--virtual','--socket','parent','--width','6144','--height','3456','--no-lockscreen','--no-global-shortcuts','--no-kactivities'],env,'parent')
  self.wait(lambda:(self.runtime/'parent').exists(),parent,20)
  config=self.runtime/'hyprland.lua';config.write_text('hl.monitor({output="",mode="6144x3456@60",position="auto",scale=2})\nhl.config({general={gaps_in=0,gaps_out=0,border_size=0},animations={enabled=false},decoration={rounding=0,shadow={enabled=false},blur={enabled=false}},misc={disable_hyprland_logo=true,disable_splash_rendering=true},xwayland={enabled=false}})\n')
  shutil.copyfile(config,OUT/'hyprland.lua')
  check=subprocess.run(['/home/wilf/.codex/skills/hyprland-lua/scripts/check-hyprland-lua.sh',str(config)],capture_output=True,text=True);(OUT/'lua-validation.log').write_text(check.stdout+check.stderr)
  if check.returncode:raise RuntimeError('Lua syntax check failed')
  env.update({'WAYLAND_DISPLAY':str(self.runtime/'parent'),'AQ_BACKEND':'headless'})
  # Compositor may use Mesa llvmpipe for its virtual output; shell is not forced to software.
  env['LIBGL_ALWAYS_SOFTWARE']='1'
  # Same finite Wayland-request flush shim as --nested-test. Compositor only;
  # the measured shell inherits neither this preload nor software rendering.
  shim=self.runtime/'nested-flush.so'
  subprocess.run(['cc','-shared','-fPIC',str(ROOT/'crates/hyprv-shell/src/nested-flush.c'),'-o',str(shim),'-ldl'],check=True)
  env['LD_PRELOAD']=str(shim)
  comp=self.launch(['Hyprland','--config',str(config)],env,'compositor')
  def find():
   sockets=[p for p in self.runtime.glob('wayland-*') if p.is_socket()];instances=list((self.runtime/'hypr').glob('*/.socket.sock'))
   if not sockets or not instances:return False
   self.socket=sockets[0];self.command=instances[0]
   try:return bool(json.loads(request(self.command,b'j/monitors')))
   except (OSError,ValueError):return False
  self.wait(find,comp,30)
  self.parent_bus=address
  self.env=self.host.copy();self.env.update({'XDG_RUNTIME_DIR':str(self.runtime),'WAYLAND_DISPLAY':str(self.socket),'HYPRLAND_INSTANCE_SIGNATURE':self.command.parent.name})
  self.env.pop('DISPLAY',None);self.env.pop('LIBGL_ALWAYS_SOFTWARE',None)
  self.env['HYPRV_ASSET_ROOT']=os.environ.get('HYPRV_ASSET_ROOT',str(BIN.parent/'release/runtime'))
  self.env['HYPRV_ICON_ROOT']=str(Path.home()/'.local/share/icons')
  self.env['PIPEWIRE_RUNTIME_DIR']=self.host['XDG_RUNTIME_DIR']
  # Aquamarine's initial xdg size may be 1280x720 despite the monitor rule.
  # Resize only the private KWin window, then require exact negotiated geometry.
  # Preserve actual service buses/config/agent inputs; isolate shell IPC and cache/state writes.
  for key in ('XDG_CACHE_HOME','XDG_STATE_HOME'):
   p=self.runtime/key;p.mkdir();self.env[key]=str(p)
  (OUT/'nested-session.json').write_text(json.dumps({'compositor_pid':comp.pid,'parent_pid':parent.pid,'parent_bus':address,'runtime':str(self.runtime),'monitors':json.loads(request(self.command,b'j/monitors')),'configerrors':request(self.command,b'configerrors'),'status':request(self.command,b'j/status'),'host_instance':self.host.get('HYPRLAND_INSTANCE_SIGNATURE'),'shell_bus':'original host session and system buses; read-only service actions','shell_environment_keys_overridden':['XDG_RUNTIME_DIR','WAYLAND_DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','XDG_CACHE_HOME','XDG_STATE_HOME']},indent=2)+'\n')
  # Map one sacrificial read-only client so KWin can size the nested compositor.
  # Start measured clients only after this resize; they see final output globals.
  warmup=self.launch([str(BIN),'--read-only','--no-exclusive'],self.env,'warmup')
  self.wait(lambda:(self.runtime/'hyprv/shell.sock').exists(),warmup,20)
  time.sleep(2)
  self.resize()
  os.killpg(warmup.pid,signal.SIGTERM);warmup.wait(timeout=8)
  time.sleep(.5)
 def resize(self):
  modes=json.loads(request(self.command,b'j/monitors'))
  if modes and all(m['width']==6144 and m['height']==3456 and m['scale']==2 for m in modes):return
  resize=self.runtime/'resize.js';resize.write_text('workspace.windowList().forEach(function(w) { w.noBorder = true; w.frameGeometry = {x:0,y:0,width:6144,height:3456}; });\n')
  parent_env=self.env.copy();parent_env['DBUS_SESSION_BUS_ADDRESS']=self.parent_bus
  for attempt in range(30):
   for args in (['/Scripting','org.kde.kwin.Scripting.loadScript',str(resize),'perf-resize-'+str(attempt)],['/Scripting','org.kde.kwin.Scripting.start']):
    result=subprocess.run(['qdbus6','org.kde.KWin',*args],env=parent_env,capture_output=True,text=True,timeout=5)
    if result.returncode:raise RuntimeError('Private KWin resize failed: '+result.stderr)
   time.sleep(.3)
   modes=json.loads(request(self.command,b'j/monitors'))
   if modes and all(m['width']==6144 and m['height']==3456 and m['scale']==2 for m in modes):break
  else:raise RuntimeError('Private compositor did not negotiate 6144x3456@2')
  path=OUT/'nested-session.json';record=json.loads(path.read_text());record['initial_monitors']=record['monitors'];record['monitors']=modes;path.write_text(json.dumps(record,indent=2)+'\n')
 def wait(self,test,child,seconds):
  deadline=time.monotonic()+seconds
  while not test():
   if child.poll() is not None:raise RuntimeError(f'Child {child.pid} exited {child.returncode}')
   if time.monotonic()>deadline:raise RuntimeError('Nested readiness timeout; see compositor/parent logs')
   time.sleep(.05)
 def __exit__(self,*args):
  for source in self.runtime.glob('hypr/*/hyprland.log'):
   try:shutil.copy2(source,OUT/'compositor-runtime.log')
   except OSError:pass
  for p in reversed(self.children):
   if p.poll() is None:
    os.killpg(p.pid,signal.SIGTERM)
    try:p.wait(timeout=4)
    except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
  for f in self.logs:f.close()
  shutil.rmtree(self.runtime,ignore_errors=True)


def measure(nested,qs,label,active=False,seconds=60,quiet=False,profile=False,quiet_hooks=False):
 env=nested.env.copy()
 cmd=[str(BIN),'--read-only','--no-exclusive']
 if quiet or quiet_hooks:
  fixture=nested.runtime/'quiet-agent-input';fixture.mkdir(exist_ok=True);(fixture/'state.json').write_text('{"version":1,"sessions":{},"pending_requests":{}}\n')
  env['HYPRV_AGENT_ISLAND_DIR']=str(fixture);env['HYPRV_CODEX_APP_SERVER_SOCK']=str(fixture/'absent.sock')
  if quiet:cmd=['unshare','--user','--map-current-user','--pid','--fork','--mount-proc',*cmd]
 if profile:cmd=['perf','record','-F','997','--call-graph','dwarf,8192','-e','cpu-clock:u','-o',str(OUT/(label+'.data')),'--delay','12000','--',*cmd]
 shell=nested.launch(cmd,env,'shell-'+label)
 nested.wait(lambda:(nested.runtime/'hyprv/shell.sock').exists(),shell,20)
 time.sleep(1)
 nested.resize()
 time.sleep(12) # cold start / provider hydration / initial animation outside measured windows
 (OUT/(label+'-data-status.json')).write_text(shell_request(nested.runtime,'bar','dataStatus'))
 if shell.poll() is not None:raise RuntimeError('Shell exited before measurement')
 layers=json.loads(request(nested.command,b'j/layers'))
 bars=[layer for output in layers.values() for level in output['levels'].values() for layer in level if layer['namespace']=='hyprv-quickshell']
 if len(bars)!=1 or bars[0]['w']!=3072:raise RuntimeError('Wrong native bar geometry: '+repr(bars))
 record_path=OUT/'nested-session.json';record=json.loads(record_path.read_text());record.setdefault('initial_monitors',record['monitors']);record['monitors']=json.loads(request(nested.command,b'j/monitors'));record_path.write_text(json.dumps(record,indent=2)+'\n')
 (OUT/(label+'-layers.json')).write_text(json.dumps(layers,indent=2))
 host_hooks=HookWatch(Path.home()/'.cache/hyprv/agent-island');host_hooks.read()
 hooks=HookWatch(Path(env.get('HYPRV_AGENT_ISLAND_DIR',str(Path.home()/'.cache/hyprv/agent-island'))));hooks.read()
 actual=shell.pid
 if quiet or profile:
  rows=v.processes();family={shell.pid}
  while True:
   expanded=family|{pid for pid,row in rows.items() if row['ppid'] in family}
   if expanded==family:break
   family=expanded
  def measured_executable(pid):
   try:return os.path.samefile(Path('/proc')/str(pid)/'exe',BIN)
   except OSError:return False
  actual=next(pid for pid in family if measured_executable(pid))
 executed_sha=hashlib.sha256(Path(f'/proc/{actual}/exe').read_bytes()).hexdigest()
 g0=gpu([actual,*qs]);p0=v.proc_snapshot([actual]);q0=v.proc_snapshot(qs);start=time.monotonic();last=start;samples=[];events=[];host_events=[];actions=[]
 comparison=f'legacy={qs}, observation-only' if qs else 'no legacy comparison; native host is environment-only'
 print(f'MEASURE {label}: shell={actual} (launcher {shell.pid}), {seconds}s; {comparison}',flush=True)
 while time.monotonic()-start<seconds:
  elapsed=time.monotonic()-start
  if active and elapsed//5>=(len(actions)):
   target,function=('calendar','toggle') if len(actions)%4 in (0,1) else ('controlPanel','toggle')
   response=shell_request(nested.runtime,target,function)
   if not json.loads(response).get('ok'):raise RuntimeError('Activity IPC rejected: '+response)
   actions.append({'elapsed':elapsed,'target':target,'function':function,'response':response})
  wait=min(1,seconds-(time.monotonic()-start))
  if wait>0:select.select([hooks.fd,host_hooks.fd],[],[],wait)
  events.extend(hooks.read());host_events.extend(host_hooks.read())
  # Snapshot at ~1s intervals for event-conditioned thread CPU; no state JSON serialization in shell.
  if time.monotonic()-last>=.98:
   samples.append({'elapsed':time.monotonic()-start,'shell':v.proc_snapshot([actual]),'legacy':v.proc_snapshot(qs),'hook_count':len(events)});last=time.monotonic()
 p1=v.proc_snapshot([actual]);q1=v.proc_snapshot(qs);elapsed=time.monotonic()-start;g1=gpu([actual,*map(int,q1['processes'])]);hooks.close();host_hooks.close()
 if shell.poll() is not None:raise RuntimeError('Shell exited during interval')
 result={'schema':1,'label':label,'instrumented':profile or os.environ.get('HYPRV_SHELL_DIAGNOSTIC')=='1','build_profile':os.environ.get('HYPRV_SHELL_BUILD_PROFILE','release'),'agent_input_mode':'quiescent agent fixture + private PID namespace; all actors retained' if quiet else ('frozen hook fixture; host process discovery and agent animations retained' if quiet_hooks else 'live hooks + host process discovery'),'host_hook_events':host_events,'measured_pid':actual,'launcher_pid':shell.pid,'argv':cmd,'legacy_roots':qs,'binary':str(BIN),'binary_sha256':executed_sha,'seconds':elapsed,'shell':v.delta(p0,p1,elapsed),'legacy':v.delta(q0,q1,elapsed),'shell_before':p0,'shell_after':p1,'legacy_before':q0,'legacy_after':q1,'gpu_before':g0,'gpu_after':g1,'hook_events':events,'actions':actions,'samples':samples,'event_boundary':'Filesystem state.json write/replace events observed separately; existing agent animations and periodic discovery are not subtracted automatically.'}
 # The IPC client wire framing must be validated by successful responses before interpreting activity.
 (OUT/(label+'.json')).write_text(json.dumps(result,indent=2)+'\n')
 print(label, 'shell CPU',result['shell']['cpu_percent'],
       *(['legacy CPU',result['legacy']['cpu_percent']]if qs else ['legacy unavailable']),
       'hook events',len(events),flush=True)
 os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=8)
 return result

def main():
 global OUT
 parser=argparse.ArgumentParser();parser.add_argument('--label',default='baseline');parser.add_argument('--seconds',type=int,default=60);parser.add_argument('--idle-only',action='store_true');parser.add_argument('--quiet-agents',action='store_true');parser.add_argument('--profile',action='store_true');parser.add_argument('--quiet-hooks',action='store_true');a=parser.parse_args()
 OUT=OUT/a.label;OUT.mkdir(parents=True,exist_ok=True)
 host,qs=env_from_live()
 (OUT/'host-monitors.json').write_text(subprocess.check_output(['hyprctl','-j','monitors','all'],env=host,text=True))
 source_root=Path(os.environ.get('HYPRV_SHELL_SOURCE_ROOT',str(ROOT)))
 gate_spec=importlib.util.spec_from_file_location('shell_gate_inputs',ROOT/'cutover/preflight.py');gate=importlib.util.module_from_spec(gate_spec);gate_spec.loader.exec_module(gate)
 files=gate.shell_sources(source_root)
 build_command=['cargo','build','--locked',*(['--release'] if os.environ.get('HYPRV_SHELL_BUILD_PROFILE','release')=='release' else []),'-p','hyprv-shell']
 receipt=json.loads(Path(os.environ['HYPRV_SHELL_INPUTS_JSON']).read_text()) if os.environ.get('HYPRV_SHELL_INPUTS_JSON') else {'command':build_command,'source_root':str(source_root),'files':files}
 (OUT/(a.label+'-sources.json')).write_text(json.dumps(receipt,indent=2)+'\n')
 with Nested(host) as nested:
  measure(nested,qs,a.label+'-idle',False,a.seconds,a.quiet_agents,a.profile,a.quiet_hooks)
  if not a.idle_only:measure(nested,qs,a.label+'-activity',True,a.seconds,a.quiet_agents,a.profile,a.quiet_hooks)
if __name__=='__main__':
 def stop(*_):raise SystemExit(143)
 signal.signal(signal.SIGTERM,stop)
 main()
