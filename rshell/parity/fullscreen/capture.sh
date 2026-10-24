#!/usr/bin/env bash
# Full-screen references: the ORIGINAL quickshell config in an isolated nested compositor.
# See README.md. Never touches the live session:
#  - private XDG_RUNTIME_DIR under /tmp/claude-1000 (live sockets live in /run/user/$UID);
#  - everything runs under one private dbus-run-session (no service activation) that is
#    also the system bus, so nothing reaches the real logind/BlueZ/NM/UPower/PipeWire;
#  - nested Hyprland: LIBSEAT_BACKEND=seatd (no seatd socket -> never asks logind),
#    AQ_DRM_DEVICES=<nonexistent>, Xwayland disabled;
#  - hyprctl only through run.py's Nested.hyprctl (signature inline, asserted != live);
#  - every rm is guarded to the private runtime dir under /tmp/claude-1000.
#
#   parity/fullscreen/capture.sh                 # every fixture, both themes
#   parity/fullscreen/capture.sh idle calendar   # some fixtures
# env: FS_THEMES="light dark", FS_OUT=<dir> (default reference/), QUICKSHELL_SRC=<dir>,
#      FS_SKIP_HOST=1 (skip the parity-host pass that supplies distance_field_text)
set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
src="${QUICKSHELL_SRC:-$HOME/.config/HyprV/quickshell}"
out="${FS_OUT:-$here/reference}"
themes="${FS_THEMES:-light dark}"
base=/tmp/claude-1000
live_rt="/run/user/$(id -u)"

die() { printf 'capture.sh: %s\n' "$*" >&2; exit 1; }
safe_rm() {
    local t="$1"
    [[ -n "$t" && "$t" == "$base"/* && "$t" != *..* ]] || { echo "refusing rm: $t" >&2; return 1; }
    rm -rf -- "$t"
}

for tool in kwin_wayland Hyprland grim quickshell dbus-run-session cc wayland-scanner; do
    command -v "$tool" >/dev/null || die "$tool is required"
done
[[ -f "$src/shell.qml" ]] || die "quickshell source not found at $src"
[[ -s "$here/fixtures/outputs.json" ]] || die "run tools/discover-outputs.sh first"

mkdir -p "$base"
# Short path: Hyprland's socket $RT/hypr/<61-char signature>/.socket2.sock must fit 108 bytes.
RT="$(mktemp -d "$base/fs.XXXX")"
chmod 700 "$RT"
[[ "$RT" != "$live_rt"* ]] || die "runtime dir collides with the live one"
trap 'safe_rm "$RT"' EXIT

bash "$here/native/build.sh" "$RT/build" >/dev/null
# Live signatures (directory listing only) for run.py's inequality assertion.
live_sigs="$(ls -1 "$live_rt/hypr" 2>/dev/null | tr '\n' ' ' || true)"
[[ -n "$live_sigs" ]] || live_sigs="none"

cat >"$RT/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:tmpdir=$RT</listen>
  <limit name="max_match_rules_per_connection">100000</limit>
  <limit name="max_connections_per_user">10000</limit>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF

mkdir -p "$out"
status=0
env -i HOME="$RT/home-unused" REAL_HOME="$HOME" PATH=/usr/bin LANG=C.UTF-8 XDG_RUNTIME_DIR="$RT" \
    ${TZ:+TZ="$TZ"} LIVE_SIGS="$live_sigs" \
    dbus-run-session --config-file="$RT/bus.conf" -- \
    sh -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"; exec "$@"' sh \
    python3 -I "$here/tools/run.py" --source "$src" --out "$out" --build "$RT/build" --themes "$themes" "$@" \
    || status=$?

python3 -I "$here/tools/manifest.py" --source "$src" --out "$out" ${FS_SKIP_HOST:+--skip-host} || status=$?
exit "$status"
