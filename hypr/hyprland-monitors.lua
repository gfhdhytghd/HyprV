package.loaded["hypr.brightness-hdr"] = nil
-- XPS 13 9350: internal 2880x1800 panel; retain automatic external outputs.
-- OLED: keep SDR black at zero; use the panel EDID for HDR peak luminance.
hl.monitor({
  output = "eDP-1", mode = "preferred", position = "0x0", scale = "2", transform = 0,
  disabled = false,
  bitdepth = 10,
  cm = "hdr",
  -- This DisplayID OLED advertises PQ/BT.2020, but auto detection falls back to sRGB.
  supports_hdr = 1,
  supports_wide_color = 1,
  min_luminance = 0.0005,
  max_luminance = 530,
  max_avg_luminance = 400,
  sdr_min_luminance = 0,
  reference_luminance = require("hypr.brightness-hdr"),
})
-- The desktop LG UltraFine 6K (same panel as SuperPower DP-4): desktop parameters, centred
-- directly above the internal panel (eDP-1 is 1440x900 logical at 0x0; LG is 3072x1728).
hl.monitor({
  output = "desc:LG Electronics LG ULTRAFINE 601INXJ74394",
  mode = "6144x3456@60.02",
  position = "-816x-1728",
  scale = "2",
  transform = 0,
  bitdepth = 10,
  cm = "hdr",
  sdr_min_luminance = 0.2,
  -- Same level as the internal panel (hypr/brightness-sync.lua); controls.lua keeps it in sync.
  reference_luminance = (function()
    local f = io.open((os.getenv("XDG_STATE_HOME") or ((os.getenv("HOME") or "/home/wilf") .. "/.local/state")) .. "/hyprv/brightness-hdr.json")
    local level = f and tonumber(f:read("*a"):match('"level"%s*:%s*(%d+)')) or 70
    if f then f:close() end
    return require("hypr.brightness-sync").plan(level).lg_ref
  end)(),
})
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = "auto" })
