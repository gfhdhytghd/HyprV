#!/usr/bin/env bash
set -euo pipefail
exec python3 -B "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/control.py" rollback "$@"
