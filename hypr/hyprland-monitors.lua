package.loaded["hypr.brightness-hdr"] = nil
local lib = require("hypr.lib")

local transform = 0
local state_path = (os.getenv("XDG_STATE_HOME") or ((os.getenv("HOME") or "/home/wilf") .. "/.local/state"))
  .. "/hyprv/monitor-transform"
local state_file = io.open(state_path, "r")
if state_file then
  local saved_transform = tonumber(state_file:read("*l"))
  state_file:close()
  if saved_transform == 0 or saved_transform == 3 then
    transform = saved_transform
  end
end

local main_width = transform % 2 == 0 and 3072 or 1728
-- Moonlight owns HEADLESS-100; Viewflow manages its own outputs.
local main_height = transform % 2 == 0 and 1728 or 3072
local headless_y = 1590 + math.floor((main_height - 1728) / 2)

hl.monitor({
  output = "DP-4",
  mode = "6144x3456@60.02",
  position = "0x0",
  scale = "2",
  transform = transform,
  bitdepth = 10,
  cm = "hdr",
  sdr_max_luminance = require("hypr.brightness-hdr"),
})

hl.monitor({
  output = "HEADLESS-100",
  mode = "2752x2064@120",
  position = main_width .. "x" .. headless_y,
  scale = "2",
  transform = 0,
})

-- Reserve a workspace for the output created on demand by Sunshine's
-- Extend Screen / Remote First prep commands. Their undo removes it.
hl.workspace_rule({ workspace = "name:moonlight", monitor = "HEADLESS-100", default = true })
hl.workspace_rule({ workspace = "2", monitor = "DP-4" })
lib.exec_once("xrdb ~/.Xresources")

-- Viewflow remote desktop: keep the live route geometry across config reloads.
hl.monitor({
  output = "HEADLESS-13",
  mode = "3840x2400@60",
  position = "3072x415",
  scale = "2",
  transform = 0,
})
