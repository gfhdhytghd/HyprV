#!/usr/bin/env python3
"""Non-installing production-shell soak in an isolated user/PID/network namespace.

All IPC and input target a verified private runtime. Filesystem is read-only
except the test output and its private runtime. Never imports live-session env.
"""
import argparse, collections, datetime, hashlib, json, math, os, select
from pathlib import Path
import shutil, signal, socket, statistics, subprocess, sys, tempfile, time
import xml.etree.ElementTree as ET

TOOLS=Path(__file__).resolve().parent
ROOT=TOOLS.parents[2]
HZ=os.sysconf('SC_CLK_TCK')

def write_json(path,data):
    tmp=path.with_name('.'+path.name+'.new')
    tmp.write_text(json.dumps(data,indent=2)+'\n');tmp.replace(path)

def request(path,data,timeout=3):
    assert '/hv-soak-' in str(path), path
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(timeout);stream.connect(str(path));stream.sendall(data)
        reply=b''
        while True:
            part=stream.recv(65536)
            if not part:break
            reply+=part
            if data.endswith(b'\n') and b'\n' in reply:break
        return reply.decode()

def ipc(runtime,target,fn,*args):
    result=json.loads(request(runtime/'hyprv/shell.sock',json.dumps(dict(target=target,function=fn,args=list(args))).encode()+b'\n'))
    if not result.get('ok'):raise RuntimeError(result)
    if result.get('result'):
        try:return json.loads(result['result'])
        except ValueError:return result['result']
    return None

def snapshot(pid):
    proc=Path('/proc')/str(pid)
    status={line.split(':',1)[0]:line.split(':',1)[1].strip() for line in (proc/'status').read_text().splitlines() if ':' in line}
    mem={line.split(':',1)[0]:int(line.split()[1]) for line in (proc/'smaps_rollup').read_text().splitlines() if ':' in line and line.split()[1].isdigit()}
    stat=(proc/'stat').read_text().rsplit(')',1)[1].split()
    fds=list((proc/'fd').iterdir());kinds=collections.Counter();timers=[]
    for fd in fds:
        try:
            name=os.readlink(fd);kind=name if name.startswith('anon_inode:') else ('socket' if name.startswith('socket:') else 'other')
            kinds[kind]+=1
            if 'timerfd' in name:
                info=(proc/'fdinfo'/fd.name).read_text()
                timers.append(dict(fd=int(fd.name),info=info,armed='it_value: (0, 0)' not in info))
        except FileNotFoundError:pass
    tasks=[]
    for task in (proc/'task').iterdir():
        try:
            fields=(task/'stat').read_text().rsplit(')',1)[1].split()
            tasks.append(dict(tid=int(task.name),name=(task/'comm').read_text().strip(),ticks=int(fields[11])+int(fields[12]),wchan=(task/'wchan').read_text().strip()))
        except FileNotFoundError:pass
    return dict(rss_kib=int(status['VmRSS'].split()[0]),pss_kib=mem['Pss'],private_dirty_kib=mem.get('Private_Dirty'),swap_kib=mem.get('Swap'),fds=len(fds),fd_types=dict(kinds),threads=int(status['Threads']),cpu_ticks=int(stat[11])+int(stat[12]),tasks=tasks,timerfds=timers,armed_timerfds=sum(t['armed'] for t in timers),posix_timers=(proc/'timers').read_text())

def gpu(binary):
    try:
        p=subprocess.run(['nvidia-smi','-q','-x'],capture_output=True,text=True,timeout=8)
        tree=ET.fromstring(p.stdout);rows=[]
        for device in tree.findall('gpu'):
            for item in device.findall('processes/process_info'):
                name=item.findtext('process_name','')
                if str(binary) in name:
                    rows.append(dict(pid=item.findtext('pid'),name=name,memory=item.findtext('used_memory'),type=item.findtext('type'),gpu=device.findtext('product_name')))
        return dict(returncode=p.returncode,rows=rows,available=bool(rows),note='NVML process accounting; namespace may hide host PID. No device-global usage attributed to shell.')
    except Exception as e:return dict(available=False,error=str(e))

class Session:
    def __init__(self,out,runtime):self.out=out;self.runtime=runtime;self.children=[];self.files=[]
    def launch(self,name,cmd,env,pipe=False):
        log=(self.out/(name+'.log')).open('w');self.files.append(log)
        child=subprocess.Popen(cmd,env=env,stdin=subprocess.PIPE if pipe else subprocess.DEVNULL,stdout=subprocess.PIPE if pipe else log,stderr=log,text=pipe,start_new_session=True)
        self.children.append(child);return child
    def wait(self,test,child,seconds=30):
        end=time.monotonic()+seconds
        while not test():
            if child.poll() is not None:raise RuntimeError(f'child {child.pid} exited {child.returncode}')
            if time.monotonic()>end:raise RuntimeError('readiness timed out')
            time.sleep(.05)
    def cleanup(self):
        for child in reversed(self.children):
            if child.poll() is None:
                try:
                    os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=8)
                except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
        for f in self.files:f.close()

def inside(args):
    out=args.out;runtime=args.runtime
    assert os.getuid()!=0 and str(runtime).startswith('/tmp/hv-soak-')
    assert os.environ.get('SOAK_SANDBOX')=='1'
    home=runtime/'home';home.mkdir();(home/'.config').mkdir()
    env={k:v for k,v in os.environ.items() if k in ('PATH','LANG','LC_ALL','TZ','XDG_DATA_DIRS')}
    env.update(HOME=str(home),XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(home/'.config'),XDG_CACHE_HOME=str(home/'.cache'),XDG_STATE_HOME=str(home/'.local/state'),XDG_DATA_HOME=str(home/'.local/share'),QT_QPA_PLATFORM='offscreen',HYPRLAND_NO_SD_VARS='1',HYPRLAND_NO_SD_TARGET='1')
    # No activation service dirs; no inherited session or system bus.
    conf=runtime/'bus.conf';conf.write_text('<busconfig><type>session</type><listen>unix:path='+str(runtime/'bus')+'</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    session=Session(out,runtime);summary=dict(status='starting',seconds_requested=args.seconds,isolation='bubblewrap: user/PID/network/IPC/UTS namespaces, readonly root, private HOME/runtime/config/cache/state, no bus activation',pid_namespace=os.readlink('/proc/self/ns/pid'),network_namespace=os.readlink('/proc/self/ns/net'))
    write_json(out/'status.json',summary)
    try:
        bus=session.launch('bus',['dbus-daemon','--nofork','--config-file='+str(conf)],env)
        session.wait(lambda:(runtime/'bus').is_socket(),bus)
        address='unix:path='+str(runtime/'bus');env.update(DBUS_SESSION_BUS_ADDRESS=address,DBUS_SYSTEM_BUS_ADDRESS=address)
        state=runtime/'media.json';write_json(state,dict(track=0,playing=False,sleep_sequence=0))
        mock=session.launch('mock-services',[sys.executable,str(TOOLS/'mock-services.py'),str(state),str(out/'mock-events.jsonl')],env)
        session.wait(lambda:(out/'mock-events.jsonl').exists() and 'ready' in (out/'mock-events.jsonl').read_text(),mock)
        (home/'.config/kwinrulesrc').write_text('[General]\ncount=1\n\n[1]\nDescription=Isolated soak viewport\nwmclass=aquamarine\nwmclassmatch=1\nsize=1280,720\nsizerule=2\nposition=0,0\npositionrule=2\nnoborder=true\nnoborderrule=2\n')
        parent=session.launch('parent',['kwin_wayland','--virtual','--socket','parent','--width','1280','--height','720','--no-lockscreen','--no-global-shortcuts','--no-kactivities'],env)
        session.wait(lambda:(runtime/'parent').is_socket(),parent)
        config=runtime/'hyprland.lua'
        config.write_text('hl.monitor({output="",mode="1280x720@60",position="auto",scale=1})\nhl.config({general={gaps_in=0,gaps_out=0,border_size=0},animations={enabled=false},decoration={rounding=0,shadow={enabled=false},blur={enabled=false}},misc={disable_hyprland_logo=true,disable_splash_rendering=true},xwayland={enabled=false}})\n')
        shutil.copyfile(config,out/'hyprland.lua')
        comp_env=env|dict(WAYLAND_DISPLAY=str(runtime/'parent'),AQ_BACKEND='headless',LIBGL_ALWAYS_SOFTWARE='1',LD_PRELOAD=str(out/'nested-flush.so'))
        comp=session.launch('compositor',['Hyprland','--config',str(config)],comp_env)
        sockets=[]
        def compositor_ready():
            nonlocal sockets
            sockets=[p for p in runtime.glob('wayland-*') if p.is_socket()]
            return sockets and bool(list((runtime/'hypr').glob('*/.socket.sock')))
        session.wait(compositor_ready,comp)
        command=next((runtime/'hypr').glob('*/.socket.sock'))
        env.update(WAYLAND_DISPLAY=str(sockets[0]),HYPRLAND_INSTANCE_SIGNATURE=command.parent.name)
        def compositor_request(cmd):return request(command,cmd.encode())
        session.wait(lambda:bool(json.loads(compositor_request('j/monitors'))),comp)
        write_json(out/'compositor-info.json',dict(monitors=json.loads(compositor_request('j/monitors')),version=compositor_request('j/version'),status=compositor_request('j/status'),errors=compositor_request('configerrors'),compositor_pid=comp.pid,parent_pid=parent.pid,private_socket=str(command)))
        agents=runtime/'agents';agents.mkdir()
        agentfile=agents/'state.json';write_json(agentfile,dict(version=1,sessions={},pending_requests={}))
        shell_env=env|dict(HYPRV_CONFIG_DIR=str(home/'.config/HyprV'),HYPRV_AGENT_ISLAND_DIR=str(agents),HYPRV_CODEX_APP_SERVER_SOCK=str(runtime/'absent-app.sock'),HYPRV_ASSET_ROOT=str(ROOT.parent/'quickshell'),HYPRV_ICON_ROOT='/home/wilf/.local/share/icons',PIPEWIRE_RUNTIME_DIR=str(runtime),LD_PRELOAD=str(out/'telemetry.so'),SOAK_TRACE=str(out/'trace.log'))
        shell=session.launch('shell',[str(out/'hyprv-shell-release'),'--read-only','--no-exclusive'],shell_env)
        session.wait(lambda:(runtime/'hyprv/shell.sock').is_socket(),shell)
        session.wait(lambda:bool(ipc(runtime,'bar','inputStatus')),shell)
        pointer=session.launch('pointer',[str(out/'pointer'),str(sockets[0])],env,True)
        def pointer_cmd(text):
            pointer.stdin.write(text+'\n');pointer.stdin.flush()
            if not select.select([pointer.stdout],[],[],3)[0]:raise RuntimeError('private pointer timeout')
            if pointer.stdout.readline().strip()!='ok':raise RuntimeError('private pointer failed')
        def move(x,y):pointer_cmd(f'move {x} {y}')
        def click(x,y):move(x,y);pointer_cmd('button 1');pointer_cmd('button 0')
        summary.update(status='running',shell_pid=shell.pid,start_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),start_monotonic=time.monotonic(),binary_sha256=hashlib.sha256((out/'hyprv-shell-release').read_bytes()).hexdigest(),driver_pid=os.getpid(),runtime=str(runtime))
        write_json(out/'status.json',summary)
        events=(out/'events.jsonl').open('w',buffering=1);samples=(out/'samples.jsonl').open('w',buffering=1)
        start=time.monotonic();next_sample=0;next_gpu=0;step_index=0;errors=0
        schedule=[(0,'settled'),(20,'calendar-open'),(27,'calendar-scroll'),(35,'calendar-close'),(45,'control-open'),(60,'control-close'),(65,'stats-hover'),(72,'stats-close'),(80,'volume'),(90,'brightness-preview'),(100,'agent-processing'),(108,'agent-hover'),(112,'agent-waiting'),(118,'agent-scroll'),(124,'agent-completed'),(128,'agent-clear'),(132,'media-play'),(140,'island-hover'),(146,'calendar-over-media'),(152,'calendar-over-media-close'),(158,'media-pause'),(162,'clear'),(165,'workspace'),(168,'hotplug-add'),(172,'hotplug-remove'),(175,'resume')]
        actions=[((cycle*180+t)/args.activity_rate,name,cycle) for cycle in range(math.ceil(args.seconds*args.activity_rate/180)) for t,name in schedule]
        while (elapsed:=time.monotonic()-start)<args.seconds:
            if shell.poll() is not None:raise RuntimeError(f'shell exited {shell.returncode} at {elapsed}')
            if comp.poll() is not None or parent.poll() is not None:raise RuntimeError('private compositor exited')
            while step_index<len(actions) and actions[step_index][0]<=elapsed:
                scheduled,name,cycle=actions[step_index];step_index+=1
                event=dict(elapsed=elapsed,scheduled=scheduled,name=name,cycle=cycle,monotonic_ns=time.monotonic_ns())
                try:
                    if name=='settled':move(400,400)
                    elif name=='calendar-scroll':move(640,170);pointer_cmd('scroll 15');pointer_cmd('scroll -15')
                    elif name.startswith('calendar-'):event['reply']=ipc(runtime,'calendar','toggle');move(640,29 if name.endswith('open') else 400)
                    elif name.startswith('control-'):event['reply']=ipc(runtime,'controlPanel','toggle');move(500,150 if name.endswith('open') else 400)
                    elif name=='stats-hover':move(25,29)
                    elif name=='stats-close':move(400,400);click(400,400)
                    elif name=='volume':event['reply']=ipc(runtime,'quickAdjust','showVolume')
                    elif name=='brightness-preview':event['reply']=ipc(runtime,'quickAdjust','showBrightnessLevel',str(25+cycle%60))
                    elif name=='agent-hover':move(640,29)
                    elif name=='agent-scroll':move(640,160);pointer_cmd('scroll 30');pointer_cmd('scroll -15')
                    elif name=='agent-clear':write_json(agentfile,dict(version=1,updated_at=time.time(),sessions={},pending_requests={}));move(400,400)
                    elif name.startswith('agent-'):
                        status={'agent-processing':'processing','agent-waiting':'waitingApproval','agent-completed':'completed'}[name]
                        now=time.time();write_json(agentfile,dict(version=1,updated_at=now,sessions={f'soak-session-{j}':dict(id=f'soak-session-{j}',source='codex' if j%2 else 'claude',source_label='Codex' if j%2 else 'Claude Code',title=f'Soak regression {j}',project='isolated-fixture',cwd=str(home),status=status,event='PermissionRequest' if status=='waitingApproval' else 'PreToolUse',current_tool='Read',detail='Synthetic observation only',progress=.3,last_activity=now,had_activity=True,terminal={'pid':os.getpid()}) for j in range(12)},pending_requests={}))
                    elif name=='media-play':write_json(state,dict(track=cycle+1,playing=True,present=True,sleep_sequence=cycle))
                    elif name=='media-pause':write_json(state,dict(track=cycle+1,playing=False,present=True,sleep_sequence=cycle));move(400,400)
                    elif name=='island-hover':move(640,29)
                    elif name=='clear':write_json(agentfile,dict(version=1,updated_at=time.time(),sessions={},pending_requests={}));write_json(state,dict(track=cycle+1,playing=False,present=False,sleep_sequence=cycle));move(400,400);click(400,400)
                    elif name=='workspace':event['reply']=compositor_request('dispatch hl.dsp.focus({workspace='+str(cycle%5+1)+'})')
                    elif name in ('hotplug-add','hotplug-remove'):
                        event['reply']=compositor_request('output create headless SOAK-HOTPLUG' if name=='hotplug-add' else 'output remove SOAK-HOTPLUG')
                        if event['reply'].strip()!='ok':raise RuntimeError('private hotplug rejected: '+event['reply'])
                        for attempt in range(30):
                            event['monitors']=json.loads(compositor_request('j/monitors'))
                            present=any(m['name']=='SOAK-HOTPLUG' for m in event['monitors'])
                            if present==(name=='hotplug-add'):break
                            time.sleep(.05)
                        else:raise RuntimeError('hotplug not reflected in monitor enumeration')
                    elif name=='resume':write_json(state,dict(track=cycle+1,playing=False,sleep_sequence=cycle+1))
                    event['ok']=True
                except Exception as error:errors+=1;event.update(ok=False,error=str(error))
                event['duration_ms']=(time.monotonic_ns()-event['monotonic_ns'])/1e6
                events.write(json.dumps(event)+'\n')
            if elapsed>=next_sample:
                t=time.monotonic();sample=dict(elapsed=elapsed,epoch=time.time(),shell=snapshot(shell.pid),errors=errors,completed_steps=step_index)
                try:
                    sample['ipc']=ipc(runtime,'bar','dataStatus');sample['ipc_ms']=(time.monotonic()-t)*1000
                    sample['layers']=json.loads(compositor_request('j/layers'))
                except Exception as e:sample['probe_error']=str(e)
                if elapsed>=next_gpu:sample['gpu']=gpu(out/'hyprv-shell-release');next_gpu=elapsed+60
                samples.write(json.dumps(sample)+'\n');next_sample=elapsed+5
                summary.update(elapsed=elapsed,samples=int(elapsed//5)+1,completed_steps=step_index,driver_errors=errors,last_sample=sample['shell'],last_ipc_ms=sample.get('ipc_ms'))
                write_json(out/'status.json',summary)
            time.sleep(.05)
        summary.update(status='complete',elapsed=time.monotonic()-start,end_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),driver_errors=errors,completed_steps=step_index)
        write_json(out/'status.json',summary)
        ipc(runtime,'shell','kill');shell.wait(timeout=10)
        summary['shell_exit_code']=shell.returncode;write_json(out/'status.json',summary)
        events.close();samples.close()
    except BaseException as error:
        summary.update(status='failed',error=repr(error));write_json(out/'status.json',summary);raise
    finally:session.cleanup()

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--seconds',type=float,default=3720);parser.add_argument('--binary',type=Path,default=ROOT/'target/release/hyprv-shell');parser.add_argument('--activity-rate',type=float,default=1.);parser.add_argument('--out',type=Path,required=True);parser.add_argument('--runtime',type=Path);parser.add_argument('--inside',action='store_true');args=parser.parse_args();args.out=args.out.resolve()
    if args.inside:return inside(args)
    assert os.getuid()!=0
    out=args.out;out.mkdir(parents=True,exist_ok=False)
    for name in ('telemetry','nested-flush'):
        subprocess.run(['cc','-shared','-fPIC','-O2',str(TOOLS/(name+'.c')),'-o',str(out/(name+'.so')),'-ldl'],check=True)
    subprocess.run(['wayland-scanner','client-header',str(TOOLS/'virtual-pointer.xml'),str(out/'pointer-protocol.h')],check=True)
    subprocess.run(['wayland-scanner','private-code',str(TOOLS/'virtual-pointer.xml'),str(out/'pointer-protocol.c')],check=True)
    subprocess.run(['cc','-O2',str(TOOLS/'pointer.c'),str(out/'pointer-protocol.c'),'-I'+str(out),'-lwayland-client','-o',str(out/'pointer')],check=True)
    shutil.copyfile(args.binary.resolve(),out/'hyprv-shell-release');(out/'hyprv-shell-release').chmod(0o755)
    files=[ROOT/'Cargo.toml',ROOT/'Cargo.lock',*ROOT.glob('crates/*/Cargo.toml'),*ROOT.glob('crates/*/src/**/*.rs')]
    write_json(out/'provenance.json',dict(binary_sha256=hashlib.sha256((out/'hyprv-shell-release').read_bytes()).hexdigest(),build_command=['cargo','build','--release','--locked','-p','hyprv-shell'],sources_at_launch={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},tools={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in TOOLS.iterdir() if p.is_file()},uid=os.getuid()))
    runtime=Path(tempfile.mkdtemp(prefix='hv-soak-'));runtime.chmod(0o700)
    env={k:v for k,v in os.environ.items() if k in ('PATH','LANG','LC_ALL','TZ')};env['SOAK_SANDBOX']='1'
    cmd=['bwrap','--unshare-user','--unshare-pid','--unshare-net','--unshare-ipc','--unshare-uts','--die-with-parent','--ro-bind','/','/','--proc','/proc','--dev-bind','/dev','/dev','--tmpfs','/tmp','--bind',str(runtime),str(runtime),'--bind',str(out),str(out),'--chdir',str(out),'--',sys.executable,str(Path(__file__).resolve()),'--inside','--seconds',str(args.seconds),'--activity-rate',str(args.activity_rate),'--out',str(out),'--runtime',str(runtime)]
    write_json(out/'launch.json',dict(command=cmd,env=env,runtime=str(runtime),host_pid_namespace=os.readlink('/proc/self/ns/pid'),host_network_namespace=os.readlink('/proc/self/ns/net')))
    try:
        result=subprocess.run(cmd,env=env)
        return result.returncode
    finally:
        # Runtime belongs exclusively to this run; no live socket exists here.
        shutil.rmtree(runtime,ignore_errors=True)

if __name__=='__main__':sys.exit(main())
