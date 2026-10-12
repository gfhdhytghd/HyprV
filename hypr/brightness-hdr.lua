-- Persist the SDR portion of the two-stage brightness control across reloads.
local state = (os.getenv("XDG_STATE_HOME") or ((os.getenv("HOME") or "/home/wilf") .. "/.local/state")) .. "/hyprv/brightness-hdr.json"
local level = 70
local f = io.open(state, "r")
if f then
  level = tonumber(f:read("*a"):match('"level"%s*:%s*(%d+)')) or 70
  f:close()
end
local t = math.max(0, math.min(1, (level - 70) / 30))
local desktop = math.floor(173 + (446 - 173) * t + 0.5)
return desktop
