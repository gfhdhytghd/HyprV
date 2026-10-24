from pathlib import Path
import subprocess,os,json,shlex
b=Path(__file__).resolve().parent
flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Quick','Qt6Qml'],text=True))
for p in Path('/usr/include/qt6').glob('*/6.12.0'):
 flags += ['-I'+str(p)] + ['-I'+str(d) for d in p.iterdir() if d.is_dir()]
subprocess.run(['c++','-std=c++17','-fPIC',str(b/'host.cpp'),'-o',str(b/'host'),*flags],check=True)
drag=[{'t':320,'type':'press','y':300}]+[{'t':320+16*i,'type':'move','y':300-10*i} for i in range(1,13)]+[{'t':528,'type':'release','y':180}]
cases={
 'wheel':[{'t':320,'type':'wheel','ay':-120}],
 'burst':[{'t':t,'type':'wheel','ay':a} for t,a in [(320,-120),(352,-120),(400,120),(432,-120)]],
 'pixels':[{'t':320+16*i,'type':'wheel','pixel':True,'py':-10,'ay':-15,'phase':2} for i in range(8)],
 'mixed-deltas':[{'t':320,'type':'wheel','pixel':True,'py':-17,'ay':-120,'phase':2}],
 'drag':drag,
 'pause':drag[:-1]+[{'t':720,'type':'release','y':180}],
 'threshold':[{'t':320,'type':'press','y':250}]+[{'t':336+16*i,'type':'move','y':250-y} for i,y in enumerate([5,10,11,12,20])]+[{'t':432,'type':'release','y':230}],
 'top':[dict(e,y=400-e['y']) for e in drag],
 'click':[{'t':320,'type':'press','y':200},{'t':352,'type':'release','y':200}],
 'held-click':[{'t':320,'type':'press','y':200},{'t':464,'type':'release','y':200}],
 'wheel-during-flick':drag+[{'t':576,'type':'wheel','ay':120}],
}
cases['pixel-session']=[{'t':304,'type':'wheel','pixel':True,'phase':1}]+cases['pixels']+[{'t':480,'type':'wheel','pixel':True,'phase':3}]
cases['pixel-momentum']=[{'t':304,'type':'wheel','pixel':True,'phase':1}]+cases['pixels']+[{'t':464+16*i,'type':'wheel','pixel':True,'phase':4,'py':-8+i,'ay':-15} for i in range(6)]+[{'t':560,'type':'wheel','pixel':True,'phase':3}]
cases['mixed-session']=[{'t':304,'type':'wheel','pixel':True,'phase':1}]+cases['mixed-deltas']+[{'t':352,'type':'wheel','pixel':True,'phase':3}]
manifest=[]
for mode in ['wifi','bluetooth','calendar','calendar-rebase']:
 selected=dict(cases)
 if mode=='wifi':
  selected['expand']=[{'t':304,'type':'set','key':'extra','value':92}]+drag
  selected['expand-during-drag']=drag+[{'t':416,'type':'set','key':'extra','value':92}]
  selected['shrink']=[{'t':304,'type':'set','key':'contentY','value':300},{'t':320,'type':'set','key':'rows','value':2}]
 if mode in ['wifi','bluetooth']:
  selected['bottom']=[{'t':304,'type':'set','key':'contentY','value':240 if mode=='bluetooth' else 330}]+drag
  selected['shrink-during-flick']=drag+[{'t':576,'type':'set','key':'rows','value':2}]
 if mode=='bluetooth':
  selected['shrink']=[{'t':304,'type':'set','key':'contentY','value':200},{'t':320,'type':'set','key':'rows','value':2}]
 if mode.startswith('calendar'):
  selected['viewport-during-flick']=[{'t':304,'type':'set','key':'height','value':310},{'t':304,'type':'set','key':'contentY','value':580}]+drag+[{'t':576,'type':'set','key':'height','value':360}]
  selected['diagonal']=[dict(e,x=300+(e['y']-300)*0.35) for e in drag]
  selected['horizontal']=[dict(e,x=e['y'],y=200) for e in drag]
  selected['rebase']=[dict(e,x=e['y']*2-300,y=200) for e in drag]
  selected['wheel-handler-timeout']=[{'t':320,'type':'wheel','ay':-120},{'t':352,'type':'wheel','ax':-120},{'t':560,'type':'wheel','ax':-120}]
  selected['wheel-x']=[{'t':320+16*i,'type':'wheel','ax':-120} for i in range(6)]
  selected['wheel-during-x-flick']=[dict(e,x=e['y'],y=200) for e in drag]+[{'t':576,'type':'wheel','ay':120}]
 for name,events in selected.items():
  path=b/f'{mode}-{name}.json';path.write_text(json.dumps(events,indent=2)+'\n')
  env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software');env.pop('QT_QUICK_FLICKABLE_WHEEL_DECELERATION',None)
  r=subprocess.run([str(b/'host'),str(b/'scene.qml'),mode,str(path),'16'],env=env,text=True,capture_output=True,check=True)
  (b/f'{mode}-{name}.csv').write_text(r.stdout);(b/f'{mode}-{name}.meta').write_text(r.stderr)
  manifest.append([mode,name])
(b/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Captured',len(manifest),'panel traces')
