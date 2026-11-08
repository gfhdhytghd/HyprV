-- Persist the SDR portion of the two-stage brightness control across reloads.
local state = (os.getenv("XDG_STATE_HOME") or ((os.getenv("HOME") or "/home/wilf") .. "/.local/state")) .. "/hyprv/brightness-hdr.json"
local level = 70
local f = io.open(state, "r")
if f then
  level = tonumber(f:read("*a"):match('"level"%s*:%s*(%d+)')) or 70
  f:close()
end
local t = math.max(0, math.min(1, (level - 70) / 30))
local desktop = math.floor(153 + (400 - 153) * t + 0.5)
local client = desktop
require("hypr.lib").plugin_config_if_available("plugin.hypr_sdr_white.reference_luminance", {
  plugin = { hypr_sdr_white = { reference_luminance = client } },
})
if hl.plugin and hl.plugin.hypr_sdr_white and hl.plugin.hypr_sdr_white.refresh then
  hl.plugin.hypr_sdr_white.refresh()
end
return desktop
