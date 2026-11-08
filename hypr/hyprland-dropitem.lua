local lib = require("hypr.lib")

hl.window_rule({
  name = "dropdown-terminal",
  match = { class = "^(kitty-dropdown)$" },
  size = "1080 225",
  float = true,
  move = "monitor_w*0.125 60",
})
hl.bind("SUPER + SHIFT + T", function() require("hypr.panels").dropdown() end)

hl.bind("SUPER + SPACE", function() require("hypr.panels").sidechat() end)
-- Copilot is assigned to hold-to-talk in hypr.lais-voice.
-- lib.bind_exec("SUPER + SHIFT + F23", "bash $HOME/.config/hypr/scripts/sidechat.sh")
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

lib.bind_exec(
  "SUPER + C",
  "\"$HOME/.local/bin/hyprv-shell\" ipc call calendar toggle"
)
hl.window_rule({
  name = "dropdown-calendar",
  match = { class = "^(chrome-calendar\\.google\\.com.*Default)$" },
  float = true,
  size = "monitor_w*0.592 monitor_h*0.6",
  move = "monitor_w*0.4 60",
})
