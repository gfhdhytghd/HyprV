#!/bin/sh
set -eu
ui_crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ui_crate_dir/../.."
ui_cargo_command=${1:-test}
if [ "$#" -gt 0 ]; then shift; fi
cargo "$ui_cargo_command" -p hyprv-ui "$@"
