local root = (os.getenv("HOME") or "/home/wilf") .. "/.config/HyprV"
package.path = root .. "/?.lua;" .. root .. "/?/init.lua;" .. package.path

local lib = require("hypr.lib")
lib.setup_package_path()

require("hypr.hyprland-monitors")
require("hypr.hyprland-main")
require("hypr.hyprland-exec-once")
require("hypr.hyprland-visual-battery")
require("hypr.hyprland-bind")
require("hypr.hyprland-dropitem")
require("hypr.hyprland-winrule")
require("hypr.hyprland-plugins")
