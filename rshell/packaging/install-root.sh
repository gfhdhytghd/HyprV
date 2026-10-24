#!/usr/bin/env bash
# Prints the root install procedure. Never run this (user-writable) file as root:
# the root step executes only the root-owned, staged and verified release copy.
set -euo pipefail
if [ "$(id -u)" = 0 ]; then
  echo "install-root.sh: refusing to run as root; it only prints the staged procedure. Run it as the desktop user." >&2
  exit 1
fi
exec python3 -B "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/install.py" root --dry-run
