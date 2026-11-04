-- Pure state-machine regression checks; not a live-compositor acceptance test.
package.path = './?.lua;' .. package.path
local ws = { id = 1, name = '1' }
local mon = { name = 'TEST', width = 1920, height = 1080, scale = 1, transform = 0, x = 0, y = 0, focused = true, active_workspace = ws }
local normal = { address = '0x1', class = 'test', mapped = true, workspace = ws, monitor = mon }
local drop = { address = '0x2', class = 'kitty-dropdown', mapped = true, workspace = { id = -1, name = 'special:dropdown_hidden' }, monitor = mon, pinned = false }
local side = { address = '0x3', class = 'ai-hub-firefox', mapped = true, workspace = { id = -2, name = 'special:sidechat_hidden' }, monitor = mon, pinned = false }
local windows, active, timers, spawns = {normal, drop, side}, normal, {}, 0
-- Live Hyprland rejects calling dispatcher objects directly; mirror that.
local dispatcher_mt = { __call = function() error('dispatcher objects cannot be called directly; use hl.dispatch(dispatcher)') end }
local function dispatch(fn) return function(args) return setmetatable({ run = function() fn(args) end }, dispatcher_mt) end end
hl = {
 get_windows = function() return windows end,
 get_active_window = function() return active end,
 get_active_workspace = function() return ws end,
 get_monitors = function() return {mon} end,
 get_monitor = function(name) return name == mon.name and mon or nil end,
 get_window = function(selector) for _,w in ipairs(windows) do if selector == 'address:' .. w.address then return w end end end,
 timer = function(fn, opts) local t = {fn=fn, ms=opts.timeout}; timers[#timers+1]=t; return t end,
 exec_cmd = function() spawns = spawns + 1 end,
 dispatch = function(d) d.run() end,
 dsp = {window = {}}
}
hl.dsp.focus = dispatch(function(a) assert(a.window.workspace.id > 0, 'focused special workspace'); active=a.window end)
hl.dsp.window.pin = dispatch(function(a) a.window.pinned=not a.window.pinned end)
hl.dsp.window.float = dispatch(function(a) a.window.floating=true end)
hl.dsp.window.resize = dispatch(function(a) a.window.size={a.x,a.y} end)
hl.dsp.window.move = dispatch(function(a)
 if a.workspace then
  assert(not a.window.pinned, 'moved pinned window')
  a.window.workspace = a.workspace:match('^special:') and {id=-1,name=a.workspace} or ws
 else a.window.at={a.x,a.y} end
end)
hl.dsp.window.alter_zorder = dispatch(function(a) assert(a.mode == 'top') end)
hl.dsp.window.close = dispatch(function(a) a.window.mapped=false end)
-- Do not mix simulated events into the user's runtime trace.
local real_open = io.open
io.open = function(path, mode)
 if path == '/tmp/sidechat.trace' or path == '/tmp/dropdown.trace' then
  return {write=function() end, close=function() end}
 end
 return real_open(path, mode)
end
local p = require('hypr.panels')
local function call(name, action, monitor) local ok,err=p[name](action,monitor); assert(ok,err) end
for _,name in ipairs({'dropdown','sidechat'}) do
 for i=1,3 do
  active=normal; call(name,'show'); call(name,'show'); call(name,'reflow'); call(name,'hide'); assert(active==normal)
 end
end
call('dropdown','show'); assert(drop.size[1]==1080 and drop.size[2]==225 and drop.at[1]==420 and drop.at[2]==60)
call('sidechat','show','TEST'); assert(side.size[1]==360 and side.size[2]==1008 and side.at[1]==1550)
call('dropdown','hide'); call('sidechat','hide'); assert(active==normal)
-- Restore-time special-workspace exclusion.
active=normal; call('dropdown','show'); normal.workspace={id=-3,name='special:other'}; call('dropdown','hide'); normal.workspace=ws
active=normal
mon.transform=1; call('sidechat','show','TEST'); assert(side.at[1]==710 and side.size[2]==1848); call('sidechat','hide'); mon.transform=0
call('sidechat','toggle'); call('sidechat','toggle'); assert(side.workspace.id>0); call('sidechat','hide')
local duplicate={address='0x4',class=side.class,mapped=true,workspace=side.workspace,monitor=mon}
windows[#windows+1]=duplicate; call('sidechat','reflow'); assert(not duplicate.mapped)
-- A hide received during launch must win over the later window-open poll.
drop.mapped=false; active=normal; call('dropdown','show'); assert(spawns==1); call('dropdown','hide'); drop.mapped=true
timers[#timers].fn(); assert(drop.workspace.id<0 and active==normal)
assert(not p.sidechat('show', 'UNKNOWN'))
print('PASS: 3x cycles per panel, idempotence, cross-case, special restore guard, geometry/transform, debounce, duplicate cleanup, pending hide, invalid monitor')
