from pathlib import Path
import subprocess,os,json,shlex
b=Path(__file__).resolve().parent
flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Quick','Qt6Qml'],text=True));qt='/usr/include/qt6'
flags += ['-I'+qt+'/QtGui/6.12.0','-I'+qt+'/QtGui/6.12.0/QtGui','-I'+qt+'/QtCore/6.12.0','-I'+qt+'/QtCore/6.12.0/QtCore']
for p in Path(qt).glob('*/6.12.0'):
 flags += ['-I'+str(p)] + ['-I'+str(d) for d in p.iterdir() if d.is_dir()]
subprocess.run(['c++','-std=c++17','-fPIC',str(b/'host.cpp'),'-o',str(b/'host'),*flags],check=True)
cases={
'wheel':[{'t':320,'type':'wheel','ay':-120}],
'wheel-burst':[{'t':t,'type':'wheel','ay':a} for t,a in [(320,-120),(368,-120),(400,-120),(448,120),(496,120)]],
'wheel-bound':[{'t':320,'type':'set','key':'contentY','value':5},{'t':336,'type':'wheel','ay':120}],
'drag':[{'t':320,'type':'press','y':300}]+[{'t':320+16*i,'type':'move','y':300-10*i,'v':-625} for i in range(1,13)]+[{'t':528,'type':'release','y':180,'v':-625}],
'drag-pause':[{'t':320,'type':'press','y':300}]+[{'t':320+16*i,'type':'move','y':300-10*i,'v':-625} for i in range(1,13)]+[{'t':720,'type':'release','y':180,'v':0}],
'drag-top':[{'t':320,'type':'press','y':100}]+[{'t':320+16*i,'type':'move','y':100+10*i,'v':625} for i in range(1,13)]+[{'t':528,'type':'release','y':220,'v':625}],
'pixel':[{'t':320,'type':'wheel','pixel':True,'phase':1,'py':0,'ay':0}]+[{'t':320+16*i,'type':'wheel','pixel':True,'phase':2,'py':-10,'ay':-15} for i in range(1,10)]+[{'t':480,'type':'wheel','pixel':True,'phase':3,'py':0,'ay':0}],
}
cases['drag-native']=[{k:v for k,v in e.items() if k!='v'} for e in cases['drag']]
cases['drag-fast']=[dict(e,**({'v':-10000} if e['type']=='move' else {})) for e in cases['drag']]
cases['drag-bottom']=[{'t':304,'type':'set','key':'contentY','value':600}]+cases['drag']
cases['drag-top-pause']=cases['drag-top'][:-1]+[{'t':720,'type':'release','y':220,'v':0}]
cases['drag-threshold']=[{'t':320,'type':'press','y':200}]+[{'t':336+16*i,'type':'move','y':200-y,'v':0} for i,y in enumerate([5,10,11,12,20])]+[{'t':432,'type':'release','y':180,'v':0}]
cases['movement-stop']=[{'t':304,'type':'set','key':'boundsMovement','value':0}]+cases['drag-top']
cases['drag-only']=[{'t':304,'type':'set','key':'boundsBehavior','value':1}]+cases['drag-top']
cases['overshoot-only']=[{'t':304,'type':'set','key':'boundsBehavior','value':2}]+cases['drag-bottom']
cases['wheel-accel']=cases['wheel']
cases['wheel-accel-burst']=cases['wheel-burst']
cases['pixel-momentum']=cases['pixel'][:-1]+[{'t':480+16*i,'type':'wheel','pixel':True,'phase':4,'py':-10+i,'ay':-15} for i in range(6)]+[{'t':592,'type':'wheel','pixel':True,'phase':3,'py':0,'ay':0}]
cases['shrink']=[{'t':320,'type':'set','key':'contentY','value':600},{'t':336,'type':'set','key':'contentHeight','value':200}]
cases['tick8-wheel']=cases['wheel']
cases['tick32-wheel']=cases['wheel']
cases['tick8-drag-top']=cases['drag-top']
cases['tick32-drag']=[dict(e,t=320+(e['t']-320)*2) for e in cases['drag']]
cases['pixel-top-momentum']=[dict(e,py=-e.get('py',0),ay=-e.get('ay',0)) for e in cases['pixel-momentum']]
cases['drag-anchor-adjust']=cases['drag']+[{'t':416,'type':'set','key':'contentY','value':80}]
(b/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
captured=0
for mode in ['menu','list']:
 for name,events in cases.items():
  if mode=='list' and name in ['drag-only','overshoot-only']:continue
  if mode=='list' and name=='shrink':events=[dict(e,key='model',value=2) if e.get('key')=='contentHeight' else e for e in events]
  p=b/'sequence.json';p.write_text(json.dumps(events));env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software')
  env.pop('QT_QUICK_FLICKABLE_WHEEL_DECELERATION',None)
  if 'accel' in name:env['QT_QUICK_FLICKABLE_WHEEL_DECELERATION']='5000'
  run=subprocess.run([str(b/'host'),str(b/'scene.qml'),mode,str(p),str(8 if name.startswith('tick8-') else 32 if name.startswith('tick32-') else 16)],env=env,text=True,capture_output=True,check=True)
  captured+=1
  (b/f'{mode}-{name}.csv').write_text(run.stdout);(b/f'{mode}-{name}.meta').write_text(run.stderr)
print('Captured',captured,'Qt traces')
