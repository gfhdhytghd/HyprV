local M = {}

function M.setup_package_path()
  local root = (os.getenv("HOME") or "/home/wilf") .. "/.config/HyprV"
  package.path = root .. "/?.lua;" .. root .. "/?/init.lua;" .. package.path
end

function M.exec_once(cmd)
  hl.on("hyprland.start", function()
    hl.exec_cmd(cmd)
  end)
end

function M.exec_once_raw(cmd)
  hl.on("hyprland.start", function()
    hl.exec_cmd(cmd)
  end)
end

function M.bind_exec(keys, cmd, opts)
  return hl.bind(keys, hl.dsp.exec_cmd(cmd), opts)
end

function M.bind_exec_raw(keys, cmd, opts)
  return hl.bind(keys, hl.dsp.exec_raw(cmd), opts)
end

function M.bind_plugin_gap(keys, dispatcher, opts)
  return hl.bind(keys, function()
    hl.notification.create({
      text = dispatcher .. " needs Lua dispatcher support in the plugin",
      timeout = 5000,
      icon = "warning",
    })
  end, opts)
end

function M.plugin_fn(namespace, name)
  local plugin_ns = hl.plugin and hl.plugin[namespace]
  local fn = plugin_ns and plugin_ns[name]
  if type(fn) == "function" then
    return fn
  end
  return nil
end

function M.bind_plugin_fn(keys, namespace, name, args, opts)
  return hl.bind(keys, function()
    local fn = M.plugin_fn(namespace, name)
    if fn then
      if args == nil then
        fn()
      else
        fn(args)
      end
      return
    end

    hl.notification.create({
      text = "hl.plugin." .. namespace .. "." .. name .. " is not available",
      timeout = 5000,
      icon = "warning",
    })
  end, opts)
end

function M.bind_plugin_fn_or_exec(keys, namespace, name, cmd, args, opts)
  return hl.bind(keys, function()
    local fn = M.plugin_fn(namespace, name)
    if fn then
      if args == nil then
        fn()
      else
        fn(args)
      end
      return
    end

    hl.exec_cmd(cmd)
  end, opts)
end

function M.call_plugin_fn(namespace, name, ...)
  local fn = M.plugin_fn(namespace, name)
  if fn then
    fn(...)
    return true
  end
  return false
end

function M.plugin_config_if_available(required_key, config)
  if hl.get_config(required_key) == nil then
    return false
  end

  hl.config(config)
  return true
end

return M
