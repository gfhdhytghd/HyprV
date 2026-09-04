#!/usr/bin/env bash
set -euo pipefail

readonly main_output="${HYPRV_ROTATE_OUTPUT:-DP-4}"
readonly state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv"
readonly state_file="$state_dir/monitor-transform"
readonly lock_file="$state_dir/monitor-transform.lock"
readonly deskflow_script="$HOME/.config/HyprV/quickshell/scripts/deskflow-target.sh"
readonly dropdown_script="$HOME/.config/HyprV/hypr/scripts/dropdown.sh"
readonly sidechat_script="$HOME/.config/HyprV/hypr/scripts/sidechat.sh"

command -v jq >/dev/null || {
  printf 'rotate-screen: jq is required\n' >&2
  exit 1
}

mkdir -p "$state_dir"
chmod 700 "$state_dir"
exec 9>"$lock_file"
flock 9

monitors_json=$(hyprctl -j monitors all)
main_json=$(jq -cer --arg output "$main_output" '.[] | select(.name == $output and (.disabled | not))' <<<"$monitors_json") || {
  printf 'rotate-screen: active output %s was not found\n' "$main_output" >&2
  exit 1
}

old_transform=$(jq -r '.transform' <<<"$main_json")
[[ $old_transform =~ ^[0-3]$ ]] || {
  printf 'rotate-screen: unsupported transform %s on %s\n' "$old_transform" "$main_output" >&2
  exit 1
}
if (( old_transform == 0 )); then
  new_transform=3
else
  new_transform=0
fi

read -r old_main_width old_main_height < <(
  jq -r '
    if (.transform % 2) == 0
    then [(.width / .scale), (.height / .scale)]
    else [(.height / .scale), (.width / .scale)]
    end | map(round) | @tsv
  ' <<<"$main_json"
)
if (( new_transform % 2 == 0 )); then
  new_main_width=$(jq -r '(.width / .scale) | round' <<<"$main_json")
  new_main_height=$(jq -r '(.height / .scale) | round' <<<"$main_json")
else
  new_main_width=$(jq -r '(.height / .scale) | round' <<<"$main_json")
  new_main_height=$(jq -r '(.width / .scale) | round' <<<"$main_json")
fi

lua_quote() {
  jq -Rrn --arg value "$1" '$value | @json'
}

monitor_call() {
  local name=$1 width=$2 height=$3 refresh=$4 x=$5 y=$6 scale=$7 transform=$8
  local quoted_name quoted_mode mode extra=''
  printf -v mode '%sx%s@%.2f' "$width" "$height" "$refresh"
  quoted_name=$(lua_quote "$name")
  quoted_mode=$(lua_quote "$mode")
  if [[ $name == "$main_output" ]]; then
    extra=', bitdepth = 10, cm = "srgb"'
  fi
  printf 'hl.monitor({ output = %s, mode = %s, position = "%sx%s", scale = "%s", transform = %s%s });' \
    "$quoted_name" "$quoted_mode" "$x" "$y" "$scale" "$transform" "$extra"
}

build_layout() {
  local requested_transform=$1 shift_x=$2 shift_y=$3
  local name width height refresh x y scale transform logical_width new_x new_y
  local code=''

  while IFS=$'\t' read -r name width height refresh x y scale transform; do
    if [[ $name == "$main_output" ]]; then
      code+=$(monitor_call "$name" "$width" "$height" "$refresh" "$x" "$y" "$scale" "$requested_transform")
      continue
    fi

    logical_width=$(jq -nr \
      --argjson width "$width" --argjson height "$height" \
      --argjson scale "$scale" --argjson transform "$transform" \
      'if ($transform % 2) == 0 then ($width / $scale) else ($height / $scale) end | round')

    if (( x >= old_main_width )); then
      new_x=$((x + shift_x))
    elif (( x + logical_width <= 0 )); then
      new_x=$x
    else
      new_x=$((x + shift_x / 2))
    fi
    new_y=$((y + shift_y))
    code+=$(monitor_call "$name" "$width" "$height" "$refresh" "$new_x" "$new_y" "$scale" "$transform")
  done < <(jq -r '.[] | select(.disabled | not) | [.name, .width, .height, .refreshRate, .x, .y, .scale, .transform] | @tsv' <<<"$monitors_json")

  printf '%s\n' "$code"
}

new_layout=$(build_layout "$new_transform" "$((new_main_width - old_main_width))" "$(((new_main_height - old_main_height) / 2))")
old_layout=$(build_layout "$old_transform" 0 0)

if [[ ${1:-} == --dry-run ]]; then
  if (( new_transform % 2 == 0 )); then
    deskflow_range='28-87%'
  else
    deskflow_range='38-71%'
  fi
  printf '%s: transform %s -> %s, logical %sx%s -> %sx%s, external-y shift %+d, Deskflow %s\n' \
    "$main_output" "$old_transform" "$new_transform" \
    "$old_main_width" "$old_main_height" "$new_main_width" "$new_main_height" \
    "$(((new_main_height - old_main_height) / 2))" "$deskflow_range"
  exit 0
fi

state_existed=false
old_state=''
if [[ -f $state_file ]]; then
  state_existed=true
  old_state=$(<"$state_file")
fi

write_state() {
  local value=$1 temp_file
  temp_file=$(mktemp "$state_dir/.monitor-transform.XXXXXX")
  printf '%s\n' "$value" >"$temp_file"
  chmod 600 "$temp_file"
  mv -f "$temp_file" "$state_file"
}

restore_state() {
  if $state_existed; then
    write_state "$old_state"
  else
    rm -f -- "$state_file"
  fi
}

rollback() {
  restore_state
  hyprctl eval "$old_layout" >/dev/null 2>&1 || true
}

wait_for_transform() {
  local expected=$1 actual
  for _ in {1..50}; do
    actual=$(hyprctl -j monitors all 2>/dev/null \
      | jq -r --arg output "$main_output" 'first(.[] | select(.name == $output) | .transform) // empty')
    [[ $actual == "$expected" ]] && return 0
    sleep 0.02
  done
  return 1
}

write_state "$new_transform"
if ! hyprctl eval "$new_layout" >/dev/null; then
  rollback
  printf 'rotate-screen: Hyprland rejected the rotated monitor layout\n' >&2
  exit 1
fi
if ! wait_for_transform "$new_transform"; then
  rollback
  printf 'rotate-screen: timed out waiting for transform %s; restored the previous monitor layout\n' "$new_transform" >&2
  exit 1
fi

if [[ -x $deskflow_script ]] && ! "$deskflow_script" reflow >/dev/null; then
  rollback
  printf 'rotate-screen: Deskflow topology update failed; restored the previous monitor layout\n' >&2
  exit 1
fi

[[ -x $dropdown_script ]] && "$dropdown_script" reflow || true
[[ -x $sidechat_script ]] && "$sidechat_script" reflow || true
