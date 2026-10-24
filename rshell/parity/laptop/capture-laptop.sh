#!/usr/bin/env bash
# Render the laptop (Dell XPS) Quickshell reference: origin/laptop pinned at
# LAPTOP_REF, exported read-only to a temp dir, rendered by the shared harness
# (../capture.sh) with the laptop fixtures. See docs/verify/laptop-qa-plan.md §2.
#
#   parity/laptop/capture-laptop.sh                         # every laptop component
#   parity/laptop/capture-laptop.sh laptop-bar              # one component
#   parity/laptop/capture-laptop.sh laptop-bar lap-compact-idle
#
# Outputs: parity/laptop/reference/<component>/ (same contract as parity/reference).
set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="${HYPRV_REPO:-$HOME/.config/HyprV}"
ref="${LAPTOP_REF:-24972f7}"

avail_gb=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
(( avail_gb >= 30 )) || { echo "capture-laptop.sh: only ${avail_gb} GB free (< 30 GB floor)" >&2; exit 1; }

export_dir="$(mktemp -d "${TMPDIR:-/tmp}/hyprv-laptop-src.XXXXXX")"
trap 'rm -rf -- "${export_dir:?}"' EXIT
git -C "$repo" archive "$ref" quickshell | tar -x -C "$export_dir"
commit="$(git -C "$repo" rev-parse --short "$ref^{commit}")"

python3 -I "$here/gen-laptop-fixtures.py" "$here/fixtures" >/dev/null

# Build the scene inside the scale-N window (parity/host/main.cpp): eagerly created items
# (laptop CP page, strip icons) load at the output DPR like Quickshell on the XPS.
PARITY_SCENE_IN_WINDOW=1 \
QUICKSHELL_SRC="$export_dir/quickshell" \
PARITY_FIXTURES="$here/fixtures" PARITY_OUT="${PARITY_OUT:-$here/reference}" \
PARITY_SCALES="${PARITY_SCALES:-2 1}" \
PARITY_SOURCE_REF="origin/laptop@$commit" PARITY_PROFILE=laptop \
    "$here/../capture.sh" "$@"
