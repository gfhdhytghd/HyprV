local lib = require("hypr.lib")

lib.bind_plugin_fn("SUPER + TAB", "hymission", "toggle")
lib.bind_plugin_fn("SUPER + SHIFT + TAB","hymission","toggle","reverse")
lib.bind_plugin_fn("ALT + TAB", "hymission", "open", "onlycurrentworkspace")
lib.bind_plugin_fn_or_exec("SUPER + s", "hyprcapture", "open", "/home/wilf/.local/bin/hyprcapture-ui")

-- TEMP: hymission grouped-window test (2026-08-10). Remove this block after testing.
hl.bind("SUPER + ALT + G", hl.dsp.group.toggle())
hl.bind("SUPER + ALT + left", hl.dsp.window.move({ into_or_create_group = "left" }))
hl.bind("SUPER + ALT + right", hl.dsp.window.move({ into_or_create_group = "right" }))
hl.bind("SUPER + ALT + up", hl.dsp.window.move({ into_or_create_group = "up" }))
hl.bind("SUPER + ALT + down", hl.dsp.window.move({ into_or_create_group = "down" }))
hl.bind("SUPER + ALT + TAB", hl.dsp.group.next())
hl.bind("SUPER + ALT + SHIFT + TAB", hl.dsp.group.prev())
hl.bind("SUPER + ALT + O", hl.dsp.window.move({ out_of_group = true }))

lib.call_plugin_fn("hymission", "gesture", {
  fingers = 4,
  direction = "vertical",
  action = "toggle",
  args = "recommand",
  mods = "SUPER",
})

lib.plugin_config_if_available("plugin.hymission.niri_mode", {
  plugin = {
    hymission = {
      pick_labels_enabled = 1,
      pick_labels_show = 0,
      pick_labels_direct_activate = 0,
      pick_labels_mode = "spatial",
      grouped_windows_policy = "expanded",
      grouped_windows_collapsed_labels = 1,
      grouped_windows_collapsed_scroll = 1,
      backdrop_blur = 0,
      niri_mode = 0,
      layout_engine = "natural",
      hide_hyprbars_during_overview=1,
      layout_engine_onlycurrentworkspace = "natural",
      toggle_switch_mode = 1,
      switch_release_key = "Super_L",
      switch_toggle_auto_next = 1,
      hover_expand_scale = 1.18,
      multi_workspace_sort_recent_first = 1,
      one_workspace_per_row = 0,
      outer_padding_top = 92,
      outer_padding_right = 32,
      outer_padding_bottom = 32,
      outer_padding_left = 32,
      row_spacing = 32,
      column_spacing = 32,
      niri_scrolling_preview_gap = 24,
      min_window_length = 120,
      small_window_boost = 1.35,
      max_preview_scale = 0.95,
      min_slot_scale = 0.10,
      layout_scale_weight = 1.0,
      layout_space_weight = 0.10,
      overview_focus_follows_mouse = 1,
      only_active_workspace = 0,
      workspace_change_keeps_overview = 1,
      hide_bar_when_strip = 1,
      hide_bar_animation = 1,
      hide_bar_animation_blur = 1,
      hide_bar_animation_move_multiplier = 0.8,
      hide_bar_animation_scale_divisor = 1.1,
      hide_bar_animation_alpha_end = 0,
      bar_single_mission_control = 0,
      show_focus_indicator = 0,
      workspace_strip_anchor = "left",
      workspace_strip_thickness = 160,
      workspace_strip_gap = 24,
    },
  },
})

lib.plugin_config_if_available("plugin.hymission.workspace_overview_max_preview_scale", {
  plugin = {
    hymission = {
      workspace_overview_max_preview_scale = 0.5,
    },
  },
})

local hyprcapture_config = {
  plugin = {
    hypr_edgehover = {
      enabled = 1
    },
    hyprcapture = {
      notification_backend = "system",
      default_mode = "region",
      fullscreen_scope = "all",
      window_background = "follow-system",
      window_border = "keep",
      window_shadow = "keep",
      save = true,
      record_countdown_seconds = 3,
      confirm_before_capture = 0,
      clipboard = true,
      show_thumbnail = true,
      save_dir = "/home/wilf/Pictures/Screenshots",
      filename_template = "Screenshot-%Y-%m-%d-%H%M%S-{window_class}-{window_title}.png",
      dynamic_window_metadata = true,
      record_save_dir = "/home/wilf/Videos/Screenrecords",
      record_fps = 60,
      record_window_fps_limit = 60,
      record_window_real_bg_fps_limit = 60,
      record_codec = "auto",
      record_solid_alpha = true,
      record_gsr_flags = "",
      record_window_backend = "compositor",
      include_cursor = false,
      thumbnail_timeout_ms = 5000,
      thumbnail_monitor  = "all",
      helper = "/home/wilf/.local/bin/hyprcapture-ui",
      overlay_scope = 'all',
      watermark_position = "bottom_right",
      watermark_width = "30%",
      fusion_mode = 1,
    },
  },
}


lib.plugin_config_if_available("plugin.hyprcapture.default_mode", hyprcapture_config)

lib.plugin_config_if_available("plugin.touch_gestures.sensitivity", {
  plugin = {
    touch_gestures = {
      sensitivity = 8,
      workspace_swipe_fingers = 3,
      long_press_delay = 400,
      resize_on_border_long_press = true,
      edge_margin = 30,
      emulate_touchpad_swipe = true,
    },
  },
})
