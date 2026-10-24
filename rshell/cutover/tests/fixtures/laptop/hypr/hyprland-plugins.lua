local lib = require("hypr.lib")

lib.bind_plugin_fn("SUPER + comma", "hymission", "stage_toggle")
lib.bind_plugin_fn("SUPER + TAB", "hymission", "toggle")
lib.bind_plugin_fn("SUPER + SHIFT + TAB","hymission","toggle","reverse")
lib.bind_plugin_fn("ALT + TAB", "hymission", "open", "onlycurrentworkspace")
lib.bind_plugin_fn_or_exec("SUPER + s", "hyprcapture", "open", "/home/wilf/.local/bin/hyprcapture-ui")
lib.bind_plugin_fn("SUPER + ALT + s", "hyprcapture", "open_editor")

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

lib.call_plugin_fn("hymission", "gesture", {
  fingers = 4,
  direction = "horizontal",
  action = "stage_toggle",
  mods = "SUPER",
})

lib.plugin_config_if_available("plugin.hymission.niri_mode", {
  plugin = {
    hymission = {
      stage_enabled = 0, -- 启动时默认关闭；运行时由快捷键/手势切换
      stage_show_active = 0,
      stage_active_glow = 0,
      stage_smartisan_mode = 0,
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
      gesture_close_restores_focus = 0,
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

lib.plugin_config_if_available("plugin.hypr_edgehover.enabled", {
  plugin = {
    hypr_edgehover = {
      enabled = 1,
    },
  },
})

local hyprcapture_config = {
  plugin = {
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
      record_window_backend = "auto",
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

lib.plugin_config_if_available("plugin.hyprcapture.in_place_edit_toolbar", {
  plugin = {
    hyprcapture = {
      in_place_edit_toolbar = true,
      language = "auto",
    },
  },
})

lib.plugin_config_if_available("plugin.hyprcapture.remember_settings", {
  plugin = {
    hyprcapture = {
      remember_settings = true,
    },
  },
})

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

-- UltraEdge: optional edge triggers.
do
  local edge = hl.plugin and hl.plugin.ultraedge
  if edge and type(edge.rule) == "function" then
    edge.clear()

    local stage = hl.plugin and hl.plugin.hymission
    if stage and type(stage.stage_open) == "function" and
        type(stage.stage_close) == "function" and type(stage.stage_region) == "function" then
      assert(edge.rule({
        monitor = "*", target = "left", mode = "move",
        move_ms = 200, movement_gap_ms = 80, noise = 0.5, band = 2, cooldown_ms = 400,
        on_enter = function() stage.stage_open() end,
        on_exit = function() stage.stage_close() end,
        hold = function(monitor) return stage.stage_region(monitor) end,
      }))
    end

    -- Both outward gestures toggle the persistent compact/full bar state.
    local function toggle_bar(monitor)
      local quoted = "'" .. monitor:gsub("'", "'\\''") .. "'"
      hl.exec_cmd("quickshell ipc -p /home/wilf/.config/HyprV/quickshell call bar toggle " .. quoted)
    end
    for _, mode in ipairs({"push", "move"}) do
      assert(edge.rule({
        monitor = "*", target = "top", mode = mode,
        move_ms = 200, movement_gap_ms = 80, pause_ms = 180,
        noise = 0.5, band = 2, cooldown_ms = 500, exit_distance = 20,
        on_enter = toggle_bar,
      }))
    end

    -- The right strip follows actual AI-HUB window geometry, including the gap
    -- between its right edge and the output boundary. No fixed 360/370 px snapshot.
    local function shell_quote(s) return "'" .. s:gsub("'", "'\\''") .. "'" end
    local function ai_region(name)
      for _, win in ipairs(hl.get_windows()) do
        local mon, ws = win.monitor, win.workspace
        if win.class == "ai-hub-firefox" and win.mapped and not win.hidden and
            ws and ws.name ~= "special:sidechat_hidden" and mon and mon.name == name then
          local at = win.at
          -- monitor.width/height are physical dimensions in Hyprland 0.56.2;
          -- logical dimensions follow scale + transform.
          local w, h = mon.width / mon.scale, mon.height / mon.scale
          if mon.transform % 2 == 1 then w, h = h, w end
          local left = math.max(mon.x, at.x)
          return { x = left, y = mon.y, width = mon.x + w - left, height = h }
        end
      end
      return nil
    end
    assert(edge.rule({
      monitor = "*", target = "right", mode = "move",
      move_ms = 200, movement_gap_ms = 80, noise = 0.5, band = 2, cooldown_ms = 400,
      hold_wait_ms = 5000, exit_distance = 80,
      on_enter = function(monitor) hl.exec_cmd("bash /home/wilf/.config/HyprV/hypr/scripts/sidechat.sh show " .. shell_quote(monitor) .. " --no-cursor-warp") end,
      on_exit = function() hl.exec_cmd("bash /home/wilf/.config/HyprV/hypr/scripts/sidechat.sh hide '' --no-cursor-warp") end,
      hold = ai_region,
    }))
  end
end
