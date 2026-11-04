#!/bin/sh
# Compatibility entry point for external callers (including screen rotation).
# Validate before interpolating into Lua; internal callers use hypr.panels directly.
action=${1:-toggle}
monitor=${2:-}
case "${3:-}" in "") warp=false ;; --no-cursor-warp) warp=true ;; *) echo "Unknown option: $3" >&2; exit 2 ;; esac
case "$action" in toggle|show|hide|reflow) ;; *) echo "Invalid panel action: $action" >&2; exit 2 ;; esac
case "$monitor" in *[!a-zA-Z0-9_.:-]*) echo "Invalid monitor name" >&2; exit 2 ;; esac
exec hyprctl dispatch "function() assert(require('hypr.panels').dropdown('$action', '$monitor', { no_warp = $warp })) end"
