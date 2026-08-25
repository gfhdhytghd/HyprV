local lib = require("hypr.lib")

hl.monitor({
  output = "DP-4",
  mode = "6144x3456@60.02",
  position = "0x0",
  scale = "2",
  transform = 0,
  bitdepth = 10,
  cm = "srgb",
})

lib.exec_once("xrdb ~/.Xresources")
