#!/usr/bin/env bash
set -euo pipefail
config_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$HOME/.cargo/bin/wl-relabel" -c "$config_dir/wl-relabel-rules.toml" --log -- "$@"
