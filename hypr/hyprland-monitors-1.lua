local lib = require("hypr.lib")

hl.monitor({ output = "DP-4", mode = "3840x2160@120", position = "0x0", scale = "2", bitdepth = 10 })
hl.monitor({ output = "HEADLESS-1", mode = "3184x2232@90", position = "1920x540", scale = "3" ,transform = 1})
lib.exec_once("xrdb ~/.Xresources")
