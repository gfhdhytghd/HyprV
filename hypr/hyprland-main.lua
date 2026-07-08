local lib = require("hypr.lib")

hl.config({
  general = {
    layout = "scrolling",
  },
  scrolling = {
    fullscreen_on_one_column = true,
    column_width = 0.85,
    focus_fit_method = 1,
    follow_focus = true,
    follow_min_visible = 0.14,
  },
  input = {
    kb_layout = "us",
    kb_variant = "",
    kb_model = "",
    kb_options = "",
    kb_rules = "",
    follow_mouse = 1,
    sensitivity = 0.1,
    touchpad = {
      natural_scroll = true,
      drag_lock = false,
      disable_while_typing = false,
    },
  },
  misc = {
    disable_hyprland_logo = true,
  },
  render = {
    direct_scanout = true,
  },
  cursor = {
    no_hardware_cursors = 1,
  },
  dwindle = {
    preserve_split = false,
  },
  gestures = {
    workspace_swipe_cancel_ratio = 0.05,
    workspace_swipe_touch = true,
  },
  xwayland = {
    force_zero_scaling = true,
  },
})

hl.device({
  name = "“apple”的鼠标",
  sensitivity = -0.4,
})

hl.device({
  name = "logitech-usb-receiver-mouse",
  enabled = true,
  sensitivity = -0.3,
  accel_profile = "flat",
})

hl.gesture({ fingers = 4, direction = "vertical", action = "workspace" })

lib.call_plugin_fn("hymission", "gesture", {
  fingers = 4,
  direction = "horizontal",
  action = "scroll",
  args = "layout",
})
hl.gesture({ fingers = 3, direction = "horizontal", action = function() hl.exec_cmd("$HOME/.config/HyprV/hypr/scripts/sidechat.sh") end })
hl.gesture({ fingers = 3, direction = "vertical", action = function() hl.exec_cmd("$HOME/.config/HyprV/hypr/scripts/dropdown.sh") end })
hl.gesture({ fingers = 3, direction = "vertical", mods = "SUPER", action = function() hl.exec_cmd("$HOME/.config/HyprV/hypr/scripts/dropcalender.sh") end })
hl.gesture({ fingers = 3, direction = "swipe", mods = "SUPER", scale = 1.5, action = "move" })

hl.bind("SUPER + SHIFT + right", hl.dsp.layout("swapcol r"))
hl.bind("SUPER + SHIFT + left", hl.dsp.layout("swapcol l"))
hl.bind("SUPER + CTRL + right", hl.dsp.layout("promote"))
hl.bind("SUPER + SHIFT + code:35", hl.dsp.layout("colresize +0.1"))
hl.bind("SUPER + SHIFT + code:34", hl.dsp.layout("colresize -0.1"))

hl.animation({ leaf = "workspaces", enabled = true, speed = 8, bezier = "default", style = "slidevert" })
hl.animation({ leaf = "workspacesIn", enabled = true, speed = 8, bezier = "default", style = "slidevert" })
hl.animation({ leaf = "workspacesOut", enabled = true, speed = 8, bezier = "default", style = "slidevert" })
