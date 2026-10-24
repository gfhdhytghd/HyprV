import QtQuick
Item {
  width: 320; height: 250
  Text { objectName: "clock"; x: 10; y: 8; text: "12:34"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "cpu"; x: 80; y: 8; text: " 7%"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "arch"; x: 160; y: 7; text: ""; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 17; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "vol"; x: 190; y: 8; text: ""; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "title"; x: 10.4; y: 40; text: "Firefox — Quickshell spec"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "frac"; x: 200.9; y: 121.3; text: "12:34"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "charge"; x: 10; y: 72; text: "正在充电"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 12; font.weight: 600; renderType: Text.NativeRendering }
  Text { objectName: "power"; x: 110; y: 72; text: "当前净功率"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 13; font.weight: 600; renderType: Text.NativeRendering }
  Text { objectName: "estimate"; x: 10; y: 96; text: "预计剩余使用时间 3小时5分钟"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 13; font.weight: 700; renderType: Text.NativeRendering }
  Text { objectName: "menu"; x: 10; y: 124; text: "Open Settings…"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 14; font.weight: 400; renderType: Text.NativeRendering }
  Text { objectName: "cal_dt"; x: 10; y: 156; text: "12:34"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.QtRendering }
  Text { objectName: "cal_cn"; x: 80; y: 156; text: "正在连接日历…"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 12; font.weight: 400; renderType: Text.QtRendering }
  Text { objectName: "cal_day"; x: 10; y: 184; text: "一 二 三 四 五"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 11; font.weight: 400; renderType: Text.QtRendering }
  Text { objectName: "cal_num"; x: 200; y: 184; text: "23"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 14; font.weight: 400; renderType: Text.QtRendering }
  Text { objectName: "fallback"; x: 240; y: 184; text: "D"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 10; font.weight: 400; renderType: Text.QtRendering }
  Text { objectName: "spaced"; x: 10; y: 200; text: "MIHOMO ON"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 9; font.weight: 700; renderType: Text.QtRendering; font.letterSpacing: 3 }
  Text { objectName: "spacedn"; x: 120; y: 196; text: "12:34"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering; font.letterSpacing: 3 }
  Text { objectName: "elide"; x: 10.4; y: 226; text: "Firefox — a very long window title that must elide"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 16; font.weight: 700; renderType: Text.NativeRendering; width: 150.5; elide: Text.ElideRight }
  Text { objectName: "elidecn"; x: 180; y: 228; text: "已接通电源，当前未充电"; color: "#2b2b2c"; font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 12; font.weight: 600; renderType: Text.NativeRendering; width: 100; elide: Text.ElideRight }
}