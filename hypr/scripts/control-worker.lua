-- Runs outside Hyprland: slow hardware/audio tools never block the compositor.
local home = assert(os.getenv("HOME"))
local cfg = dofile(home .. "/.config/HyprV/hypr/controls-profile.lua")
local function quote(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end
local function capture(cmd)
  local p = assert(io.popen(cmd .. " 2>/dev/null")); local s = p:read("*a"); local ok = p:close()
  assert(ok, "Command failed: " .. cmd); return s
end
local function execute(cmd) assert(os.execute(cmd), "Command failed: " .. cmd) end
local function read(path) local f = assert(io.open(path)); local s = f:read("*a"); f:close(); return s end
local action, result, value, knownMax = arg[1], arg[2], tonumber(arg[3]), tonumber(arg[4])
local function hardware()
  if cfg.backend == "backlight" then
    local base = "/sys/class/backlight/intel_backlight/"
    if value then execute("brightnessctl -d intel_backlight --min-value=1 set " .. value) end
    return assert(tonumber(read(base .. "brightness"))), assert(tonumber(read(base .. "max_brightness")))
  end
  local bus = cfg.ddc_bus
  -- The controller already knows the range. During interaction, send only
  -- the latest target; it schedules a real read-back once input settles.
  if value and bus and knownMax then
    assert(value >= 0 and value <= knownMax and value == math.floor(value), "Invalid DDC value")
    execute("ddcutil --bus " .. bus .. " --skip-ddc-checks --noverify setvcp 10 " .. value)
    return value, knownMax, "sent"
  end
  local function get(b)
    local ok, s = pcall(capture, "ddcutil --bus " .. b .. " --skip-ddc-checks getvcp 10 --terse")
    if ok then local cur, max = s:match("VCP%s+10%s+C%s+(%d+)%s+(%d+)"); if cur then return tonumber(cur), tonumber(max) end end
  end
  local cur, max
  if bus then cur,max=get(bus) end
  if not cur then
    for b in capture("ddcutil detect --brief"):gmatch("/dev/i2c%-(%d+)") do
      cur, max = get(b); if cur then bus=tonumber(b); break end
    end
  end
  assert(cur, "No DDC display available")
  if value then execute("ddcutil --bus " .. bus .. " setvcp 10 " .. value); cur,max=get(bus) end
  return assert(cur), assert(max)
end
if action == "audio-status" then
  local ok, s=pcall(capture, "wpctl get-volume @DEFAULT_AUDIO_SINK@")
  local volume=ok and tonumber(s:match("Volume:%s*([%d.]+)"))
  if not volume then print("available=false\nmuted=false\nvolume=0"); os.exit(1) end
  print("available=true\nmuted=" .. tostring(s:find("[MUTED]",1,true)~=nil) .. "\nvolume=" .. math.floor(volume*100+.5))
  os.exit(0)
end
assert(action == "hardware")
local ok, raw, max, status = pcall(hardware)
local f=assert(io.open(result .. ".tmp","w"))
f:write(ok and ((status or "ok") .. " " .. raw .. " " .. max) or ("error " .. tostring(raw))); f:close()
assert(os.rename(result .. ".tmp",result))
