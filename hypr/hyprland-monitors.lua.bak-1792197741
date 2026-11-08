package.loaded["hypr.brightness-hdr"] = nil
-- XPS 13 9350: internal 2880x1800 panel; retain automatic external outputs.
-- OLED: keep SDR black at zero; use the panel EDID for HDR peak luminance.
hl.monitor({
  output = "eDP-1", mode = "preferred", position = "0x0", scale = "2", transform = 0,
  bitdepth = 10,
  cm = "hdr",
  -- This DisplayID OLED advertises PQ/BT.2020, but auto detection falls back to sRGB.
  supports_hdr = 1,
  supports_wide_color = 1,
  min_luminance = 0.0005,
  max_luminance = 530,
  max_avg_luminance = 400,
  sdr_min_luminance = 0,
  sdr_max_luminance = require("hypr.brightness-hdr"),
})
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = "auto" })
