#!/bin/bash

current=$(powerprofilesctl get 2>/dev/null)

# 计算下一个 profile
case "$current" in
  performance)
    next="balanced"
    icon="⚡"
    ;;
  balanced)
    next="power-saver"
    icon="⚖️"
    ;;
  power-saver)
    next="performance"
    icon="🔋"
    ;;
  *)
    next="balanced"
    icon="❓"
    ;;
esac

# 如果是点击触发，就切换
if [[ "$1" == "toggle" ]]; then
  powerprofilesctl set "$next"
  exit 0
fi

# Waybar 显示
echo "$icon"
