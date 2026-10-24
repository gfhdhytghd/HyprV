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
local commandedRaw = known and raw or nil
local goal, dragging, verifyTicks, confirmRetries, confirmFailed = nil, false, nil, 0, false
local requestSequence = 0
local maxConfirmRetries = 3
M.stats={hardware_jobs=0,sdr_updates=0,verification_reads=0,verification_retries=0}
local appliedDesktop, forceSDR
local function targets(level)
  if level<10 then return round(10+(cfg.desktop-10)*clamp(level/10,0,1)) end
  local t=clamp((level-70)/30,0,1)
  return round(cfg.desktop+(cfg.peak-cfg.desktop)*t)
end
appliedDesktop=targets(saved)
-- UI 0..10 holds minimum hardware brightness and adjusts SDR white.
-- UI 10..70 traverses the previous UI 0..70 hardware curve.
local function hardwarePercent(level)
  level=clamp((level-10)*70/60,0,70)
  if level<=10 then return level end
  return 10*10^((level-10)/60)
end
function M.plan(level)
  local minimum=cfg.backend=="backlight" and 1 or 0
  return clamp(round(maximum*hardwarePercent(level)/100),minimum,maximum)
end
local function rawLevel(r)
  local percent=100*r/maximum
  local oldLevel=percent<=10 and percent or (10+60*math.log(percent/10)/math.log(10))
  return round(10+oldLevel*60/70)
end
local function notice(text)
  hl.notification.create({text=text,timeout=4000,icon="warning"})
end
local function notify(kind,level)
  hl.exec_cmd("quickshell -p " .. quote(root .. "/quickshell") .. " ipc call quickAdjust " .. kind .. (level and (" " .. level) or ""))
end
local function applySDR(level)
  -- SDR white and color-managed clients follow reference_luminance.
  local d=targets(level)
  if d==appliedDesktop and not forceSDR then return end
  M.stats.sdr_updates=M.stats.sdr_updates+1
  local mon=assert(hl.get_monitor(cfg.output),"Display unavailable")
  local p=mon.position
  local fields={output=cfg.output,mode=cfg.mode,position=p.x .. "x" .. p.y,scale=mon.scale,transform=mon.transform,bitdepth=10,cm="hdr",sdr_min_luminance=cfg.sdr_min,reference_luminance=d}
  for k,v in pairs(cfg.monitor or {}) do fields[k]=v end
  hl.monitor(fields)
  appliedDesktop,forceSDR=d,false
end
local function persist(level)
  local f=assert(io.open(state .. ".tmp","w")); f:write('{"level":' .. level .. '}'); f:close(); assert(os.rename(state .. ".tmp",state))
end
local function startJob(level,verify)
  sequence=sequence+1
  M.stats.hardware_jobs=M.stats.hardware_jobs+1
  local path=runtime .. "-" .. cfg.output .. "-" .. tostring({}):gsub("[^%w]","") .. "-" .. sequence
  local target=level and M.plan(level) or nil
  busy={path=path,level=level,verify=verify,requestSequence=requestSequence,started=os.time()}
  if verify then M.stats.verification_reads=M.stats.verification_reads+1 end
  -- Bound the entire worker so timeout recovery cannot overlap a stuck job.
  hl.exec_cmd("timeout --kill-after=1s 10s lua " .. quote(root .. "/hypr/scripts/control-worker.lua") .. " hardware " .. quote(path) .. (target and (" " .. target .. " " .. maximum) or ""))
end
local function applyLevel(level,save)
  local ok,err=pcall(function()
    applySDR(level)
    if save then persist(level);saved=level end
  end)
  if not ok then M.stats.last_error=tostring(err);notice(tostring(err)) end
end
local function pump()
  if busy then return end
  if wanted then
    -- Initial discovery is needed once to learn the actual brightness range.
    if not known and not commandedRaw then startJob(nil);return end
    local level=wanted;wanted=nil
    if M.plan(level)==commandedRaw then applyLevel(level,false)
    else startJob(level);return end
  end
  if not dragging and verifyTicks==0 then verifyTicks=nil;startJob(nil,true) end
end
local function retryConfirmation(reason)
  M.stats.last_error=reason
  if confirmRetries<maxConfirmRetries then
    confirmRetries=confirmRetries+1
    M.stats.verification_retries=M.stats.verification_retries+1
    commandedRaw=nil -- A failed write/read must never satisfy the target cache.
    wanted=goal
    verifyTicks=0
  else
    confirmFailed=true;verifyTicks=nil;wanted=nil
    notice(reason .. "; brightness target retained after 3 retries")
  end
end
function M.brightness_get()
  if cfg.backend=="backlight" then
    raw=tonumber(read("/sys/class/backlight/intel_backlight/brightness")) or raw
    maximum=tonumber(read("/sys/class/backlight/intel_backlight/max_brightness")) or maximum
  elseif not busy and not wanted and not dragging and not verifyTicks and not confirmFailed and os.time()-lastRead>=2 then startJob(nil) end
  -- Keep user intent stable during writing, confirmation, and failed retries.
  -- Monitor read-back is tracked separately rather than fed into the slider.
  if goal then return goal end
  if not known then return nil end
  if raw==M.plan(0) and appliedDesktop<cfg.desktop then
    return round(10*clamp((appliedDesktop-10)/(cfg.desktop-10),0,1))
  end
  if raw<maximum then return rawLevel(raw) end
  return round(70+30*clamp((appliedDesktop-cfg.desktop)/(cfg.peak-cfg.desktop),0,1))
end
function M.brightness_raw() return known and raw or nil end
function M.brightness_status()
  return {target=goal,confirmed_raw=known and raw or nil,commanded_raw=commandedRaw,
    dragging=dragging,busy=busy~=nil,confirming=verifyTicks~=nil or (busy and busy.verify) or false,
    failed=confirmFailed,retries=confirmRetries,last_error=M.stats.last_error}
end
function M.brightness_set(level,show,deferVerify)
  assert(type(level)=="number" and level==level,"Invalid brightness")
  level=clamp(round(level),0,100)
  requestSequence=requestSequence+1
  wanted=level;goal=level;dragging=deferVerify==true
  -- Sliders explicitly finish on release. Key repeats and CLI calls settle
  -- after 300 ms without a new request.
  verifyTicks=not dragging and 12 or nil
  confirmRetries=0;confirmFailed=false;M.stats.last_error=nil
  if show then notify("showBrightnessLevel",level) end
  pump()
end
function M.brightness_finish(level)
  M.brightness_set(level,false,true)
  dragging=false;verifyTicks=0
  pump()
end
function M.brightness_step(delta)
  local level=M.brightness_get()
  if level then M.brightness_set(level+delta,true) else notice("Reading display brightness; try again shortly") end
end
function M.restore() forceSDR=true; M.brightness_set(saved,false) end
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
      local status,r,m=s:match("^(%a+) (%d+) (%d+)")
      local job=busy;local level=job.level;busy=nil;lastRead=os.time()
      if r and (status=="ok" or status=="sent") then
        local observed,limit=tonumber(r),tonumber(m)
        local previousRaw=raw
        maximum=limit;commandedRaw=observed
        if status=="ok" then
          raw=observed;known=true
          if job.verify and job.requestSequence==requestSequence and goal and not dragging then
            local target=M.plan(goal)
            if raw==target then
              verifyTicks=nil;confirmRetries=0;confirmFailed=false;M.stats.last_error=nil
              applyLevel(goal,true)
            else
              retryConfirmation("Brightness confirmation: requested " .. target .. ", monitor reports " .. raw)
            end
          elseif not job.verify and goal and not wanted and not dragging and not verifyTicks and not confirmFailed and raw~=previousRaw then
            -- An idle external monitor adjustment may update the UI normally.
            goal=nil
          end
        end
        -- Skip obsolete SDR updates during a drag; only apply the newest target.
        if level and (not wanted or wanted==level) then
          applyLevel(level,false)
        end
      else
        commandedRaw=nil;M.stats.last_error=s
        if goal and not dragging and job.requestSequence==requestSequence then retryConfirmation(s)
        elseif not wanted then notice(s) end
      end
    elseif os.time()-busy.started>12 then
      local job=busy;busy=nil;commandedRaw=nil
      local reason="Brightness hardware operation timed out"
      if goal and not dragging and job.requestSequence==requestSequence then retryConfirmation(reason)
      else notice(reason) end
    end
  end
  if not busy and not wanted and verifyTicks and verifyTicks>0 then
    verifyTicks=verifyTicks-1
  end
  pump()
end,{timeout=25,type="repeat"})
if cfg.backend=="ddc" then startJob(nil) end
return M
