local lib = require("hypr.lib")

-- SuperPower may boot with every physical connector unplugged. Create a
-- virtual monitor and start the streaming host after Hyprland is responsive.
lib.exec_once("~/.local/bin/superpower-headless-session")
