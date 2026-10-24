#!/usr/bin/env python3
"""Use measure-shell unchanged except perf's unprivileged user-callchain option."""
import importlib.util, inspect, signal, os, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('measure_shell',root/'docs/verify/measure-shell.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
s=inspect.getsource(m.measure).replace("'cpu-clock:u'", "'task-clock:u'").replace("'--call-graph','dwarf,8192'", "'--call-graph','dwarf,8192','--user-callchains'")
if os.environ.get('HYPRV_SERVICE_PROFILE_HZ'):
 hz=int(os.environ['HYPRV_SERVICE_PROFILE_HZ'])
 if not 1<=hz<=9999:raise ValueError('profiling frequency out of range')
 s=s.replace("'-F','997'", "'-F',"+repr(str(hz)))
exec(compile(s,str(root/'docs/verify/measure-shell.py'),'exec'),m.__dict__)
launch=m.Nested.launch
def traced_launch(self,cmd,env,name):
 if name.startswith('shell-') and os.environ.get('HYPRV_SERVICE_TRACE_ENABLE')=='1':
  env=env.copy();env['HYPRV_SERVICE_TRACE']=str(m.OUT/(name+'.trace'))
  if os.environ.get('HYPRV_SERVICE_TRACE_LEGACY')=='1':env['HYPRV_SERVICE_TRACE_LEGACY']='1'
 return launch(self,cmd,env,name)
m.Nested.launch=traced_launch
measure=m.measure
def monitored_measure(nested,qs,label,*args,**kwargs):
 log=(m.OUT/(label+'-dbus-headers.log')).open('w')
 monitor=subprocess.Popen(['dbus-monitor','--profile','--address',nested.env['DBUS_SESSION_BUS_ADDRESS'],"type='signal',interface='org.kde.StatusNotifierItem'"],stdout=log,stderr=subprocess.STDOUT)
 try:return measure(nested,qs,label,*args,**kwargs)
 finally:
  monitor.terminate();monitor.wait(timeout=5);log.close()
m.measure=monitored_measure
def stop(*_):raise SystemExit(143)
signal.signal(signal.SIGTERM,stop)
m.main()
