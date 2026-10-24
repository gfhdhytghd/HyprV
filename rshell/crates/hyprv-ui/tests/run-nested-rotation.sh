#!/usr/bin/env bash
# Runs tests/nested_rotation.rs inside a bwrap sandbox:
# - / and /sys are read-only; /tmp and /run are private;
# - only the GPU render node and the NVIDIA control nodes are exposed (no DRM primary
#   node, no input devices), so the nested compositor can never touch the real
#   display, seat or session;
# - own PID namespace; a clean environment, so user graphics overrides such as
#   AQ_DRM_DEVICES or __EGL_VENDOR_LIBRARY_FILENAMES never reach the nested stack.
# Usage: crates/hyprv-ui/tests/run-nested-rotation.sh [report-file]
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$root"
bin=$(cargo test -q -p hyprv-ui --test nested_rotation --no-run --message-format=json 2>/dev/null \
  | python3 -c 'import json,sys;[print(m["executable"]) for m in map(json.loads,sys.stdin) if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["name"]=="nested_rotation"]' | tail -1)
report=${1:-}
devs=()
for d in /dev/dri/renderD* /dev/nvidiactl /dev/nvidia0 /dev/nvidia-modeset /dev/nvidia-uvm; do
  [ -e "$d" ] && devs+=(--dev-bind "$d" "$d")
done
out=()
envs=()
[ -n "$report" ] && { mkdir -p "$(dirname "$report")"; : > "$report"; out=(--bind "$(realpath "$report")" /tmp/rotation-report); envs=(--setenv HYPRV_ROTATION_REPORT /tmp/rotation-report); }
exec bwrap --ro-bind / / --tmpfs /tmp --tmpfs /run --ro-bind /run/udev /run/udev \
  --dev /dev "${devs[@]}" --tmpfs /dev/shm --proc /proc --ro-bind /sys /sys \
  "${out[@]}" --unshare-pid --unshare-ipc --die-with-parent --new-session \
  --clearenv --setenv PATH /usr/bin --setenv HOME /tmp --setenv XDG_RUNTIME_DIR /run --setenv LANG C.UTF-8 \
  --setenv RUST_BACKTRACE "${RUST_BACKTRACE:-0}" "${envs[@]}" \
  --setenv HYPRV_UI_PROFILE 1 \
  -- "$bin" --ignored --nocapture --test-threads=1
