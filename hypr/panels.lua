-- Shared by gestures, binds, edge callbacks and the one-IPC CLI wrappers.
-- No compositor queries or dispatches leave this Lua state.
local M = {}
local panels = {
  dropdown = { class = "kitty-dropdown", hidden = "special:dropdown_hidden", attempts = 20 },
  sidechat = { class = "ai-hub-firefox", hidden = "special:sidechat_hidden", attempts = 40 },
}
local function trace(p, message)
  pcall(function()
    local f = io.open(p == panels.sidechat and "/tmp/sidechat.trace" or "/tmp/dropdown.trace", "a")
    if f then f:write(os.date("%F %T"), " lua ", message, "\n"); f:close() end
  end)
end
local function guarded(p, fn)
  local ok, err = pcall(fn)
  if not ok then trace(p, "error: " .. tostring(err)) end
  return ok, err
end
local function later(p, ms, fn)
  return hl.timer(function() guarded(p, fn) end, { timeout = ms, type = "oneshot" })
end
local function hidden(p, w)
  return w.workspace and w.workspace.name == p.hidden
end
local function find(p, cleanup)
  local matches, keep = {}, nil
  for _, w in ipairs(hl.get_windows()) do
    if w.mapped and w.class == p.class then
      matches[#matches + 1] = w
      if not keep or (hidden(p, keep) and not hidden(p, w)) then keep = w end
    end
  end
  if cleanup and p == panels.sidechat then
    for _, w in ipairs(matches) do
      if w.address ~= keep.address then hl.dispatch(hl.dsp.window.close({ window = w })) end
    end
  end
  return keep
end
local function normal(w)
  -- Exclude BOTH panels, including while visible, at save and restore time.
  return w and w.mapped and w.workspace and w.workspace.id > 0
    and not w.workspace.name:match("^special:")
    and w.class ~= panels.dropdown.class and w.class ~= panels.sidechat.class
end
local function save(p)
  local w = hl.get_active_window()
  p.previous = normal(w) and { address = w.address, workspace = w.workspace.id } or nil
end
local function restore(p)
  local previous = p.previous
  p.previous = nil
  if not previous then return end
  local w = hl.get_window("address:" .. previous.address)
  local ws = hl.get_active_workspace()
  if normal(w) and ws and ws.id == previous.workspace and w.workspace.id == previous.workspace then
    hl.dispatch(hl.dsp.focus({ window = w }))
  end
end
local function pin(w, value)
  if w.pinned ~= value then hl.dispatch(hl.dsp.window.pin({ window = w })) end
end
local function geometry(p, w, mon)
  assert(mon, "panel monitor unavailable")
  local width, height = mon.width / mon.scale, mon.height / mon.scale
  if mon.transform % 2 == 1 then width, height = height, width end
  width, height = math.floor(width + 0.5), math.floor(height + 0.5)
  local side = p == panels.sidechat
  local x = side and (width - 370) or math.modf((width - 1080) / 2)
  hl.dispatch(hl.dsp.window.float({ action = "enable", window = w }))
  hl.dispatch(hl.dsp.window.resize({ x = side and 360 or 1080, y = side and math.max(1, height - 72) or 225, window = w }))
  hl.dispatch(hl.dsp.window.move({ x = mon.x + x, y = mon.y + 60, relative = false, window = w }))
end
local function show(p, w, mon, workspace)
  pin(w, false)
  hl.dispatch(hl.dsp.window.move({ workspace = tostring(workspace), window = w, follow = false }))
  geometry(p, w, mon)
  hl.dispatch(hl.dsp.focus({ window = w }))
  pin(w, true)
  if p == panels.sidechat then hl.dispatch(hl.dsp.window.alter_zorder({ mode = "top", window = w })) end
  trace(p, "shown addr=" .. w.address .. " ws=" .. workspace)
end
local function hide(p, w)
  pin(w, false)
  -- Restore before moving so the compositor cannot choose a special window.
  restore(p)
  hl.dispatch(hl.dsp.window.move({ workspace = p.hidden, window = w, follow = false }))
  trace(p, "hidden addr=" .. w.address)
end
local function shell_quote(s) return "'" .. s:gsub("'", "'\\''") .. "'" end
local function finish_launch(p, terminate)
  local path = p.pidfile
  p.pidfile = nil
  if not path then return end
  if terminate then
    local f = io.open(path, "r")
    if f then
      local pid = f:read("*l")
      f:close()
      if pid and pid:match("^%d+$") then hl.exec_cmd("kill " .. pid) end
    end
  end
  os.remove(path)
end
local function launch(p)
  p.pidfile = os.tmpname()
  local script = (os.getenv("HOME") or "/home/wilf") .. "/.config/HyprV/hypr/scripts/panel-launch.sh"
  hl.exec_cmd("bash " .. shell_quote(script) .. " " .. (p == panels.dropdown and "dropdown" or "sidechat") .. " " .. shell_quote(p.pidfile))
end
local function run(p, action, monitor)
  assert(action == "toggle" or action == "show" or action == "hide" or action == "reflow", "invalid panel action")
  local mon
  if monitor and monitor ~= "" then
    mon = hl.get_monitor(monitor)
    assert(mon and mon.name == monitor, "Unknown monitor: " .. monitor)
  end
  if p == panels.sidechat and action == "toggle" then
    if p.debounce then trace(p, "debounced"); return end
    p.debounce = later(p, 300, function() p.debounce = nil end)
  end
  local w = find(p, true)
  if action == "reflow" then
    if w and not hidden(p, w) then geometry(p, w, w.monitor) end
    return
  end
  if p.pending then
    -- Serialize requests while spawning without blocking the compositor.
    p.pending.action = action == "toggle" and (p.pending.action == "hide" and "show" or "hide") or action
    if mon then p.pending.monitor = mon; p.pending.workspace = mon.active_workspace.id end
    return
  end
  if action == "hide" or (action == "toggle" and w and not hidden(p, w)) then
    if w and not hidden(p, w) then hide(p, w) end
    return
  end
  if w and not hidden(p, w) and (not mon or w.monitor.name == mon.name) then return end
  if not mon then
    for _, candidate in ipairs(hl.get_monitors()) do
      if candidate.focused then mon = candidate; break end
    end
  end
  assert(mon and mon.active_workspace and mon.active_workspace.id > 0, "normal target workspace unavailable")
  save(p)
  if w then show(p, w, mon, mon.active_workspace.id); return end
  local pending = { action = "show", monitor = mon, workspace = mon.active_workspace.id, attempts = 0 }
  p.pending = pending
  launch(p)
  local function poll()
    if p.pending ~= pending then return end
    local found = find(p, true)
    if found then
      p.pending = nil
      finish_launch(p, false)
      if pending.action == "hide" then hide(p, found)
      else show(p, found, pending.monitor, pending.workspace) end
      return
    end
    pending.attempts = pending.attempts + 1
    if pending.attempts >= p.attempts then
      p.pending = nil
      p.previous = nil
      finish_launch(p, true)
      trace(p, "launch_timeout")
      return
    end
    p.wait_timer = later(p, 100, poll)
  end
  p.wait_timer = later(p, 100, poll)
end
for name, p in pairs(panels) do
  M[name] = function(action, monitor)
    return guarded(p, function() run(p, action or "toggle", monitor) end)
  end
end
return M
