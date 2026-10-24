#!/usr/bin/env bash
set -euo pipefail
if [ "$(id -u)" = 0 ]; then
  echo "rollback.sh: refusing to run as root (user-writable). Root phase: sudo /usr/bin/python3 -I -B /usr/local/lib/hyprv-cutover/current/cutover/control.py rollback --scope system --apply" >&2
  exit 1
fi
exec python3 -B "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/control.py" rollback "$@"
