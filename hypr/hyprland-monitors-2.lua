local lib = require("hypr.lib")

hl.monitor({ output = "eDP-1", mode = "2880x1800@60", position = "1760x2048", scale = "2", bitdepth = 10 })
hl.monitor({ output = "DP-1", mode = "5120x2880@60", position = "640x800", scale = "2", bitdepth = 10 })
hl.monitor({ output = "DP-6", mode = "3840x2400@60", position = "1520x848", scale = "2", bitdepth = 10 })
hl.monitor({ output = "DP-2", mode = "3840x2560@60", position = "1276.4x208", scale = "1.33" })
hl.monitor({ output = "HEADLESS-1", mode = "3168x2375@60", position = "3354x2128", scale = "3" })
hl.monitor({ output = "HEADLESS-2", mode = "3168x2375@60", position = "3354x2128", scale = "3" })
hl.monitor({ output = "HEADLESS-3", mode = "3168x2375@60", position = "3354x2128", scale = "3" })

lib.exec_once("xrdb ~/.Xresources")
