local lib = require("hypr.lib")

-- SuperPower: use any attached motherboard display normally and keep the
-- Sunshine virtual display identical to the source laptop panel.
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = "1" })
hl.monitor({ output = "HEADLESS-1", mode = "2732x2048@120", position = "0x0", scale = "2", transform = 0, bitdepth = 10, cm = "srgb" })
lib.exec_once("xrdb ~/.Xresources")
