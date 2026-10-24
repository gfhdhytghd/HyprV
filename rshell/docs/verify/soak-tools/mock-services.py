#!/usr/bin/env python3
"""Synthetic MPRIS and logind, only on the activation-free private bus."""
import json, os, sys, time
from pathlib import Path
import dbus, dbus.service, dbus.mainloop.glib
from gi.repository import GLib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus=dbus.bus.BusConnection(os.environ['DBUS_SESSION_BUS_ADDRESS'])
names=[dbus.service.BusName(n,bus) for n in ('org.mpris.MediaPlayer2.Soak','org.freedesktop.login1')]
state=Path(sys.argv[1]); log=Path(sys.argv[2]).open('a',buffering=1)
PLAYER='org.mpris.MediaPlayer2.Player'; ROOT='org.mpris.MediaPlayer2'; PROPS='org.freedesktop.DBus.Properties'
def record(**kw): log.write(json.dumps(dict(epoch=time.time(),**kw))+'\n')
class Media(dbus.service.Object):
 def __init__(self): super().__init__(bus,'/org/mpris/MediaPlayer2');self.track=0;self.playing=False
 def properties(self,iface):
  if iface==ROOT:return {'Identity':'Soak mock player','DesktopEntry':'soak','CanRaise':False,'CanQuit':False,'HasTrackList':False,'SupportedUriSchemes':dbus.Array([],signature='s'),'SupportedMimeTypes':dbus.Array([],signature='s')}
  return {'PlaybackStatus':'Playing' if self.playing else 'Paused','LoopStatus':'None','Rate':dbus.Double(1.),'MinimumRate':dbus.Double(1.),'MaximumRate':dbus.Double(1.),'Shuffle':False,'Volume':dbus.Double(.5),'Position':dbus.Int64(self.track%180*1000000),'CanGoNext':False,'CanGoPrevious':False,'CanPlay':False,'CanPause':False,'CanSeek':False,'CanControl':False,'Metadata':dbus.Dictionary({'mpris:trackid':dbus.ObjectPath('/soak/track'+str(self.track)),'mpris:length':dbus.Int64(180000000),'xesam:title':'Soak track '+str(self.track%11),'xesam:artist':dbus.Array(['Synthetic artist'],signature='s')},signature='sv')}
 @dbus.service.method(PROPS,in_signature='s',out_signature='a{sv}')
 def GetAll(self,iface): record(read='GetAll',iface=iface);return self.properties(iface)
 @dbus.service.method(PROPS,in_signature='ss',out_signature='v')
 def Get(self,iface,key): record(read='Get',key=key);return self.properties(iface)[key]
 @dbus.service.method(PROPS,in_signature='ssv')
 def Set(self,iface,key,value): record(FORBIDDEN_MUTATION='Set',key=key);raise dbus.exceptions.DBusException('read only')
 @dbus.service.signal(PROPS,signature='sa{sv}as')
 def PropertiesChanged(self,iface,changed,invalid): pass
 @dbus.service.signal(PLAYER,signature='x')
 def Seeked(self,position): pass
class Login(dbus.service.Object):
 @dbus.service.signal('org.freedesktop.login1.Manager',signature='b')
 def PrepareForSleep(self,sleeping): record(signal='PrepareForSleep',value=bool(sleeping))
media=Media();login=Login(bus,'/org/freedesktop/login1');last={};owned=True
def tick():
 global last,owned
 try: data=json.loads(state.read_text())
 except (OSError,ValueError):return True
 if data!=last:
  present=bool(data.get('present',False))
  if present!=owned:
   if present:bus.request_name('org.mpris.MediaPlayer2.Soak')
   else:bus.release_name('org.mpris.MediaPlayer2.Soak')
   owned=present;record(owner_present=present)
  media.track=int(data.get('track',0));media.playing=bool(data.get('playing',False))
  if present:media.PropertiesChanged(PLAYER,media.properties(PLAYER),[])
  if data.get('sleep_sequence')!=last.get('sleep_sequence'):
   login.PrepareForSleep(True)
   GLib.timeout_add(300,lambda:(login.PrepareForSleep(False),False)[1])
  record(update=data);last=data
 return True
GLib.timeout_add(200,tick);record(ready=True);GLib.MainLoop().run()
