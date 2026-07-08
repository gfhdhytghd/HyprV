hl.config({
  general = {
    gaps_in = 5,
    gaps_out = { top = 0, right = 10, bottom = 10, left = 10 },
    border_size = 2,
    col = {
      active_border = "rgb(cdd6f4)",
      inactive_border = "rgba(595959aa)",
    },
    resize_on_border = true,
    extend_border_grab_area = true,
  },
  misc = {
    disable_hyprland_logo = true,
  },
  decoration = {
    rounding = 17,
    blur = {
      enabled = true,
      size = 5,
      ignore_opacity = true,
      special = true,
      passes = 4,
      new_optimizations = true,
      xray = false,
    },
    shadow = {
      enabled = true,
      range = 7,
      render_power = 4,
      color = "rgba(1a1a1aee)",
      offset = { 0, 0 },
    },
  },
  animations = {
    enabled = true,
  },
})

hl.curve("myBezier", { type = "bezier", points = { { 0.10, 0.9 }, { 0.1, 1.05 } } })
hl.animation({ leaf = "windows", enabled = true, speed = 7, bezier = "myBezier", style = "slide" })
hl.animation({ leaf = "windowsOut", enabled = true, speed = 7, bezier = "myBezier", style = "slide" })
hl.animation({ leaf = "border", enabled = true, speed = 10, bezier = "default" })
hl.animation({ leaf = "fade", enabled = true, speed = 7, bezier = "default" })
hl.animation({ leaf = "workspaces", enabled = true, speed = 6, bezier = "default" })
