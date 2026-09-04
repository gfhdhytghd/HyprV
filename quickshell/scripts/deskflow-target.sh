#!/usr/bin/env bash
set -euo pipefail

readonly config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/Deskflow"
readonly config_file="$config_dir/deskflow-server.conf"
readonly log_file="${DESKFLOW_LOG_FILE:-$HOME/deskflow.log}"
readonly transform_file="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/monitor-transform"
readonly windows_screen=WindowsVM
readonly macos_screen=linhaikuodeMac-mini.local
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly script_dir
readonly ir_helper="$script_dir/hdmi-switch-ir.py"

current_target() {
  if [[ -r "$config_file" ]] && grep -Eq "right\\([0-9]+,[0-9]+\\) = ${macos_screen}\\(0,100\\)" "$config_file"; then
    printf '%s\n' macos
  else
    printf '%s\n' windows
  fi
}

portal_range() {
  local transform=0
  if [[ -r $transform_file ]]; then
    read -r transform <"$transform_file" || true
  else
    transform=$(hyprctl -j monitors all 2>/dev/null \
      | jq -r '.[] | select(.name == "DP-4") | .transform' \
      | head -n 1)
  fi

  case "$transform" in
    1|3) printf '%s %s\n' 38 71 ;;
    *) printf '%s %s\n' 28 87 ;;
  esac
}

write_config() {
  local target=$1 target_screen temp_file portal_start portal_end
  case "$target" in
    windows) target_screen=$windows_screen ;;
    macos) target_screen=$macos_screen ;;
    *) return 2 ;;
  esac
  read -r portal_start portal_end < <(portal_range)

  mkdir -p "$config_dir"
  temp_file=$(mktemp "$config_dir/.deskflow-server.conf.XXXXXX")
  trap 'rm -f "$temp_file"' RETURN

  sed "s/__TARGET_SCREEN__/$target_screen/g" >"$temp_file" <<'EOF'
section: screens
		WindowsVM:
		halfDuplexCapsLock = false
		halfDuplexNumLock = false
		halfDuplexScrollLock = false
		xtestIsXineramaUnaware = false
		switchCorners = none
		switchCornerSize = 0
	linhaikuodeMac-mini.local:
		halfDuplexCapsLock = false
		halfDuplexNumLock = false
		halfDuplexScrollLock = false
		xtestIsXineramaUnaware = false
		switchCorners = none
		switchCornerSize = 0
	SuperPower:
		halfDuplexCapsLock = false
		halfDuplexNumLock = false
		halfDuplexScrollLock = false
		xtestIsXineramaUnaware = false
		switchCorners = none
		switchCornerSize = 0
end

section: aliases
end

section: links
	__TARGET_SCREEN__:
		left(0,100) = SuperPower(__PORTAL_START__,__PORTAL_END__)
	SuperPower:
		right(__PORTAL_START__,__PORTAL_END__) = __TARGET_SCREEN__(0,100)
end

section: options
	protocol = barrier
	relativeMouseMoves = false
	win32KeepForeground = false
	defaultLockToScreenState = false
	disableLockToScreen = false
	clipboardSharing = true
		clipboardSharingSize = 3072
		switchCorners = none
		switchCornerSize = 0
		keystroke(control+alt+BracketR) = switchToScreen(__TARGET_SCREEN__)
		keystroke(control+alt+BracketL) = switchToScreen(SuperPower)
end
EOF

  sed -i \
    -e "s/__PORTAL_START__/$portal_start/g" \
    -e "s/__PORTAL_END__/$portal_end/g" \
    "$temp_file"

  chmod 600 "$temp_file"
  mv -f "$temp_file" "$config_file"
  trap - RETURN
}

find_deskflow_core_pid() {
  local control_group proc exe cmdline
  local -a candidates=()

  if [[ $(systemctl --user is-active deskflow.service 2>/dev/null || true) == active ]]; then
    control_group=$(systemctl --user show deskflow.service -p ControlGroup --value)

    if [[ -n "$control_group" && -r "/sys/fs/cgroup${control_group}/cgroup.procs" ]]; then
      while IFS= read -r proc; do
        [[ -r "/proc/$proc/exe" ]] || continue
        exe=$(readlink "/proc/$proc/exe" 2>/dev/null || true)
        exe=${exe% (deleted)}
        if [[ ${exe##*/} == deskflow-core ]]; then
          printf '%s\n' "$proc"
          return 0
        fi
      done <"/sys/fs/cgroup${control_group}/cgroup.procs"
    fi
  fi

  while IFS= read -r proc; do
    [[ -r "/proc/$proc/exe" && -r "/proc/$proc/cmdline" ]] || continue
    exe=$(readlink "/proc/$proc/exe" 2>/dev/null || true)
    exe=${exe% (deleted)}
    cmdline=$(tr '\0' ' ' <"/proc/$proc/cmdline")
    [[ ${exe##*/} == deskflow-core && " $cmdline " == *' server '* ]] || continue
    candidates+=("$proc")
  done < <(pgrep -x deskflow-core || true)

  ((${#candidates[@]} == 1)) || return 1
  printf '%s\n' "${candidates[0]}"
}

reload_deskflow() {
  local core_pid log log_offset=0

  if ! core_pid=$(find_deskflow_core_pid); then
    printf 'Deskflow is not service-managed; configuration will apply on its next start\n' >&2
    return 3
  fi

  [[ -f "$log_file" ]] && log_offset=$(stat -c %s "$log_file")
  kill -HUP "$core_pid"

  for _ in {1..20}; do
    log=$(tail -c "+$((log_offset + 1))" "$log_file" 2>/dev/null || true)
    grep -Fq 'reloaded configuration' <<<"$log" && return 0
    kill -0 "$core_pid" 2>/dev/null || break
    sleep 0.1
  done

  printf 'Deskflow did not confirm configuration reload\n' >&2
  return 1
}

switch_display_ir() {
  local target=$1 button
  case "$target" in
    windows) button=input1 ;;
    macos) button=input2 ;;
    *) printf 'Unknown display target: %s\n' "$target" >&2; return 2 ;;
  esac

  [[ -x "$ir_helper" ]] || {
    printf 'HDMI switch IR helper is not executable: %s\n' "$ir_helper" >&2
    return 1
  }
  "$ir_helper" send "$button" >/dev/null
}

queue_display_switch() {
  local target=$1
  local script_path
  script_path=$(readlink -f "$0")
  systemd-run --user --quiet --collect \
    --unit="hyprv-display-switch-$(date +%s%N)" \
    --property=Type=exec \
    bash "$script_path" display-switch "$target"
}

case "${1:-}" in
  status)
    current_target
    ;;
  set)
    requested_target=${2:-}
    case "$requested_target" in
      windows|macos) ;;
      *) printf 'Unknown Deskflow target: %s\n' "$requested_target" >&2; exit 2 ;;
    esac

    backup_file=$(mktemp "$config_dir/.deskflow-server.conf.backup.XXXXXX")
    trap 'rm -f "$backup_file"' EXIT
    cp -a "$config_file" "$backup_file"
    write_config "$requested_target"
    if reload_deskflow; then
      :
    else
      reload_status=$?
      if (( reload_status != 3 )); then
        mv -f "$backup_file" "$config_file"
        reload_deskflow || true
        printf 'Restored the previous Deskflow configuration\n' >&2
        exit 1
      fi
    fi
    rm -f "$backup_file"
    trap - EXIT
    queue_display_switch "$requested_target"
    current_target
    ;;
  reflow)
    requested_target=$(current_target)
    backup_file=$(mktemp "$config_dir/.deskflow-server.conf.backup.XXXXXX")
    trap 'rm -f "$backup_file"' EXIT
    cp -a "$config_file" "$backup_file"
    write_config "$requested_target"
    if reload_deskflow; then
      :
    else
      reload_status=$?
      if (( reload_status != 3 )); then
        mv -f "$backup_file" "$config_file"
        reload_deskflow || true
        printf 'Restored the previous Deskflow configuration\n' >&2
        exit 1
      fi
    fi
    rm -f "$backup_file"
    trap - EXIT
    ;;
  display-switch)
    case "${2:-}" in
      windows|macos)
        if ! switch_display_ir "$2"; then
          command -v notify-send >/dev/null 2>&1 \
            && notify-send "Win/macOS switch" "HDMI switch to $2 failed" || true
          exit 1
        fi
        ;;
      *) printf 'Unknown display target: %s\n' "${2:-}" >&2; exit 2 ;;
    esac
    ;;
  *)
    printf 'Usage: %s {status|set windows|set macos|reflow|display-switch macos|display-switch windows}\n' "$0" >&2
    exit 2
    ;;
esac
