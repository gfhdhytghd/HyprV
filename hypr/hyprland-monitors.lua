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
local main_height = transform % 2 == 0 and 1728 or 3072
local headless_y = 1300 + math.floor((main_height - 1728) / 2)

hl.monitor({
  output = "DP-4",
  mode = "6144x3456@60.02",
  position = "0x0",
  scale = "2",
  transform = transform,
  bitdepth = 10,
  cm = "srgb",
})

hl.monitor({
  output = "HEADLESS-1",
  mode = "2752x2064@60",
  position = main_width .. "x" .. headless_y,
  scale = "2",
  transform = 1,
})

lib.exec_once("xrdb ~/.Xresources")
