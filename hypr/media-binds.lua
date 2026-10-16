local lib = require("hypr.lib")
local script = "~/.config/HyprV/hypr/scripts"

lib.bind_exec("XF86AudioPlay", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("XF86AudioPause", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("code:179", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("SHIFT + XF86AudioPlay", "playerctl next && notify-send 'Next-Music'")
lib.bind_exec("CTRL + XF86AudioPlay", "playerctl previous && notify-send 'Previous-Music'")
lib.bind_exec("SHIFT + code:179", "playerctl next")
lib.bind_exec("CTRL + code:179", "playerctl previous")


lib.bind_exec("XF86AudioNext", "playerctl next || playerctl position `bc <<< \"100 * $(playerctl metadata mpris:length) / 1000000 / 100\"`", { locked = true })
lib.bind_exec("XF86AudioPrev", "playerctl previous", { locked = true })

local controls = require("hypr.controls")
local repeat_locked = { locked = true, repeating = true }
hl.bind("XF86AudioRaiseVolume", function() controls.volume("inc") end, repeat_locked)
hl.bind("XF86AudioLowerVolume", function() controls.volume("dec") end, repeat_locked)
hl.bind("XF86AudioMute", function() controls.volume("toggle") end, { locked = true })
hl.bind("XF86AudioMicMute", function() controls.volume("toggle-mic") end, { locked = true })
hl.bind("XF86MonBrightnessUp", function() controls.brightness_step(2) end, repeat_locked)
hl.bind("XF86MonBrightnessDown", function() controls.brightness_step(-2) end, repeat_locked)
hl.bind("ALT + XF86AudioRaiseVolume", function() controls.brightness_step(2) end, repeat_locked)
hl.bind("ALT + XF86AudioLowerVolume", function() controls.brightness_step(-2) end, repeat_locked)
hl.bind("SUPER + XF86MonBrightnessUp", function() controls.keyboard_step(1) end, repeat_locked)
hl.bind("SUPER + XF86MonBrightnessDown", function() controls.keyboard_step(-1) end, repeat_locked)
