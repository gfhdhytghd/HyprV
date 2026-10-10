-- Keep Viewflow off the numbered workspace strip. Windows manages window geometry.
local workspace_name = "special:viewflow"
hl.window_rule({ name = "viewflow-remote-floating", match = { workspace = workspace_name }, float = true })
hl.window_rule({ name = "viewflow-macos-floating", match = { workspace = "name:viewflow-underlay" }, float = true })
local applying = false
local function keep_floating(window)
    if applying or not window or not window.mapped or window.floating then return end
    local workspace = window.workspace
    if not workspace or (workspace.name ~= workspace_name and workspace.name ~= "viewflow-underlay") then return end
    applying = true
    hl.dsp.window.float({ action = "enable", window = window })()
    applying = false
end
hl.on("window.open", keep_floating)
hl.on("window.move_to_workspace", keep_floating)
hl.on("window.update_rules", keep_floating)
-- The launcher owns output geometry and creates a named (non-numbered) workspace.
for _, name in ipairs({ workspace_name, "viewflow-underlay" }) do
    local workspace = hl.get_workspace(name)
    if workspace then
        for _, window in ipairs(hl.get_workspace_windows(workspace)) do keep_floating(window) end
    end
end
