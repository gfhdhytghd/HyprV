#!/usr/bin/env bash
# Rebuild everything and reproduce the residual-diff tables in docs/spec/text-rendering.md.
# Read-only towards the session: Qt renders through an *unmapped* Wayland window
# (TEXTREF_NO_SHOW=1), which gives the same RHI/OpenGL path and output scale as quickshell
# without showing anything.
set -euo pipefail
cd "$(dirname "$0")"
root=$(cd ../.. && pwd)
cmake -S qt-ref -B qt-ref/build -DCMAKE_BUILD_TYPE=Release -DQT_NO_PRIVATE_MODULE_WARNING=ON >/dev/null
make -C qt-ref/build -j"$(nproc)" >/dev/null
(cd skia-text && CARGO_TARGET_DIR="$root/target" cargo build -q)
cmp="$root/target/debug/textcmp"
mkdir -p out
for th in light dark; do
  TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland ./qt-ref/build/textref "scene-$th.qml" "out/qt-$th@2.png" > "out/qt-$th@2.txt"
done
# dpr 1.5: the screen is scale 2, QT_SCALE_FACTOR=0.75 gives an effective 1.5
TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland QT_SCALE_FACTOR=0.75 ./qt-ref/build/textref scene-light.qml out/qt-light@1.5.png > out/qt-light@1.5.txt
# software backend (what a QT_QPA_PLATFORM=offscreen harness would capture)
QT_QPA_PLATFORM=offscreen QT_SCALE_FACTOR=2 ./qt-ref/build/textref scene-light.qml out/sw-light@2.png > out/sw-light@2.txt
"$cmp" scene.json out/qt-light@2.txt out/qt-light@2.png out/cmp-light 2
"$cmp" scene.json out/qt-dark@2.txt out/qt-dark@2.png out/cmp-dark 2 | sed -n '/^sample/,$p'
"$cmp" scene.json out/qt-light@1.5.txt out/qt-light@1.5.png out/cmp-light-1.5 1.5 | sed -n '/^sample/,$p'
TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland ./qt-ref/build/textref --dumpdf "JetBrainsMono Nerd Font" 700 "12" out/df-jb.json
TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland ./qt-ref/build/textref --dumpdf "JetBrainsMono Nerd Font" 400 "正" out/df-cn.json
"$cmp" --dfcheck out/df-jb.json "JetBrainsMono Nerd Font" 700
"$cmp" --dfcheck out/df-cn.json "HarmonyOS Sans SC" 400
