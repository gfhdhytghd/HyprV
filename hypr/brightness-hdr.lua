-- Persist the SDR portion of the two-stage brightness control across reloads.
local state = (os.getenv("XDG_STATE_HOME") or ((os.getenv("HOME") or "/home/wilf") .. "/.local/state")) .. "/hyprv/brightness-hdr.json"
local level = 70
local f = io.open(state, "r")
if f then
  level = tonumber(f:read("*a"):match('"level"%s*:%s*(%d+)')) or 70
  f:close()
end
package.loaded["hypr.brightness-sync"] = nil
local sync = require("hypr.brightness-sync")
if sync.lg_monitor() then return sync.plan(level).xps_ref end
local t = math.max(0, math.min(1, (level - 70) / 30))
local desktop = math.floor(153 + (400 - 153) * t + 0.5)
return desktop
