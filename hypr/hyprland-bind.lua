local lib = require("hypr.lib")

local main = "SUPER"

lib.bind_exec("SUPER_L", "true")
lib.bind_exec(main .. " + Q", "kitty")
hl.bind(main .. " + W", hl.dsp.window.close())
lib.bind_exec(main .. " + L", "hyprlock")
lib.bind_exec(main .. " + M", "wlogout --protocol layer-shell -b 5")
lib.bind_exec(main .. " + F", "code")
lib.bind_exec(main .. " + SHIFT + F", "cursor")
hl.bind(main .. " + SHIFT + M", hl.dsp.exit())
lib.bind_exec(main .. " + E", "dolphin")
hl.bind(main .. " + V", hl.dsp.window.float({ action = "toggle" }))
lib.bind_exec("ALT + SPACE", "rofi -show drun")
hl.bind("ALT + A", hl.dsp.pass({ window = "class:^(wechat)$" }))
hl.bind(main .. " + P", hl.dsp.window.pseudo())
hl.bind(main .. " + J", hl.dsp.layout("togglesplit"))
lib.bind_plugin_fn_or_exec(
  main .. " + O",
  "hymission",
  "fullscreen",
  "hyprctl dispatch fullscreen 1 toggle",
  { mode = "maximized", action = "toggle" }
)
hl.bind(main .. " + Y", hl.dsp.window.float({ action = "set" }))
lib.bind_exec(main .. " + SHIFT + V", "cliphist list | wofi -S dmenu | cliphist decode | wl-copy")
hl.bind(main .. " + Y", hl.dsp.window.move({ monitor = "1" }))
hl.bind(main .. " + Y", hl.dsp.window.resize({ x = 1415, y = 2075 }))
hl.bind(main .. " + Y", hl.dsp.window.move({ x = 1772, y = 861 }))
lib.bind_exec(main .. " + Y", "quickshell kill -p $HOME/.config/HyprV/quickshell")
lib.bind_plugin_fn_or_exec(
  main .. " + SHIFT + O",
  "hymission",
  "fullscreen",
  "hyprctl dispatch fullscreen 0 toggle",
  { mode = "fullscreen", action = "toggle" }
)
hl.bind(main .. " + D", hl.dsp.focus({ workspace = "100" }))
hl.bind(main .. " + SHIFT + up", hl.dsp.workspace.move({ monitor = "+1" }))
hl.bind(main .. " + SHIFT + down", hl.dsp.workspace.move({ monitor = "-1" }))

lib.bind_exec(main .. " + I", "zen-browser")
lib.bind_exec(main .. " + B", "~/.config/HyprV/hypr/scripts/toggle-battery-mode.sh")
lib.bind_exec(main .. " + R", "~/.config/hypr/scripts/record-script.sh --fullscreen-sound")
lib.bind_exec(main .. " + SHIFT + R", "~/.config/HyprV/quickshell/scripts/reload.sh && fcitx5")
lib.bind_exec(main .. " + SHIFT + S", "~/.config/hypr/scripts/ocr.sh")
lib.bind_exec(main .. " + N", "swaync-client -t -sw")
lib.bind_exec(main .. " + SHIFT + b", "brightnessctl s 50%")
lib.bind_exec(main .. " + A", "quickshell ipc call -p $HOME/.config/HyprV/quickshell controlPanel toggle")

hl.bind(main .. " + left", hl.dsp.focus({ direction = "left" }))
hl.bind(main .. " + right", hl.dsp.focus({ direction = "right" }))
hl.bind(main .. " + up", hl.dsp.focus({ direction = "up" }))
hl.bind(main .. " + down", hl.dsp.focus({ direction = "down" }))

for i = 1, 10 do
  local key = tostring(i % 10)
  lib.bind_exec(main .. " + " .. key, "~/.config/hypr/scripts/workspace_action.sh workspace " .. i)
  lib.bind_exec(main .. " + SHIFT + " .. key, "~/.config/hypr/scripts/workspace_action.sh movetoworkspace " .. i)
end

hl.bind(main .. " + SHIFT + mouse_down", hl.dsp.window.move({ workspace = "-1" }))
hl.bind(main .. " + SHIFT + mouse_up", hl.dsp.window.move({ workspace = "+1" }))
hl.bind(main .. " + mouse_down", hl.dsp.focus({ workspace = "e+1" }))
hl.bind(main .. " + mouse_up", hl.dsp.focus({ workspace = "e-1" }))

hl.bind(main .. " + mouse:272", hl.dsp.window.drag(), { mouse = true })
hl.bind(main .. " + SHIFT + mouse:272", hl.dsp.window.resize(), { mouse = true })

require("hypr.media-binds")
require("hypr.rog-g15-strix-2021-binds")
