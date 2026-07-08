local lib = require("hypr.lib")
local script = "~/.config/HyprV/hypr/scripts"

lib.bind_exec("xf86audioraisevolume", script .. "/volume --inc")
lib.bind_exec("xf86audiolowervolume", script .. "/volume --dec")
lib.bind_exec("xf86AudioMicMute", script .. "/volume --toggle-mic")
lib.bind_exec("xf86audioMute", script .. "/volume --toggle")
lib.bind_exec("XF86AudioMute", "wpctl set-volume @DEFAULT_AUDIO_SINK@ 0%", { locked = true })
lib.bind_exec("XF86AudioPlay", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("XF86AudioPause", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("code:179", "playerctl play-pause && notify-send 'Play-Pause'")
lib.bind_exec("SHIFT + XF86AudioPlay", "playerctl next && notify-send 'Next-Music'")
lib.bind_exec("CTRL + XF86AudioPlay", "playerctl previous && notify-send 'Previous-Music'")
lib.bind_exec("SHIFT + code:179", "playerctl next")
lib.bind_exec("CTRL + code:179", "playerctl previous")

lib.bind_exec("SUPER + XF86MonBrightnessDown", script .. "/kb-brightness --dec")
lib.bind_exec("SUPER + XF86MonBrightnessUp", script .. "/kb-brightness --inc")
lib.bind_exec("XF86MonBrightnessDown", script .. "/brightness --dec")
lib.bind_exec("XF86MonBrightnessUp", script .. "/brightness --inc")

lib.bind_exec("XF86AudioNext", "playerctl next || playerctl position `bc <<< \"100 * $(playerctl metadata mpris:length) / 1000000 / 100\"`", { locked = true })
lib.bind_exec("XF86AudioPrev", "playerctl previous", { locked = true })
lib.bind_exec("XF86AudioMute", "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle", { locked = true })
