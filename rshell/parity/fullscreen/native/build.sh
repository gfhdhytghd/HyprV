#!/usr/bin/env bash
# Build the native helpers into $1 (default: ./build next to this script).
set -euo pipefail
N="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-$N/build}"
mkdir -p "$OUT"
cc -O2 -Wall -Wextra -shared -fPIC -o "$OUT/vclock.so" "$N/vclock.c" -ldl
wayland-scanner client-header "$N/wlr-virtual-pointer-unstable-v1.xml" "$OUT/wlr-virtual-pointer-unstable-v1-client-protocol.h"
wayland-scanner private-code "$N/wlr-virtual-pointer-unstable-v1.xml" "$OUT/wlr-virtual-pointer-unstable-v1-protocol.c"
cc -O2 -Wall -Wextra -I"$OUT" -o "$OUT/vpointer" "$N/vpointer.c" "$OUT/wlr-virtual-pointer-unstable-v1-protocol.c" -lwayland-client -lm
XDG=/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml
wayland-scanner client-header "$XDG" "$OUT/xdg-shell-client-protocol.h"
wayland-scanner private-code "$XDG" "$OUT/xdg-shell-protocol.c"
cc -O2 -Wall -Wextra -I"$OUT" -o "$OUT/titlewin" "$N/titlewin.c" "$OUT/xdg-shell-protocol.c" -lwayland-client
echo "built into $OUT"
