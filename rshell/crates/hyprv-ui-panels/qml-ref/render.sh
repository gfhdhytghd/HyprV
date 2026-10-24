#!/usr/bin/env bash
# Render every scene with Qt's RHI renderer at dpr 2 through an UNMAPPED Wayland window
# (nothing is shown; same path and scale as quickshell). Uses experiments/text/qt-ref.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
textref="$root/experiments/text/qt-ref/build/textref"
[ -x "$textref" ] || { cmake -S "$root/experiments/text/qt-ref" -B "$root/experiments/text/qt-ref/build" -DCMAKE_BUILD_TYPE=Release >/dev/null && make -C "$root/experiments/text/qt-ref/build" >/dev/null; }
(cd "$here/.." && cargo run -q --no-default-features --example export_fixtures -- qml-ref/fixtures.json)
python3 "$here/gen.py"
mkdir -p "$here/out"
for s in "$here"/scenes/*.qml; do
  n=$(basename "$s" .qml)
  QML_IMPORT_PATH="$here/mocks" TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland "$textref" "$s" "$here/out/$n@2.png" > "$here/out/$n@2.txt" 2>&1 || { echo "FAILED $n"; tail -5 "$here/out/$n@2.txt"; }
done
ls "$here/out" | grep -c png
