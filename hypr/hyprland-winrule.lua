local function wr(spec)
  hl.window_rule(spec)
end

local function lr(spec)
  hl.layer_rule(spec)
end

wr({ name = "float-pavucontrol", match = { class = "^(pavucontrol)$" }, float = true })
wr({ name = "float-dankcalendar", match = { class = "^(com\\.danklinux\\.dankcalendar)$" }, float = true, center = true, animation = "popup" })
wr({ name = "float-blueman", match = { class = "^(blueman-manager)$" }, float = true })
wr({ name = "float-nm-connection-editor", match = { class = "^(nm-connection-editor)$" }, float = true })
wr({ name = "float-showmethekey", match = { class = "^(one\\.alynx\\.showmethekey)$" }, float = true })
wr({ name = "pin-showmethekey-gtk", match = { class = "^(showmethekey-gtk)$" }, float = true, pin = true })
wr({ name = "no-anim-wofi", match = { class = "^(wofi)$" }, no_anim = true })

wr({
  name = "xwayland-video-bridge-fixes",
  match = { class = "xwaylandvideobridge" },
  no_initial_focus = true,
  no_focus = true,
  no_anim = true,
  no_blur = true,
  max_size = "1 1",
  opacity = "0.0",
})

wr({
  name = "kdeconnect-fullscreen",
  match = { class = "^(org\\.kde\\.kdeconnect\\.daemon)$" },
  float = true,
  no_blur = true,
  decorate = false,
  no_shadow = true,
  move = "0 0",
  size = "monitor_w*1 monitor_h*1",
  animation = "popin",
  no_focus = true,
  no_initial_focus = true,
})

wr({ name = "tile-warp", match = { class = "dev.warp.Warp" }, tile = true })
wr({ name = "pip-float", match = { title = "^([Pp]icture[-\\s]?[Ii]n[-\\s]?[Pp]icture)(.*)$" }, float = true })

-- Restore the desktop application layout without leaving the scrolling layout.
local function session_slot(name, match, workspace, scrolling_width)
  wr({
    name = "session-" .. name,
    match = match,
    workspace = workspace .. " silent",
    no_initial_focus = true,
    tile = true,
    scrolling_width = scrolling_width,
  })
end

-- Workspace 2: Discord / Telegram over QQ / WeChat.
session_slot("discord", { class = "^(discord)$" }, "2", 0.5)
session_slot("qq", { class = "^(QQ)$" }, "2", 0.5)
session_slot("telegram", { class = "^(org\\.telegram\\.desktop)$" }, "2", 0.5)
session_slot("wechat", { class = "^(wechat)$" }, "2", 0.5)

-- Workspace 3: Gmail / Outlook / Feishu in three horizontal columns.
session_slot("gmail", { class = "^(chrome-fmgjjmmmlfnkbppncabfkddbjimcfncm-Default)$" }, "3", 0.45)
session_slot("outlook", { class = "^(msedge-_faolnafnngnfdaknnbpnkhgohbobgegn-Profile_1)$" }, "3", 0.45)
session_slot("feishu", { title = "^(飞书)$" }, "3", 0.45)

-- Workspaces 4-6: one or two full-height application columns.
session_slot("zen", { class = "^(zen)$" }, "4", 0.85)
session_slot("chatgpt", { class = "^(Chatgpt)$" }, "5", 0.85)
session_slot("code-oss", { class = "^(code-oss)$" }, "5", 0.85)
session_slot("cider", { class = "^(Cider)$" }, "6", 0.85)

wr({ name = "dialog-open-file", match = { title = "^(Open File)(.*)$" }, float = true })
wr({ name = "dialog-open-cn", match = { title = "^(打开)(.*)$" }, float = true })
wr({ name = "dialog-select-file", match = { title = "^(Select a File)(.*)$" }, float = true })
wr({ name = "dialog-select-cn", match = { title = "^(选择)(.*)$" }, float = true })
wr({ name = "dialog-wallpaper", match = { title = "^(Choose wallpaper)(.*)$" }, float = true })
wr({ name = "dialog-open-folder", match = { title = "^(Open Folder)(.*)$" }, float = true })
wr({ name = "dialog-save-as", match = { title = "^(Save As)(.*)$" }, float = true })
wr({ name = "dialog-save-cn", match = { title = "^(保存)(.*)$" }, float = true })
wr({ name = "dialog-library", match = { title = "^(Library)(.*)$" }, float = true })
wr({ name = "dialog-file-upload", match = { title = "^(File Upload)(.*)$" }, float = true })
wr({ name = "dialog-ocr", match = { title = "^(OCR )(.*)$" }, float = true, animation = "popup" })
wr({ name = "dialog-unlock-db", match = { title = "^(解锁数据库)(.*)$" }, float = true, animation = "popup" })
wr({ name = "dialog-image-viewer", match = { title = "^(图片查看器)$" }, float = true, animation = "popup" })
wr({ name = "dialog-video-player", match = { title = "^(视频播放器)$" }, float = true, animation = "popup" })
wr({ name = "dialog-image-video", match = { title = "^(图片和视频)$" }, float = true, center = true, animation = "popup" })
wr({ name = "dialog-log", match = { title = "^(.*聊天记录)(.*)$" }, float = true, animation = "popup" })

wr({ name = "steam-tearing", match = { class = "(steam_app)" }, immediate = true })

wr({ name = "xwayland-float-popin", match = { xwayland = true, float = true }, animation = "popin" })
wr({ name = "xwayland-float-no-blur", match = { xwayland = true, float = true }, no_blur = true })
wr({ name = "xwayland-float-no-shadow", match = { xwayland = true, float = true }, no_shadow = true })
wr({ name = "xwayland-float-square", match = { xwayland = true, float = true }, rounding = 0 })
wr({ name = "xwayland-float-undecorated", match = { xwayland = true, float = true }, decorate = false })

lr({ name = "layer-xray-all", match = { namespace = ".*" }, xray = false })
lr({ name = "layer-no-anim-walker", match = { namespace = "walker" }, no_anim = true })
lr({ name = "layer-no-anim-selection", match = { namespace = "selection" }, no_anim = true })
lr({ name = "layer-no-anim-overview", match = { namespace = "overview" }, no_anim = true })
lr({ name = "layer-no-anim-anyrun", match = { namespace = "anyrun" }, no_anim = true })
lr({ name = "layer-no-anim-indicator", match = { namespace = "indicator.*" }, no_anim = true })
lr({ name = "layer-no-anim-osk", match = { namespace = "osk" }, no_anim = true })
lr({ name = "layer-no-anim-hyprpicker", match = { namespace = "hyprpicker" }, no_anim = true })
lr({ name = "layer-shell-blur", match = { namespace = "shell:.*" }, blur = true, blur_popups = true, ignore_alpha = 0.05 })
lr({ name = "layer-quickshell", match = { namespace = "hyprv-quickshell" }, blur = true, blur_popups = true, ignore_alpha = 0.15 })
lr({ name = "layer-no-anim-noanim", match = { namespace = "noanim" }, no_anim = true })
lr({ name = "layer-gtk", match = { namespace = "gtk-layer-shell" }, blur = true, ignore_alpha = 0 })
lr({ name = "layer-launcher", match = { namespace = "launcher" }, blur = true, ignore_alpha = 0.5 })
lr({ name = "layer-notifications", match = { namespace = "notifications" }, blur = true, ignore_alpha = 0.69 })
lr({ name = "layer-bar", match = { namespace = "bar" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-corner", match = { namespace = "corner.*" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-dock", match = { namespace = "dock" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-indicator", match = { namespace = "indicator.*" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-overview", match = { namespace = "overview" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-cheatsheet", match = { namespace = "cheatsheet" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-sideright", match = { namespace = "sideright" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-sideleft", match = { namespace = "sideleft" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-indicator-wild", match = { namespace = "indicator*" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "layer-osk", match = { namespace = "osk" }, blur = true, ignore_alpha = 0.6 })
lr({ name = "swaync-control-center", match = { namespace = "swaync-control-center" }, blur = true, ignore_alpha = 0.05 })
lr({ name = "swaync-notification-window", match = { namespace = "swaync-notification-window" }, blur = true, ignore_alpha = 0.05 })
