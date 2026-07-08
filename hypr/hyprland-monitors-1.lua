local lib = require("hypr.lib")

hl.monitor({ output = "eDP-1", mode = "2880x1800@60", position = "1760x2048", scale = "2", bitdepth = 10 })
hl.monitor({ output = "DP-6", disabled = true })
hl.monitor({ output = "DP-2", mode = "5120x2880@60", position = "1276.4x208", scale = "1.33" })
hl.monitor({ output = "HEADLESS-2", mode = "3184x2232@60", position = "3200x2204", scale = "3" ,transform = 1})
hl.monitor({ output = "DP-1", disabled = true })

lib.exec_once("xrdb ~/.Xresources")
