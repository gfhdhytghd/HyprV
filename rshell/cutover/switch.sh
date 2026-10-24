#!/usr/bin/env bash
set -euo pipefail
if [ "$(id -u)" = 0 ]; then
  echo "switch.sh: refusing to run as root (user-writable). Root phase: sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py switch --scope system --apply" >&2
  exit 1
fi
exec python3 -B "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/control.py" switch "$@"
