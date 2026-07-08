local lib = require("hypr.lib")

hl.window_rule({
  name = "dropdown-terminal",
  match = { class = "^(alacritty-dropdown)$" },
  size = "1080 225",
  float = true,
  move = "monitor_w*0.125 60",
})
lib.bind_exec("SUPER + SHIFT + T", "$HOME/.config/hypr/scripts/dropdown.sh")

lib.bind_exec("SUPER + SPACE", "bash $HOME/.config/hypr/scripts/sidechat.sh")
lib.bind_exec("SUPER + SHIFT + F23", "bash $HOME/.config/hypr/scripts/sidechat.sh")
hl.window_rule({
  name = "dropdown-ai-hub",
  match = { class = "^(ai-hub-firefox)$" },
  float = true,
  animation = "slide",
  size = "360 monitor_h-72",
  move = "monitor_w-370 60",
})

hl.window_rule({
  name = "pin-meeting",
  match = { class = "(Meeting)$" },
  pin = true,
})

lib.bind_exec("SUPER + C", "~/.config/HyprV/hypr/scripts/dropcalender.sh")
hl.window_rule({
  name = "dropdown-calendar",
  match = { class = "^(chrome-calendar\\.google\\.com.*Default)$" },
  float = true,
  size = "monitor_w*0.592 monitor_h*0.6",
  move = "monitor_w*0.4 60",
})
