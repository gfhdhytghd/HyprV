-- Native Hyprland control logic. No synchronous subprocesses in the compositor.
local M = {}
local home = assert(os.getenv("HOME"))
local root = home .. "/.config/HyprV"
local cfg = require("hypr.controls-profile")
local state = (os.getenv("XDG_STATE_HOME") or home .. "/.local/state") .. "/hyprv/brightness-hdr.json"
local runtime = (os.getenv("XDG_RUNTIME_DIR") or "/tmp") .. "/hyprv-controls"
local function read(path)
  local f=io.open(path); if not f then return nil end
  local s=f:read("*a"); f:close(); return s
end
local function round(x) return math.floor(x+.5) end
local function clamp(x,a,b) return math.max(a,math.min(b,x)) end
local function quote(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end
local saved = tonumber((read(state) or ""):match('"level"%s*:%s*(%d+)')) or 70
local raw, maximum = cfg.initial_raw, cfg.initial_max
local wanted, busy, sequence, lastRead = nil, nil, 0, os.time()
local known = cfg.backend=="backlight"
M.stats={hardware_jobs=0,sdr_updates=0}
local appliedDesktop, appliedClient
local function targets(level)
  local t=clamp((level-70)/30,0,1)
  local white=round(cfg.desktop+(cfg.peak-cfg.desktop)*t)
  return white,white
end
appliedDesktop,appliedClient=targets(saved)
-- Hardware percent: linear 0..10, then logarithmic 10..100 over UI 10..70.
local function hardwarePercent(level)
  level=clamp(level,0,70)
  if level<=10 then return level end
  return 10*10^((level-10)/60)
end
function M.plan(level)
  local d,c=targets(level)
  local minimum=cfg.backend=="backlight" and 1 or 0
  return clamp(round(maximum*hardwarePercent(level)/100),minimum,maximum),d,c
end
local function rawLevel(r)
  local percent=100*r/maximum
  if percent<=10 then return round(percent) end
  return round(10+60*math.log(percent/10)/math.log(10))
end
local function notice(text)
  hl.notification.create({text=text,timeout=4000,icon="warning"})
end
local function notify(kind,level)
  hl.exec_cmd("quickshell -p " .. quote(root .. "/quickshell") .. " ipc call quickAdjust " .. kind .. (level and (" " .. level) or ""))
end
local function applySDR(level)
  local d,c=targets(level)
  if d==appliedDesktop and c==appliedClient then return end
  M.stats.sdr_updates=M.stats.sdr_updates+1
  local plugin=hl.plugin and hl.plugin.hypr_sdr_white
  assert(plugin and plugin.refresh,"hypr-sdr-white is not loaded")
  hl.config({plugin={hypr_sdr_white={reference_luminance=c}}})
  if d~=appliedDesktop then
    local mon=assert(hl.get_monitor(cfg.output),"Display unavailable")
    local p=mon.position
    local fields={output=cfg.output,mode=cfg.mode,position=p.x .. "x" .. p.y,scale=mon.scale,transform=mon.transform,bitdepth=10,cm="hdr",sdr_min_luminance=cfg.sdr_min,sdr_max_luminance=d}
    for k,v in pairs(cfg.monitor or {}) do fields[k]=v end
    hl.monitor(fields)
  end
  plugin.refresh()
  appliedDesktop,appliedClient=d,c
end
local function persist(level)
  local f=assert(io.open(state .. ".tmp","w")); f:write('{"level":' .. level .. '}'); f:close(); assert(os.rename(state .. ".tmp",state))
end
local function startJob(level)
  sequence=sequence+1
  M.stats.hardware_jobs=M.stats.hardware_jobs+1
  local path=runtime .. "-" .. cfg.output .. "-" .. tostring({}):gsub("[^%w]","") .. "-" .. sequence
  local target=level and M.plan(level) or nil
  busy={path=path,level=level,started=os.time()}
  hl.exec_cmd("lua " .. quote(root .. "/hypr/scripts/control-worker.lua") .. " hardware " .. quote(path) .. (target and (" " .. target) or ""))
end
function M.brightness_get()
  if wanted then return wanted end
  if busy and busy.level then return busy.level end
  if cfg.backend=="backlight" then
    raw=tonumber(read("/sys/class/backlight/intel_backlight/brightness")) or raw
    maximum=tonumber(read("/sys/class/backlight/intel_backlight/max_brightness")) or maximum
  elseif not busy and os.time()-lastRead>=2 then startJob(nil) end
  if not known then return nil end
  if raw<maximum then return rawLevel(raw) end
  return round(70+30*clamp((appliedDesktop-cfg.desktop)/(cfg.peak-cfg.desktop),0,1))
end
function M.brightness_raw() return known and raw or nil end
function M.brightness_set(level,show)
  assert(type(level)=="number" and level==level,"Invalid brightness")
  level=clamp(round(level),0,100)
  -- Read actual hardware state, but never force adjacent raw levels for a
  -- slider request: identical low targets must remain identical, without toggling.
  M.brightness_get()
  wanted=level
  if show then notify("showBrightnessLevel",level) end
end
function M.brightness_step(delta)
  local level=M.brightness_get()
  if level then M.brightness_set(level+delta,true) else notice("Reading display brightness; try again shortly") end
end
function M.restore()
  appliedClient=nil
  if cfg.auto_brightness then
    -- ALS owns startup hardware brightness; do not overwrite it with last session's manual setting.
    applySDR(70)
    persist(70)
    saved=70
  else M.brightness_set(saved,false) end
end
function M.volume(action,value)
  local cmd
  if action=="set" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ " .. string.format("%.2f",clamp(assert(tonumber(value)),0,100)/100)
  elseif action=="inc" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+"
  elseif action=="dec" then cmd="wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"
  elseif action=="toggle" then cmd="wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"
  elseif action=="toggle-mic" then cmd="wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"
  elseif action=="mic-inc" then cmd="wpctl set-volume -l 1 @DEFAULT_AUDIO_SOURCE@ 5%+"
  elseif action=="mic-dec" then cmd="wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 5%-"
  else error("Unknown audio action") end
  hl.exec_cmd(cmd)
  if action=="toggle-mic" then
    hl.exec_cmd("notify-send -h string:x-canonical-private-synchronous:sys-notify 'Microphone mute toggled'")
  elseif not action:find("mic",1,true) and action~="set" then
    hl.timer(function() notify("showVolume") end,{timeout=80,type="oneshot"})
  end
end
function M.keyboard_step(delta)
  hl.exec_cmd("brightnessctl -d '*::kbd_backlight' set 33%" .. (delta>0 and "+" or "-"))
end
M.timer=hl.timer(function()
  if busy then
    local s=read(busy.path)
    if s then
      os.remove(busy.path)
      local r,m=s:match("^ok (%d+) (%d+)")
      local level=busy.level; busy=nil;lastRead=os.time()
      if r then
        raw,maximum=tonumber(r),tonumber(m);known=true
        -- Skip obsolete SDR updates during a drag; only apply the newest target.
        if level and (not wanted or wanted==level) then
          local ok,err=pcall(function() applySDR(level);persist(level);saved=level end)
          if not ok then notice(tostring(err)) end
        end
      else wanted=nil;notice(s) end
    elseif os.time()-busy.started>12 then
      wanted=nil;busy=nil;notice("Brightness hardware operation timed out")
    end
  end
  if not busy and wanted then
    local level=wanted;wanted=nil
    local target=M.plan(level)
    if target==raw then
      local ok,err=pcall(function() applySDR(level);persist(level);saved=level end)
      if not ok then notice(tostring(err)) end
    else startJob(level) end
  end
end,{timeout=25,type="repeat"})
if cfg.backend=="ddc" then startJob(nil) end
return M
