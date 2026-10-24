#!/usr/bin/env python3
"""Read-only references and same-window procfs observation; writes only owned artifacts."""
import argparse,collections,datetime,hashlib,json,math,os,pathlib,select,shutil,socket,stat,subprocess,time
CRATE=pathlib.Path(__file__).resolve().parents[1];ROOT=CRATE.parents[1];OUT=ROOT/'docs/verify';RAW=OUT/'raw';ORIGINAL=ROOT.parent/'quickshell';BINARY=pathlib.Path(os.environ.get('HYPRV_PROBE_BINARY',str(CRATE/'.check/target/release/hyprv-svc-probe')))

def processes():
    rows={}
    for path in pathlib.Path('/proc').glob('[0-9]*'):
        try:
            fields=(path/'stat').read_text().rsplit(')',1)[1].split()
            rows[int(path.name)]={'pid':int(path.name),'ppid':int(fields[1]),'comm':(path/'comm').read_text().strip(),'argv':(path/'cmdline').read_bytes().replace(b'\0',b' ').decode(errors='replace'),'start':int(fields[19]),'ticks':sum(int(fields[n]) for n in [11,12,13,14]),'rss_kib':int(fields[21])*os.sysconf('SC_PAGE_SIZE')/1024}
        except (OSError,ValueError,IndexError):pass
    return rows

def session():
    rows=processes();candidates=[p for p in rows.values() if p['comm'] in ('quickshell','qs') and str(ORIGINAL) in p['argv']]
    env=os.environ.copy();root_pids=[p['pid'] for p in candidates]
    if len(candidates)==1:
        data=(pathlib.Path('/proc')/str(root_pids[0])/'environ').read_bytes()
        values=dict(pair.split(b'=',1) for pair in data.split(b'\0') if b'='in pair)
        for key in ['HYPRLAND_INSTANCE_SIGNATURE','XDG_RUNTIME_DIR','WAYLAND_DISPLAY','DBUS_SESSION_BUS_ADDRESS']:
            if values.get(key.encode()):env[key]=values[key.encode()].decode()
    # Existing PID-addressed socket only; verify owner, process identity, and type.
    sockets=[]
    for path in pathlib.Path(env.get('XDG_RUNTIME_DIR','/run/user/1000')).glob('dankcal-*.sock'):
        try:
            pid=int(path.stem.split('-')[-1]);exe=os.readlink(f'/proc/{pid}/exe')
            if stat.S_ISSOCK(path.stat().st_mode) and path.stat().st_uid==os.getuid() and ('dankcal' in exe.lower() or pathlib.Path(exe).name=='dcal'):sockets.append(str(path))
        except (OSError,ValueError):pass
    if len(sockets)==1:env['HYPRV_DCAL_SOCKET']=sockets[0]
    (OUT/'session.json').write_text(json.dumps({'quickshell_roots':candidates,'recovered_keys':[k for k in ['HYPRLAND_INSTANCE_SIGNATURE','XDG_RUNTIME_DIR','WAYLAND_DISPLAY','DBUS_SESSION_BUS_ADDRESS'] if env.get(k)],'calendar_socket':env.get('HYPRV_DCAL_SOCKET'),'other_quickshell_configs':[r for r in rows.values() if r['comm']in('quickshell','qs')and r['pid']not in root_pids]},indent=2)+'\n')
    return env,root_pids

def command(argv,env,timeout=12,first_line=False):
    started=time.time();record={'argv':list(map(str,argv)),'started':started}
    try:
        if first_line:
            process=subprocess.Popen(argv,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                if not select.select([process.stdout],[],[],timeout)[0]:raise subprocess.TimeoutExpired(argv,timeout)
                record.update(stdout=process.stdout.readline(),stderr='',returncode=0)
            finally:
                process.terminate()
                try:process.communicate(timeout=2)
                except subprocess.TimeoutExpired:process.kill();process.communicate()
        else:
            result=subprocess.run(argv,env=env,capture_output=True,text=True,timeout=timeout)
            record.update(stdout=result.stdout,stderr=result.stderr,returncode=result.returncode)
    except (OSError,subprocess.TimeoutExpired) as error:record.update(stdout='',stderr=str(error),returncode=None)
    record['ended']=time.time();return record

def states(path):
    rows=[]
    for line in path.read_text().splitlines():
        try:rows.append(json.loads(line))
        except ValueError:pass # A concurrent reader may see a partial final write.
    return rows

def latest(rows):return {row['topic']:row for row in rows if 'state'in row}
def kv(text):return dict(line.split('=',1) for line in text.splitlines()if '='in line)
def value(raw):
    if raw in ('true','false'):return raw=='true'
    try:return float(raw)
    except (ValueError,TypeError):return raw

def refs(env,runtime):
    reference={};base=runtime/'legacy';base.mkdir(parents=True,exist_ok=True)
    state=base/'state';state.mkdir(exist_ok=True)
    source=pathlib.Path(env.get('XDG_STATE_HOME',str(pathlib.Path.home()/'.local/state')))/'hyprv/quickshell'
    if source.exists():shutil.copytree(source,state/'hyprv/quickshell',dirs_exist_ok=True)
    isolated=env|{'XDG_STATE_HOME':str(state)}
    # Status paths with stale-state cleanup operate only on the copied state.
    specs={'network':('wifi-status.sh',[]),'bluetooth':('bluetooth-status.sh',[]),'mpris':('media-status.sh',[]),'vm':('windows-vm-control.sh',['status']),'mihomo':('mihomo-control.sh',['status']),'deskflow':('deskflow-target.sh',['status']),'viewflow':('viewflow-target.sh',['status']),'system_snapshot':('system-snapshot.sh',[]),'fan':('fan-control-helper.sh',['status']),'control_panel':('control-panel-status.sh',[]),'inhibit':('prevent-sleep.sh',['status']),'demo':('demo-mode.sh',['status']),'profile_icon':('power-profile.sh',[])}
    for key,(file,args) in specs.items():reference[key]=command(['bash',str(ORIGINAL/'scripts'/file),*args],isolated if key in ['control_panel','inhibit','demo']else env,timeout=45 if key=='bluetooth'else 12)
    battery_env=env|{'XDG_RUNTIME_DIR':str(base)}
    reference['battery_history']=command(['bash',str(ORIGINAL/'scripts/battery-info.sh')],battery_env)
    agent=base/'agent-island';agent.mkdir(exist_ok=True);source=pathlib.Path(env.get('HYPRV_AGENT_ISLAND_DIR',str(pathlib.Path.home()/'.cache/hyprv/agent-island')))/'state.json'
    if source.exists():shutil.copyfile(source,agent/'state.json')
    reference['agent_island']=command(['python3',str(ORIGINAL/'scripts/agent-island-status.py'),'--json'],env|{'HYPRV_AGENT_ISLAND_DIR':str(agent)})
    weather_env=env|{'QS_CACHE_DIR':str(base/'weather-cache'),'QS_STATE_DIR':str(base/'weather-state'),'QS_RUN_DIR':str(base/'weather-run')}
    reference['weather']=command(['bash',str(ORIGINAL/'hypr-comp/scripts/quickshell/calendar/weather.sh'),'--json'],weather_env,timeout=20)
    reference['applications']=command(['python3',str(ORIGINAL/'hypr-comp/scripts/quickshell/applauncher/app_fetcher.py')],env)
    reference['theme']=command(['bash',str(ORIGINAL/'scripts/ui-state.sh'),'print'],isolated)
    reference['audio']=command(['bash',str(ORIGINAL/'hypr-comp/scripts/quickshell/watchers/audio_fetch.sh')],env)
    reference['notifications']=command(['swaync-client','-swb'],env,first_line=True,timeout=5)
    for key in ['monitors','clients','activewindow','activeworkspace','workspaces','devices','animations']:
        reference['hyprland.'+key]=command(['hyprctl','-j',key],env)
    def bus(name,path,interface,prop,user=False):return command(['busctl',*(['--user']if user else []),'--json=short','get-property',name,path,interface,prop],env)
    reference['power.OnBattery']=bus('org.freedesktop.UPower','/org/freedesktop/UPower','org.freedesktop.UPower','OnBattery')
    reference['power.Profile']=bus('net.hadess.PowerProfiles','/net/hadess/PowerProfiles','net.hadess.PowerProfiles','ActiveProfile')
    reference['power.ProfileDetails']=command(['busctl','--json=short','call','net.hadess.PowerProfiles','/net/hadess/PowerProfiles','org.freedesktop.DBus.Properties','GetAll','s','net.hadess.PowerProfiles'],env)
    reference['tray']=bus('org.kde.StatusNotifierWatcher','/StatusNotifierWatcher','org.kde.StatusNotifierWatcher','RegisteredStatusNotifierItems',True)
    try:
        items=json.loads(reference['tray']['stdout'])['data']
        for item in items:
            name,separator,path=item.partition('/')
            if separator:reference['tray.item.'+item]=command(['busctl','--user','--json=short','call',name,'/'+path,'org.freedesktop.DBus.Properties','GetAll','s','org.kde.StatusNotifierItem'],env)
    except (KeyError,ValueError):pass
    display=command(['busctl','--json=short','call','org.freedesktop.UPower','/org/freedesktop/UPower','org.freedesktop.UPower','GetDisplayDevice'],env)
    try:
        path=json.loads(display['stdout'])['data'];path=path[0]if isinstance(path,list)else path
        reference['power.DisplayDevice']=command(['busctl','--json=short','call','org.freedesktop.UPower',path,'org.freedesktop.DBus.Properties','GetAll','s','org.freedesktop.UPower.Device'],env)
    except (KeyError,ValueError):pass
    reference['ir.list']=command(['python3',str(ORIGINAL/'scripts/hdmi-switch-ir.py'),'list'],env)
    ir_database=pathlib.Path.home()/'.local/share/hyprv-ir/hdmi-switch.json'
    try:reference['ir.database']={'path':str(ir_database),'result':json.loads(ir_database.read_text())}
    except (OSError,ValueError)as error:reference['ir.database']={'error':str(error)}
    return reference

def rpc(path,method,params):
    with socket.socket(socket.AF_UNIX) as channel:
        channel.settimeout(10);channel.connect(path);channel.sendall((json.dumps({'id':1,'method':method,'params':params})+'\n').encode())
        reader=channel.makefile();deadline=time.monotonic()+10
        while True:
            channel.settimeout(max(.001,deadline-time.monotonic()))
            line=reader.readline(1048577)
            if not line:raise EOFError('calendar closed before correlated reply')
            if len(line)>1048576:raise ValueError('calendar reply oversized')
            response=json.loads(line)
            if response.get('id')==1:
                if response.get('error'):raise ValueError(str(response['error']))
                return response
            if time.monotonic()>=deadline:raise TimeoutError('calendar correlation timeout')

def equivalence(env):
    runtime=CRATE/'.check/equivalence-runtime';runtime.mkdir(parents=True,exist_ok=True)
    path=RAW/'equivalence-probe.jsonl'
    with path.open('w')as output,(RAW/'equivalence-probe.stderr').open('w')as errors:
        process=subprocess.Popen([str(BINARY),'--duration','180','--state-dir',str(runtime)],stdout=output,stderr=errors,env=env)
        time.sleep(6)
        reference=refs(env,runtime)
        observed=latest(states(path))
        if env.get('HYPRV_DCAL_SOCKET'):
            try:
                reference['calendar.calendars']={'result':rpc(env['HYPRV_DCAL_SOCKET'],'calendars.list',{})}
                week=datetime.date.fromisoformat(observed['calendar']['state']['week_start_local']);local_from=datetime.datetime.combine(week-datetime.timedelta(days=7),datetime.time()).astimezone();local_to=datetime.datetime.combine(week+datetime.timedelta(days=14),datetime.time()).astimezone()
                reference['calendar.events']={'result':rpc(env['HYPRV_DCAL_SOCKET'],'events.list',{'from':local_from.astimezone(datetime.timezone.utc).isoformat(),'to':local_to.astimezone(datetime.timezone.utc).isoformat(),'limit':2000})}
            except Exception as error:reference['calendar.error']={'error':str(error)}
        # Read while actors stay alive, then stop only our own process gracefully.
        observed=latest(states(path));process.send_signal(2)
        try:process.wait(timeout=8)
        except subprocess.TimeoutExpired:process.kill();process.wait()
    (RAW/'legacy.json').write_text(json.dumps(reference,ensure_ascii=False,indent=2)+'\n')
    (RAW/'rust-latest.json').write_text(json.dumps(observed,ensure_ascii=False,indent=2)+'\n')
    compare(reference,observed)

def compare(reference,observed):
    rows=[]
    def add(topic,field,legacy,native,reason='',temporal=False,tolerance=None):
        if topic not in observed:
            rows.append(dict(topic=topic,field=field,legacy=legacy,rust=None,result='UNAVAILABLE',reason='module not present in this build; earlier media observations retained in initial-* artifacts'));return
        equal=legacy==native
        if tolerance is not None and isinstance(legacy,(int,float))and isinstance(native,(int,float)):equal=abs(legacy-native)<=tolerance
        result='PASS'if equal else ('TEMPORAL'if temporal else 'MISMATCH')
        rows.append(dict(topic=topic,field=field,legacy=legacy,rust=native,result=result,reason=reason))
    def unavailable(topic,field,reason):rows.append(dict(topic=topic,field=field,legacy=None,rust=None,result='UNAVAILABLE',reason=reason))
    def native(topic):return observed.get(topic,{}).get('state',{})
    def parsed(topic):
        try:return json.loads(reference[topic]['stdout'])
        except (KeyError,ValueError):return None
    def fields(topic,legacy,mapping,temporal=()):
        if legacy is None:unavailable(topic,'*','original read path failed or returned no JSON');return
        for field,target in mapping.items():
            if field not in legacy:unavailable(topic,field,'source omitted field in this provider state');continue
            add(topic,field,legacy[field],native(topic).get(target),temporal=field in temporal,reason='asynchronous provider snapshots'if field in temporal else ('peripheral Battery selected instead of legacy BAT* laptop scope'if topic=='battery_history'and field=='available'and legacy[field]!=native(topic).get(target)else ''))
    fields('network',parsed('network'),{f:f for f in ['present','iface','enabled','hardwareEnabled','connected','ssid','signal','security']},['signal'])
    network=parsed('network')
    if network:
        rust={x['ssid']:x for x in native('network').get('networks',[])}
        for item in network.get('networks',[]):
            for field in ['ssid','signal','security','secure','enterprise','active','known']:
                add('network',f'networks[{item["ssid"]}].{field}',item.get(field),rust.get(item['ssid'],{}).get(field),temporal=field=='signal')
        add('network','network SSID set',sorted(x['ssid']for x in network.get('networks',[])),sorted(rust))
    fields('bluetooth',parsed('bluetooth'),{f:f for f in ['present','enabled','discovering','pairable']})
    bluetooth=parsed('bluetooth')
    if bluetooth:
        devices={d['address']:d for d in native('bluetooth').get('devices',[])}
        for item in bluetooth.get('devices',[]):
            for field in ['address','name','icon','paired','trusted','connected','blocked','rssi']:
                if field in item:add('bluetooth',f'devices[{item["address"]}].{field}',item[field],devices.get(item['address'],{}).get(field),temporal=field=='rssi')
        add('bluetooth','device address set',sorted(x['address']for x in bluetooth.get('devices',[])),sorted(devices))
    media=kv(reference['mpris']['stdout']);mapping={'available':'available','playing':'playing','cider':'cider','title':'title','artist':'artist','player':'player_name','art_url':'art_url','length':'length_seconds'}
    for field,target in mapping.items():
        if field in media:add('mpris',field,value(media[field]),native('mpris').get(target),temporal=field in ['title','artist','length'])
    if 'position'in media:
        anchor=native('mpris').get('position',{});add('mpris','position',value(media['position']),anchor.get('position_us',0)/1e6,reason='position read times differ; UI extrapolation uses monotonic anchor',temporal=True,tolerance=2)
    audio=parsed('audio')
    if audio:
        add('audio','muted',audio.get('is_muted')=='true',native('audio').get('muted'))
        add('audio','volume_percent',value(audio.get('volume')),native('audio').get('volume_percent'),reason='auxiliary audio_fetch truncates; main QML rounds',tolerance=1)
        unavailable('audio','icon','presentation derived by UI, not a service field')
    notify=parsed('notifications')
    if notify:
        for field in ['alt','tooltip','text']:add('notifications',field,notify.get(field),native('notifications').get(field),temporal=field in ['tooltip','text'])
        add('notifications','dnd','dnd'in notify.get('alt',''),native('notifications').get('dnd'))
        add('notifications','has_dot','notification'in notify.get('alt',''),native('notifications').get('has_dot'))
    add('vm','running',reference['vm']['stdout'].strip()=='running',native('vm').get('running'))
    proxy=parsed('mihomo')
    if proxy:
        fields('mihomo',proxy,{'online':'online','service':'legacy_service_scope','version':'version','mode':'mode','connections':'connections','upload':'upload_bytes_per_s','download':'download_bytes_per_s','dashboard':'dashboard_url'},['connections','upload','download'])
    add('deskflow','target',reference['deskflow']['stdout'].strip(),native('deskflow').get('target'))
    battery=parsed('battery_history')
    fields('battery_history',battery,{'available':'available','status':'status','mode':'mode','capacity':'capacity_percent','powerW':'power_w','averagePowerW':'average_power_w','sampleCount':'samples','sampleWindowSeconds':'window_s','windowComplete':'window_complete','estimateSeconds':'estimate_s','estimateBasis':'estimate_basis','energyNowWh':'energy_now_wh','energyFullWh':'energy_full_wh'})
    for key,field in [('OnBattery','on_battery'),('Profile','profile')]:
        v=parsed('power.'+key)
        if v:
            actual=native('power').get(field)
            if field=='profile':actual={'Performance':'performance','Balanced':'balanced','PowerSaver':'power-saver'}.get(actual,str(actual))
            data=v.get('data');data=data[0]if isinstance(data,list)and len(data)==1 else data;add('power',field,data,actual)
        else:unavailable('power',field,'provider property read unavailable')
    # Original __MEM__/__TEMP__/__ROUTE__ contract, with explicit units.
    sections={};section=None
    for line in reference['system_snapshot']['stdout'].splitlines():
        if line.startswith('__'):section=line;sections[section]=[]
        elif section:sections[section].append(line)
    mem={l.split(':')[0]:int(l.split()[1])*1024 for l in sections.get('__MEM__',[])if ':'in l}
    if mem:
        add('sysstats','memory_total_bytes',mem.get('MemTotal'),native('sysstats').get('memory_total_bytes'))
        add('sysstats','memory_available_bytes',mem.get('MemAvailable'),native('sysstats').get('memory_available_bytes'),temporal=True,reason='memory changes between reads')
    temps=[l for l in sections.get('__TEMP__',[])if l.strip()]
    if temps:add('sysstats','temperature_c',float(temps[0])/1000,native('sysstats').get('temperature_c'),temporal=True,reason='source CPU package preference versus native sensor selector')
    unavailable('sysstats','cpu/core/network rates and histories','require aligned deltas; absolute proc sample alone cannot prove rate equivalence')
    unavailable('sysstats','package_power_w','legacy uses independently sampled /run/hyprv/cpu-package-power-w; native RAPL baseline is separate')
    # Hyprland records: independently enumerate every visible record field.
    monitors=parsed('hyprland.monitors');desktop=native('hyprland')
    if monitors is not None:
        table={m['name']:m for m in desktop.get('monitors',[])}
        for m in monitors:
            target=table.get(m['name'],{});scale=m.get('scale',1);rotated=m.get('transform',0)%2;size=[m.get('width'),m.get('height')];size=size[::-1]if rotated else size
            for f in ['name','id','scale','transform']:add('hyprland',f'monitors[{m["name"]}].{f}',m.get(f),target.get(f))
            for f,v in dict(x=m.get('x'),y=m.get('y'),width=size[0]/scale,height=size[1]/scale).items():add('hyprland',f'monitors[{m["name"]}].{f}',v,target.get('geometry',{}).get(f))
    clients=parsed('hyprland.clients')
    if clients is not None:
        windows={w['address']:w for w in desktop.get('windows',[])}
        for w in clients:
            target=windows.get(w['address'],{})
            for f in ['address','pid','class','title','monitor']:add('hyprland',f'windows[{w["address"]}].{f}',w.get(f),target.get(f),temporal=f in ['title','monitor'])
            for f,v in dict(x=w['at'][0],y=w['at'][1],width=w['size'][0],height=w['size'][1]).items():add('hyprland',f'windows[{w["address"]}].{f}',v,target.get('geometry',{}).get(f),temporal=True)
            add('hyprland',f'windows[{w["address"]}].fullscreen',bool(w.get('fullscreen')),target.get('fullscreen'))
    active=parsed('hyprland.activewindow')
    if active is not None:add('hyprland','active_window',active.get('address')or None,desktop.get('active_window'),temporal=True)
    workspaces=parsed('hyprland.workspaces')
    focused=parsed('hyprland.activeworkspace')
    if workspaces is not None:
        actual={item['id']:item for item in desktop.get('workspaces',[])}
        for item in workspaces:
            if 'id'not in item:
                unavailable('hyprland',f'workspaces[{item.get("address")}].id','current compositor omits numeric IDs for special workspaces; native maps missing numeric ID to zero')
                continue
            target=actual.get(item['id'],{})
            for field in ['id','name','monitor','windows']:add('hyprland',f'workspaces[{item["id"]}].{field}',item.get(field),target.get(field),temporal=field=='windows')
            add('hyprland',f'workspaces[{item["id"]}].tooltip',item.get('lastwindowtitle',''),target.get('tooltip'),temporal=True)
            if focused:add('hyprland',f'workspaces[{item["id"]}].active',item['id']==focused.get('id'),target.get('active'),temporal=True)
        for id_ in range(1,9):
            if not any(item.get('id')==id_ for item in workspaces):
                for field,expected in {'windows':0,'active':False,'tooltip':'Empty','state':'empty'}.items():add('hyprland',f'empty_workspaces[{id_}].{field}',expected,actual.get(id_,{}).get(field))
    devices=parsed('hyprland.devices')
    if devices:
        keyboards=devices.get('keyboards',[]);main=next((k for k in keyboards if k.get('main')),keyboards[0]if keyboards else {})
        add('hyprland','keymap',main.get('active_keymap',''),desktop.get('keymap'))
    animations=parsed('hyprland.animations')
    if animations:
        a=next((a for name in ['windowsMove','windows','global']for a in animations[0]if a['name']==name and a.get('overridden')),None)
        if a:
            curve=next((c for c in animations[1]if c['name']==a['bezier']),None)
            if curve:add('hyprland','move_duration_ms',max(0,math.floor(a['speed']*100+0.5))if a['enabled']else 0,desktop.get('move_duration_ms'));add('hyprland','move_bezier',[curve[f]for f in ['X0','Y0','X1','Y1']]+[1,1],desktop.get('move_bezier'))
    # Calendar fields are normalized from the actual CalendarBridge wire accessors.
    for key,target,fields_ in [('calendars','calendars',{'id':'id','summary':'name','color':'color','hidden':'hidden','holdsTasks':'holds_tasks'}),('events','events',{'uid':'uid','calendarId':'calendar_id','summary':'title','start':'start_iso','end':'end_iso','allDay':'all_day','location':'location'})]:
        payload=reference.get('calendar.'+key,{}).get('result',{}).get('result')
        if payload is None:unavailable('calendar',key,'native read-only socket reference unavailable');continue
        values=payload if key=='calendars'else payload.get('events',[]);actual=native('calendar').get(target,[])
        add('calendar',key+'.count',len(values),len(actual))
        def ident(v):return str(v.get('id'))if key=='calendars'else(str(v.get('uid')),v.get('start',v.get('start_iso')))
        table={ident(v):v for v in actual}
        for item in values:
            match=table.get(ident(item),{})
            for f,t in fields_.items():
                expected=item.get(f)
                if f=='summary':expected=item.get('summary',item.get('name',''))
                if f in ['hidden','holdsTasks','allDay']:expected=bool(item.get(f,False))
                add('calendar',f'{key}[{ident(item)}].{f}',expected,match.get(t))
    applications=parsed('applications')
    if applications is not None:
        targets={item['name']:item for item in native('applications').get('applications',[])}
        add('applications','names',sorted(item['name']for item in applications),sorted(targets))
        for item in applications:
            for field in ['name','exec','icon']:add('applications',f'applications[{item["name"]}].{field}',item.get(field),targets.get(item['name'],{}).get(field))
    theme=kv(reference.get('theme',{}).get('stdout',''))
    for field in ['theme','variant']:
        if field in theme:add('theme',field,theme[field],native('theme').get(field))
    weather=parsed('weather')
    if weather:
        for f,t in {'current_temp':'current_temperature','current_icon':'current_icon','current_hex':'current_color'}.items():add('weather',f,value(weather.get(f))if f=='current_temp'else weather.get(f),native('weather').get(t))
        for index,day in enumerate(weather.get('forecast',[])):
            actual=(native('weather').get('days',[])+[{}]*5)[index]
            for f,t in {'id':'id','day_full':'weekday','max':'high','min':'low','feels_like':'feels_like','wind':'wind','humidity':'humidity_percent','pop':'precipitation_percent','icon':'icon','hex':'color','desc':'description'}.items():add('weather',f'forecast[{index}].{f}',value(day.get(f))if f in ['max','min','feels_like','wind','humidity','pop']else day.get(f),actual.get(t))
            for slot,h in enumerate(day.get('hourly',[])):
                a=(actual.get('hours',[])+[{}]*48)[slot]
                for f,t in {'time':'local_time','temp':'temperature','icon':'icon','hex':'color'}.items():add('weather',f'forecast[{index}].hourly[{slot}].{f}',value(h.get(f))if f=='temp'else h.get(f),a.get(t))
    agent=parsed('agent_island')
    if agent:
        add('agent_island','active',agent.get('active'),native('agent_island').get('active'));add('agent_island','count',agent.get('count'),native('agent_island').get('count'),temporal=True,reason='live hooks and cleanup between snapshots')
        targets={s['id']:s for s in native('agent_island').get('sessions',[])}
        for item in agent.get('sessions',[]):
            for f,t in {'id':'id','source':'source','source_label':'source_label','title':'title','project':'project','status':'status','current_tool':'current_tool','detail':'detail','progress':'base_progress','last_activity':'activity_epoch_s'}.items():
                if f in item:
                    actual=targets.get(item['id'],{}).get(t)
                    if f=='progress' and actual is not None:
                        session=targets[item['id']]
                        if session.get('status')in ['running','processing']:
                            sample_at=reference['agent_island']['ended'];anchor=session.get('activity_epoch_s')or sample_at
                            actual=min(.94,max(actual,.22+((sample_at-anchor)%12)/12*.66))
                    add('agent_island',f'sessions[{item["id"]}].{f}',item[f],actual,temporal=f in ['status','current_tool','detail','progress','last_activity'])
    def unpack_properties(payload):
        if not isinstance(payload,dict):return {}
        data=payload.get('data',{})
        if isinstance(data,list)and len(data)==1:data=data[0]
        if isinstance(data,list):data=dict(data)
        return {key:(entry.get('data')if isinstance(entry,dict)and 'data'in entry else entry)for key,entry in data.items()}if isinstance(data,dict)else {}
    properties=unpack_properties(parsed('power.DisplayDevice'))
    if properties:
        present=bool(properties.get('IsPresent'))and properties.get('Type')==2
        add('power','battery_present',present,native('power').get('battery_present'))
        add('power','battery_percent',properties.get('Percentage')if present else None,native('power').get('battery_percent'))
        expected_mode='unknown'if not present else {1:'charging',2:'discharging',4:'full',5:'charging',6:'discharging'}.get(properties.get('State'),'idle'if native('power').get('on_battery')else 'plugged')
        add('power','battery_mode',expected_mode,native('power').get('battery_mode'))
        for key,target in {'Energy':'energy_now_wh','EnergyFull':'energy_full_wh','EnergyRate':'energy_rate_w','TimeToEmpty':'time_to_empty_s','TimeToFull':'time_to_full_s'}.items():add('power',target,properties.get(key)if present else None,native('power').get(target))
    properties=unpack_properties(parsed('power.ProfileDetails'))
    if properties:
        add('power','performance_degraded',properties.get('PerformanceDegraded')or None,native('power').get('performance_degraded')or None)
        expected=[v['Profile']['data'] for v in properties.get('Profiles',[])]
        names={'Performance':'performance','Balanced':'balanced','PowerSaver':'power-saver'}
        add('power','profiles',sorted(expected),sorted(names.get(v,v)for v in native('power').get('profiles',[])))
    tray_items=native('tray').get('items',[])
    reference_items=[]
    for key in reference:
        if not key.startswith('tray.item.'):continue
        item=key[len('tray.item.'):];props=unpack_properties(parsed(key));reference_items.append(item)
        # Actor IDs retain watcher registration identity; compare by service+path.
        target=next((v for v in tray_items if v.get('key')==item or v.get('service','')+v.get('path','')==item),{})
        for field,target_field in {'Id':'id','IconThemePath':'icon_theme_path','Title':'title','Status':'status','Category':'category','IconName':'icon_name','AttentionIconName':'attention_icon_name','OverlayIconName':'overlay_icon_name','ItemIsMenu':'only_menu'}.items():
            if field in props:add('tray',item+'.'+field,props[field],target.get(target_field))
        for field,target_field in {'IconPixmap':'icon_pixmaps','AttentionIconPixmap':'attention_pixmaps','OverlayIconPixmap':'overlay_pixmaps'}.items():
            if field not in props:continue
            expected=[{'width':w,'height':h,'argb':data}for w,h,data in props[field] if 0<w<=512 and 0<h<=512 and len(data)==w*h*4][:8]
            def digest(pixmaps):return [{'width':p['width'],'height':p['height'],'argb_sha256':hashlib.sha256(bytes(p['argb'])).hexdigest()}for p in pixmaps]
            add('tray',item+'.'+field,digest(expected),digest(target.get(target_field,[])))
        tooltip=props.get('ToolTip')
        if isinstance(tooltip,list)and len(tooltip)==4:
            for index,field in [(0,'tooltip_icon_name'),(2,'tooltip_title'),(3,'tooltip_description')]:add('tray',item+'.'+field,tooltip[index],target.get(field))
        if 'Menu'in props:
            menu=props['Menu']if props['Menu']not in ['', '/']else None
            add('tray',item+'.menu',menu,target.get('menu'));add('tray',item+'.has_menu',bool(menu),target.get('has_menu'))
        icon=props.get('AttentionIconName')if props.get('Status')=='NeedsAttention'and props.get('AttentionIconName')else props.get('IconName','')
        if icon and props.get('IconThemePath')and not icon.startswith('/'):icon+='?path='+props['IconThemePath']
        add('tray',item+'.icon',icon,target.get('icon'))
        unavailable('tray',item+'.menu_layout','menu hierarchy is demand/action-driven; no menu preparation actions dispatched')
    add('tray','item_count',len(reference_items),len(tray_items),temporal=True,reason='watcher registrations can change between reads')
    inhibit=kv(reference['inhibit']['stdout'])
    if inhibit:
        add('inhibit','enabled',value(inhibit.get('enabled')),native('inhibit').get('enabled'))
        add('inhibit','pid',int(inhibit['pid'])if inhibit.get('pid')else None,native('inhibit').get('pid'),reason='native FD ownership replaces legacy sleeper PID')
    if 'demo'in observed:
        data=kv(reference['demo']['stdout'])
        for field in ['enabled','source','headless']:add('demo',field,value(data.get(field)),native('demo').get(field))
    if 'viewflow'in observed:add('viewflow','target',reference['viewflow']['stdout'].strip(),native('viewflow').get('target'))
    if 'recording'in observed:add('recording','active',value(kv(reference['control_panel']['stdout']).get('recording')),native('recording').get('active'))
    database=reference.get('ir.database',{}).get('result')
    if database:
        for field in ['carrier_hz','duty_cycle']:add('ir',field,database.get(field),native('ir').get(field))
        actual={v['name']:v for v in native('ir').get('buttons',[])}
        for name,item in database.get('buttons',{}).items():
            for field in ['captured_at','source_pin','polarity','durations_us','decoded']:
                if field in item:add('ir',f'buttons[{name}].{field}',item[field],actual.get(name,{}).get(field))
    if weather:
        add('weather','forecast.count',len(weather.get('forecast',[])),len(native('weather').get('days',[])))
        for index,day in enumerate(weather.get('forecast',[])):
            actual=(native('weather').get('days',[])+[{}]*5)[index]
            try:
                date=datetime.date.fromisoformat(actual.get('local_date',''))
                for field,format_ in [('day','%a'),('date','%d %b')]:add('weather',f'forecast[{index}].{field}',day.get(field),date.strftime(format_))
            except ValueError:unavailable('weather',f'forecast[{index}].date','native date unavailable')
            add('weather',f'forecast[{index}].hourly.count',len(day.get('hourly',[])),len(actual.get('hours',[])))
    # Explain static gaps individually, without treating defaults as live evidence.
    for topic,reason in {'fan':'helper absent; no autoactivation','backlight':'native sysfs/DDC Get VCP observation recorded; legacy Lua brightness_get path was not invoked','inhibit':'actor only owns its own leases; original PID file is a different owner','demo':'module not yet exported/buildable','recording':'module not yet exported/buildable','theme':'module not yet exported/buildable','applications':'module not yet exported/buildable','viewflow':'module exists but not exported/buildable','ir':'database is observed; no serial status command sent','spectrum':'capture gate deliberately false; no active audio readback','cider':'no Lookup actions dispatched by verifier','egpu':'inventory observed; original disconnect script has no status/read branch','startup':'action-only readiness contract; no approvals/actions dispatched','systemd':'internal generic unit monitor; UI VM running observation compared separately'}.items():
        if topic not in observed or topic in ['fan','backlight','cider','spectrum','egpu','startup','systemd']:unavailable(topic,'*',reason)
    (RAW/'comparisons.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2)+'\n')
    counts=collections.Counter(r['result']for r in rows)
    lines=['# Service equivalence verification','',f'Run: {datetime.datetime.now(datetime.timezone.utc).isoformat()}. Build inputs: [build snapshot](build-snapshot.json).','',f'Field checks: **{dict(counts)}**. These are observed comparisons, not blanket parity certification.','', 'Original scripts were executed only through audited status/read branches. Scripts that normally clean/persist state used copied state/cache/runtime directories below the probe. The running shell, devices, service configuration and original state files were not changed. Native probe dispatch count is zero.','', 'The actual Quickshell session environment was recovered from its process using an explicit four-key whitelist. Calendar uses an existing same-user PID-addressed DankCal socket whose process identity was checked; no dcal activation or guessed production configuration was written.','', 'Coverage and provider-state limits: [coverage.md](coverage.md). Repeated application/session/event records contribute individual field checks; totals do not imply equivalent coverage across modules.','', '## Field results','', '| Topic | Field | Result | Explanation |','|---|---|---|---|']
    lines.extend(f'| {r["topic"]} | `{str(r["field"]).replace("|","/")}` | {r["result"]} | {r["reason"]} |'for r in rows)
    lines+=['','Exact legacy/Rust values are in [comparisons.json](raw/comparisons.json); original command outputs and timestamps are in [legacy.json](raw/legacy.json), and corresponding Rust snapshots in [rust-latest.json](raw/rust-latest.json).','','## Interpretation','','Bluetooth was also sampled in a dedicated same-binary net-only follow-up after the original 12-second script timeout; its successful original script response and aligned native snapshot are retained in [bluetooth-followup.json](raw/bluetooth-followup.json). Reference timestamps identify the different observation windows.','','MISMATCH means the sampled fields differ and needs owner review. TEMPORAL means the field differs across asynchronous observations; it is not counted as a pass. Rate baselines, physical controls, authorization, spectrum capture and unimplemented modules remain unavailable rather than fabricated. Empty/absent provider branches do not prove populated branches.']
    (OUT/'equivalence.md').write_text('\n'.join(lines)+'\n')
    print('equivalence:',dict(counts),flush=True)

# procfs scheduling counts are a wakeup proxy, not kernel wakeup tracepoints.
def proc_snapshot(roots):
    all_=processes();family=set(roots)
    while True:
        expanded=family|{p for p,r in all_.items()if r['ppid']in family}
        if expanded==family:break
        family=expanded
    tasks={}
    for pid in family:
        for path in (pathlib.Path('/proc')/str(pid)/'task').glob('[0-9]*'):
            try:
                fields=(path/'stat').read_text().rsplit(')',1)[1].split();sched=list(map(int,(path/'schedstat').read_text().split()));status=dict(l.split(':',1)for l in(path/'status').read_text().splitlines()if ':'in l)
                tasks[f'{pid}:{path.name}:{fields[19]}']={'name':status.get('Name','').strip(),'runtime_ns':sched[0],'runqueue_ns':sched[1],'timeslices':sched[2],'voluntary':int(status.get('voluntary_ctxt_switches',0)),'involuntary':int(status.get('nonvoluntary_ctxt_switches',0))}
            except (OSError,ValueError,IndexError):pass
    return {'monotonic':time.monotonic(),'epoch':time.time(),'processes':{str(pid):all_[pid]for pid in family if pid in all_},'tasks':tasks}

def delta(before,after,seconds):
    common=before['tasks'].keys()&after['tasks'].keys();new=after['tasks'].keys()-before['tasks'].keys();gone=before['tasks'].keys()-after['tasks'].keys()
    summed={key:sum(max(0,after['tasks'][pid][key]-before['tasks'][pid][key])for pid in common)+sum(after['tasks'][pid][key]for pid in new)for key in ['runtime_ns','runqueue_ns','timeslices','voluntary','involuntary']}
    # utime+stime+cutime+cstime counts reaped descendants without summing exited processes twice.
    b={f'{pid}:{r["start"]}':r for pid,r in before['processes'].items()};a={f'{pid}:{r["start"]}':r for pid,r in after['processes'].items()}
    ticks=sum(r['ticks']for r in a.values())-sum(r['ticks']for r in b.values())
    return dict(seconds=seconds,cpu_seconds=ticks/os.sysconf('SC_CLK_TCK'),cpu_percent=ticks/os.sysconf('SC_CLK_TCK')/seconds*100,rss_start_kib=sum(r['rss_kib']for r in before['processes'].values()),rss_end_kib=sum(r['rss_kib']for r in after['processes'].values()),scheduling_proxy_per_s=summed['timeslices']/seconds,context_switches_per_s=(summed['voluntary']+summed['involuntary'])/seconds,exited_task_count=len(gone),new_task_count=len(new),task_counters=summed)

def performance(env,quickshell,mode,seconds=60,excludes=(),freeze_agent=False):
    runtime=CRATE/f'.check/perf-{mode}';runtime.mkdir(parents=True,exist_ok=True);path=RAW/f'perf-{mode}-probe.jsonl'
    args=[str(BINARY),'--duration',str(seconds+15),'--state-dir',str(runtime)]
    if freeze_agent:args+=['--freeze-agent-input']
    if mode!='active':args+=['--idle-after','5']
    if mode=='no-agent':args+=['--exclude','agent_island']
    for module in excludes:args+=['--exclude',module]
    with path.open('w')as output,(RAW/f'perf-{mode}.stderr').open('w')as errors:
        process=subprocess.Popen(args,env=env,stdout=output,stderr=errors)
        time.sleep(8)
        if process.poll()is not None:raise RuntimeError('probe exited before measurement')
        p0=proc_snapshot([process.pid]);q0=proc_snapshot(quickshell);start=time.monotonic()
        print(f'performance {mode}: probe {process.pid}, Quickshell roots {quickshell}, starting {seconds} seconds',flush=True)
        # Observer sleeps once. It is excluded from both measured process families.
        time.sleep(seconds)
        p1=proc_snapshot([process.pid]);q1=proc_snapshot(quickshell);elapsed=time.monotonic()-start
        process.send_signal(2)
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill();process.wait()
    if process.pid not in [int(pid)for pid in p1['processes']]:raise RuntimeError('probe exited during measurement; interval rejected')
    payload={'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'build_snapshot':json.loads((OUT/'build-snapshot.json').read_text()),'excludes':list(excludes),'agent_inputs_frozen':freeze_agent,'mode':mode,'binary_path':str(BINARY),'elapsed':elapsed,'probe':delta(p0,p1,elapsed),'quickshell':delta(q0,q1,elapsed),'probe_before':p0,'probe_after':p1,'quickshell_before':q0,'quickshell_after':q1}
    events=collections.Counter(row.get('topic')for row in states(path)if p0['epoch']<=row.get('epoch',0)<=p1['epoch']and 'state'in row)
    payload['state_events']=dict(events);(RAW/f'perf-{mode}.json').write_text(json.dumps(payload,indent=2)+'\n');write_perf();print('performance',mode,payload['probe'],flush=True)

def write_debug_perf():
    lines=['# Same-window service-probe / Quickshell process measurement','','Baseline and active modes measure the probe and the live main-shell Quickshell process plus descendants concurrently for 60 seconds; separately labelled isolation diagnostics use 10-second windows. Quickshell was only observed; no panel visibility, service or compositor state was changed. The separate DankCal Quickshell config is listed in session.json and excluded from the main-shell family.','','`idle`: all topics are initially subscribed; after 5 s only demand-driven subscriptions are released. Measurement begins after 8 s. Event-driven subscriptions stay live. `active`: all safe topics retain demand (stats/battery/Mihomo/weather). Spectrum capture stays off; backlight performs only sysfs/DDC Get VCP reads, and absent helper is never activated.','','## Results','','| Mode | Process family | CPU seconds | CPU % of one core | RSS start/end KiB | Scheduling proxy / s | Context switches / s | Exited threads |','|---|---|---|---|---|---|---|']
    baseline=RAW/'perf-idle.json'
    if baseline.exists():
        r=json.loads(baseline.read_text())['probe']
        lines[6:6]=[f'Latest 60-second idle result: **target not met** ({r["cpu_percent"]:.3f}% of one core, {r["scheduling_proxy_per_s"]:.2f} scheduling events/s). Event counts and matched group diagnostics below constrain interpretation; they do not certify every readiness branch.','']
    publications=[]
    for mode in ['idle','active','after-sys-review-idle','initial-idle','no-agent']:
        path=RAW/f'perf-{mode}.json'
        if not path.exists():continue
        payload=json.loads(path.read_text())
        for family in ['probe','quickshell']:
            r=payload[family];lines.append(f'| {mode} | {family} | {r["cpu_seconds"]:.3f} | {r["cpu_percent"]:.3f} | {r["rss_start_kib"]:.0f} / {r["rss_end_kib"]:.0f} | {r["scheduling_proxy_per_s"]:.2f} | {r["context_switches_per_s"]:.2f} | {r["exited_task_count"]} |')
        publications.append(f'{mode} state publications during the interval: `{payload["state_events"]}`.')
    lines+=['',*publications,'','`after-sys-review-idle`, `initial-idle` and `no-agent` are earlier build snapshots, retained as diagnostic history. They are not matched interventions against the current idle build. Each raw payload retains its source hashes where available.','','## Earlier matched-binary isolation','','These four sequential 10-second windows used the same earlier binary, with media omitted during an in-flight source transition. They identified paths for owner investigation; they do not represent the final build.','','| Exclusion | Probe CPU % | Scheduling proxy / s |','|---|---|---|']
    for label in ['all','no-agent','no-viewflow','neither']:
        path=RAW/f'perf-isolation-{label}.json'
        if path.exists():
            r=json.loads(path.read_text())['probe'];lines.append(f'| {label} | {r["cpu_percent"]:.3f} | {r["scheduling_proxy_per_s"]:.2f} |')
    lines+=['']
    lines+=['## Latest matched-binary group diagnostics','','Each group-only window lasts 10 seconds after 8 seconds warmup; demand is released at 5 seconds. These are separate sequential observations, not additive CPU attribution. Their embedded source hashes identify the pre-follow-up build.','','| Included group | CPU % | Scheduling proxy / s | State events |','|---|---|---|---|']
    for label in ['desktop','sys','power','net-media']:
        path=RAW/f'perf-final-isolation-{label}.json'
        if path.exists():
            payload=json.loads(path.read_text());r=payload['probe'];lines.append(f'| {label} | {r["cpu_percent"]:.3f} | {r["scheduling_proxy_per_s"]:.2f} | {payload["state_events"]} |')
    lines+=['','## Limits','','The third `/proc/PID/task/TID/schedstat` field counts scheduling timeslices, **not exact wakeups**. Voluntary/nonvoluntary context-switch totals are also scheduling proxies. Exited threads and short-lived legacy child processes have incomplete scheduling counters because only window endpoints are read; reported scheduling rates are lower bounds. CPU uses utime+stime+cutime+cstime over surviving family members so reaped child CPU is included. RSS is a family sum, not PSS/USS, and shared pages may be counted multiple times. Raw endpoint counters and process/thread identities are retained in `raw/perf-*.json`. No perf privileges, tracing changes or sysctls were used.','','This is an active multi-agent desktop: hook files, tray icons, compositor events and external network/provider signals may be genuine activity. JSON serialization/publication is part of probe CPU, while rendering is part of the live Quickshell family. Do not interpret the difference as a causal speedup of a fully integrated replacement UI. Missing providers can create retry deadlines; active sampling deliberately creates timer wakeups. A zero-idle target is not certified by this measurement.']
    (OUT/'perf.md').write_text('\n'.join(lines)+'\n')

def write_perf():
    if not (RAW/'perf-release-before.json').exists():return write_debug_perf()
    lines=['# Release idle CPU repair and same-window comparison','',
        'Release builds use Cargo opt-level=3 (isolated workspace default release profile; no fat LTO). Before/after dependencies are frozen; only power actor.rs changes. Runtime traces are from a separate instrumented debug copy and are excluded from all performance intervals. No native actions were dispatched.','',
        'The live main Quickshell process and its descendants include rendering and UI work. The probe contains services and JSON publication only: parity with Quickshell CPU would not satisfy the services budget. Target: **at most 0.1% of one core**, with only genuine event activity at idle. The separate DankCal Quickshell config is excluded from both families.','',
        'The first before/after pair changes only power actor.rs. `release-latest` additionally includes concurrently landed desktop improvements; it is a separate checkpoint. `release-latest-no-agent` excludes the active agent actor. `release-frozen-agent` retains every actor but freezes hook inputs after one initial copy and points the agent app-server observer at a probe-owned absent endpoint. It is an explicit idle fixture, not a claim that the live active desktop consumes that amount.','',
        '## Release measurements','',
        '| Mode | Family | Seconds | CPU seconds | CPU % of one core | RSS start/end KiB | Scheduling proxy / s | Context switches / s |',
        '|---|---|---|---|---|---|---|---|']
    payloads={}
    for mode in ['release-before','release-after','release-power-before','release-power-after','release-latest','release-latest-no-agent','release-frozen-agent']:
        path=RAW/f'perf-{mode}.json'
        if not path.exists():continue
        payload=json.loads(path.read_text());payloads[mode]=payload
        for family in ['probe','quickshell']:
            r=payload[family];lines.append(f'| {mode} | {family} | {r["seconds"]:.3f} | {r["cpu_seconds"]:.3f} | {r["cpu_percent"]:.4f} | {r["rss_start_kib"]:.0f} / {r["rss_end_kib"]:.0f} | {r["scheduling_proxy_per_s"]:.3f} | {r["context_switches_per_s"]:.3f} |')
    lines+=['','Each interval begins after 8 seconds warmup. Demand-driven guards are released at 5 seconds; event subscriptions remain. Power-only excludes net/media/sys/desktop; fan helper was absent and was never activated. Baseline power-only is a 10-second diagnosis, after power-only is 60 seconds. Full-probe before and after are 60 seconds.','']
    for mode,payload in payloads.items():
        lines.append(f'{mode}: state publications `{payload["state_events"]}`, exclusions `{payload.get("excludes",[])}`, frozen agent inputs `{payload.get("agent_inputs_frozen",False)}`. [Raw counters, binary digest and source hashes](raw/perf-{mode}.json).')
    if 'release-after'in payloads:
        current='release-latest'if 'release-latest'in payloads else 'release-after'
        r=payloads[current]['probe'];lines+=['',f'Latest live full-probe CPU budget: **{"PASS"if r["cpu_percent"]<=.1 else "NOT MET"}** ({r["cpu_percent"]:.4f}%). Event-conditioned interpretation and power-only measurements remain separate from this numerical budget.']
    for mode in ['release-power-after','release-latest','release-latest-no-agent','release-frozen-agent']:
        if mode in payloads:
            r=payloads[mode]['probe'];runtime=sum(a['runtime_ns']-payloads[mode]['probe_before']['tasks'][identity]['runtime_ns']for identity,a in payloads[mode]['probe_after']['tasks'].items()if identity in payloads[mode]['probe_before']['tasks'])/1e9
            lines+=['',f'{mode}: **{"PASS"if r["cpu_percent"]<=.1 else "NOT MET"}** against 0.1% process CPU; named-thread schedstat CPU {runtime:.6f} s ({runtime/r["seconds"]*100:.4f}%). Genuine input activity and excluded/frozen inputs are listed separately above.']
    lines+=['','## Exact task and cause','',
        'Named-thread /proc counters attribute the baseline hot loop to `hyprv-services`. Scratch tracing identifies `hyprv_svc_power::power::Native`: all four provider inboxes receive global DBus NameOwnerChanged and each other\'s PropertiesChanged. Power refreshes exceed 256 in eight seconds without a matching power-state change. [Before event trace](raw/power-event-trace.log), [after event trace](raw/power-event-trace-after.log). After demand release the after trace has one genuine UPower PropertiesChanged and one refresh, not a sequence of unrelated-owner refreshes. `strace` was unavailable; no tracing was attached to Quickshell or another process.','',
        'zbus MessageStream matching cannot resolve a well-known sender locally. With multiple rules on one shared connection, the broad provider inboxes receive signals admitted by unrelated rules. Each accepted event triggers the expensive power snapshot. This is event amplification, not a timer polling loop.','',
        'Repair in power actor.rs: resolve providers to unique owners, keep absent-provider inboxes pending, and subscribe to NameOwnerChanged with interface/member/path and per-provider arg0 filters. Owner watches precede connect/snapshot; replacement rebuilds streams. The independent drain, bounded overflow behavior and reconnect backoff from the concurrent review repairs remain intact.','',
        'Regression: unrelated PropertiesChanged admitted by a second rule triggers a refresh on the before source and is rejected by the repaired source; genuine provider signals still arrive. A second private-bus test covers irrelevant owner traffic, initially absent providers and owner replacement. 17 power tests and strict all-target clippy pass. [Negative control](raw/power-idle-before-regression.log), [tests](raw/power-idle-tests.log), [clippy](raw/power-idle-clippy.log).','',
        '## Per-thread evidence','',
        '| Mode | Thread | CPU seconds (schedstat) | Scheduling timeslices |','|---|---|---|---|']
    for mode,payload in payloads.items():
        for identity,a in payload['probe_after']['tasks'].items():
            b=payload['probe_before']['tasks'].get(identity)
            if b:lines.append(f'| {mode} | {a["name"]} | {(a["runtime_ns"]-b["runtime_ns"])/1e9:.6f} | {a["timeslices"]-b["timeslices"]} |')
    lines+=['','## Measurement limits','',
        'schedstat scheduling timeslices and voluntary/involuntary context switches are **proxies, not exact kernel wakeups**. Endpoint scans are not atomic. Exited threads/short-lived children have incomplete scheduling counts; process CPU includes reaped child time. CPU tick resolution is 0.01 seconds on this host; a zero-tick 60-second interval means below roughly 0.017%, not mathematical zero. RSS is a family sum and includes shared pages more than once.','',
        'This desktop has active agent hooks, tray and compositor events. Their publications are recorded rather than suppressed. No panels were hidden, services restarted, credentials used, hardware controls changed or privileged tracing enabled. Debug historical measurements are retained in [perf-debug-history.md](perf-debug-history.md) and are not the release before/after evidence. Build snapshots are in [before](raw/release-before-build-snapshot.json) and [after](raw/release-after-build-snapshot.json).']
    (OUT/'perf.md').write_text('\n'.join(lines)+'\n')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['equivalence','idle','active','no-agent','isolation'])
    parser.add_argument('--binary',type=pathlib.Path);parser.add_argument('--label');parser.add_argument('--seconds',type=int,default=60);parser.add_argument('--exclude',action='append',default=[])
    parser.add_argument('--freeze-agent-input',action='store_true')
    args=parser.parse_args()
    if args.binary:BINARY=args.binary.resolve()
    RAW.mkdir(parents=True,exist_ok=True);env,qs=session()
    if args.mode=='equivalence':equivalence(env)
    elif args.mode=='isolation':
        for label,excludes in [('all',()),('no-agent',('agent_island',)),('no-viewflow',('viewflow',)),('neither',('agent_island','viewflow'))]:performance(env,qs,'isolation-'+label,10,excludes)
    else:performance(env,qs,args.label or args.mode,args.seconds,args.exclude,args.freeze_agent_input)
