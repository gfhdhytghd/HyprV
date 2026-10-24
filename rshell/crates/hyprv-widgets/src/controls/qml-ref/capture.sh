#!/usr/bin/env bash
# Capture widget-level QML references for hyprv-widgets controls/viz.
# Reuses the parity harness host (rshell/parity/build/parity-host, built by
# parity/capture.sh) read-only; scenes/fixtures are generated into a temp dir.
#
#   capture.sh                  # every fixture in fixtures/
#   capture.sh toggle-active    # one fixture (file stem)
# Env: PARITY_SCALES="1 2" PARITY_THEMES="light dark" QUICKSHELL_SRC=<dir>
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
rshell="$(cd -- "$here/../../../../.." && pwd)"
src="${QUICKSHELL_SRC:-$HOME/.config/HyprV/quickshell}"
host="${PARITY_HOST:-$rshell/parity/build/parity-host}"
assets="$rshell/parity/fixtures/assets"
out="$here/reference"
themes="${PARITY_THEMES:-light dark}"
scales="${PARITY_SCALES:-1 2}"

die() { printf 'qml-ref/capture.sh: %s\n' "$*" >&2; exit 1; }
[[ -x "$host" ]] || die "parity host missing: run rshell/parity/capture.sh once to build it"
command -v weston >/dev/null || die "weston is required"

work="$(mktemp -d "${TMPDIR:-/tmp}/hyprv-widget-ref.XXXXXX")"
chmod 700 "$work"
socket="widget-ref-$$"
weston --backend=headless --renderer=gl --shell=desktop --no-config --idle-time=0 \
    --width=4096 --height=2048 --socket="$socket" >"$work/weston.log" 2>&1 &
weston_pid=$!
cleanup() { kill "$weston_pid" 2>/dev/null || true; wait "$weston_pid" 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
for _ in $(seq 100); do [[ -S "${XDG_RUNTIME_DIR}/$socket" ]] && break; sleep 0.05; done
[[ -S "${XDG_RUNTIME_DIR}/$socket" ]] || { cat "$work/weston.log" >&2; die "headless weston did not start"; }

if [[ $# -ge 1 ]]; then fixtures=(); for n in "$@"; do fixtures+=("$here/fixtures/$n.json"); done; else fixtures=("$here"/fixtures/*.json); fi
mkdir -p "$out"
for fx in "${fixtures[@]}"; do
    [[ -f "$fx" ]] || die "no fixture $fx"
    python3 -I "$here/gen.py" "$src" "$fx" "$work"
    name="$(basename "$fx" .json)"
    component="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["component"])' "$fx")"
    for theme in $themes; do
        for scale in $scales; do
            env -i HOME="$HOME" PATH="$PATH" XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
                LANG="${LANG:-C.UTF-8}" TZ="${TZ:-}" \
                WAYLAND_DISPLAY="$socket" QT_QPA_PLATFORM=wayland \
                QT_WAYLAND_DISABLE_WINDOWDECORATION=1 QT_QPA_PLATFORMTHEME=qt6ct \
                QSG_RENDER_LOOP=basic QSG_RHI_BACKEND=opengl PARITY_REQUIRE_RHI=1 \
                "$host" --fixture "$work/fixtures/$component/$name.json" --scenes "$work/scenes" \
                --source "$src" --assets "$assets" --out "$out" --theme "$theme" --scale "$scale" \
                || die "host failed for $name [$theme @$scale]"
        done
    done
done
rm -f "$out"/.*.json
