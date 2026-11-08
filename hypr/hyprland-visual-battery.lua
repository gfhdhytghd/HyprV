hl.config({
  general = {
    gaps_in = 1,
    -- Compensate the running scrolling layout horizontal inset (1 px).
    gaps_out = { top = 2, right = 1, bottom = 2, left = 1 },
    border_size = 0,
  },
  decoration = {
    rounding = 0,
    active_opacity = 1.0,
    inactive_opacity = 1.0,
    fullscreen_opacity = 1.0,
    blur = {
      enabled = false,
    },
    shadow = {
      enabled = false,
    },
  },
  animations = {
    enabled = false,
  },
})

-- Battery mode ignores client window alpha. Layer-shell gaps stay transparent.
hl.window_rule({
  name = "battery-windows-solid",
  match = { class = ".*" },
  force_rgbx = true,
})
