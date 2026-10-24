-- Stub of the laptop `hypr.controls` module for nested e2e (laptop-qa-plan.md §2.3).
-- Loaded instead of ~/.config/HyprV/hypr/controls.lua by prepending this fakes dir to
-- package.path in the nested Hyprland config:
--   package.path = "<parity>/laptop/fakes/?.lua;" .. package.path
-- State: $HYPRV_FAKE_CONTROLS_DIR/level (current UI level, default 50).
-- Log:   $HYPRV_FAKE_CONTROLS_DIR/controls.log, one line per call: "<fn> <args>".
-- The HDR/backlight mapping is NOT stubbed here: M.plan is the real closed form from
-- the laptop controls.lua (24972f7) so `lap.brightness.plan` can compare against it.
local M = {}
local dir = os.getenv("HYPRV_FAKE_CONTROLS_DIR") or "/tmp"
local cfg = {desktop = 153, peak = 400, max = 400, backend = "backlight"}

local function log(line)
  local f = io.open(dir .. "/controls.log", "a")
  if f then f:write(line, "\n"); f:close() end
end
local function round(x) return math.floor(x + .5) end
local function clamp(x, a, b) return math.max(a, math.min(b, x)) end
local function read_level()
  local f = io.open(dir .. "/level")
  if not f then return 50 end
  local v = tonumber(f:read("*a")); f:close()
  return v or 50
end
local function write_level(v)
  local f = io.open(dir .. "/level", "w")
  if f then f:write(tostring(v)); f:close() end
end

function M.brightness_get() log("brightness_get"); return read_level() end
function M.brightness_set(level, show, interactive)
  log(string.format("brightness_set %s %s %s", tostring(level), tostring(show), tostring(interactive)))
  write_level(level)
end
function M.brightness_finish(level) log("brightness_finish " .. tostring(level)); write_level(level) end
function M.brightness_step(delta) log("brightness_step " .. tostring(delta)) end
function M.keyboard_step(delta) log("keyboard_step " .. tostring(delta)) end
function M.volume(action, value) log("volume " .. tostring(action) .. " " .. tostring(value)) end
function M.restore() log("restore") end

-- Real mapping (laptop controls.lua:21-37 with controls-profile.lua values).
local function targets(level)
  local t = clamp((level - 70) / 30, 0, 1)
  local white = round(cfg.desktop + (cfg.peak - cfg.desktop) * t)
  return white, white
end
local function hardwarePercent(level)
  level = clamp(level, 0, 70)
  if level <= 10 then return level end
  return 10 * 10 ^ ((level - 10) / 60)
end
function M.plan(level)
  local d, c = targets(level)
  local minimum = cfg.backend == "backlight" and 1 or 0
  return clamp(round(cfg.max * hardwarePercent(level) / 100), minimum, cfg.max), d, c
end

return M
