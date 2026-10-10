#!/usr/bin/env bash
set -euo pipefail

readonly unit=windows-autoshop.service

case "${1:-}" in
  status)
    if systemctl is-active --quiet "$unit"; then
      printf '%s\n' running
    else
      printf '%s\n' stopped
    fi
    ;;
  start)
    exec pkexec /usr/bin/systemctl start "$unit"
    ;;
  hibernate)
    exec pkexec /usr/bin/systemctl stop "$unit"
    ;;
  *)
    printf 'Usage: %s {status|start|hibernate}\n' "$0" >&2
    exit 2
    ;;
esac
