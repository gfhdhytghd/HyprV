#!/usr/bin/env bash

set -euo pipefail

STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hyprv/quickshell/demo-mode"
ACTIVE_FILE="$STATE_DIR/active"
SESSION_FILE="$STATE_DIR/session"
SOURCE_FILE="$STATE_DIR/source"
HEADLESS_FILE="$STATE_DIR/headless"
CREATED_HEADLESS_FILE="$STATE_DIR/created-headless"
PREVENT_SLEEP_FILE="$STATE_DIR/prevent-sleep-was-enabled"
PREVENT_SLEEP_SCRIPT="${HOME}/.config/HyprV/quickshell/scripts/prevent-sleep.sh"

notify() {
    command -v notify-send >/dev/null 2>&1 || return 0
    notify-send "HyprV Demo mode" "$1" >/dev/null 2>&1 || true
}

current_session() {
    printf '%s' "${HYPRLAND_INSTANCE_SIGNATURE:-unknown}"
}

state_belongs_to_current_session() {
    [[ -f "$ACTIVE_FILE" && -f "$SESSION_FILE" ]] || return 1
    [[ "$(<"$SESSION_FILE")" == "$(current_session)" ]]
}

cleanup_stale_state() {
    [[ -e "$ACTIVE_FILE" ]] || return 0
    state_belongs_to_current_session && return 0
    rm -rf -- "$STATE_DIR"
}

status() {
    cleanup_stale_state
    if state_belongs_to_current_session; then
        printf 'enabled=true\n'
        printf 'source=%s\n' "$(<"$SOURCE_FILE")"
        printf 'headless=%s\n' "$(<"$HEADLESS_FILE")"
        return 0
    fi
    printf 'enabled=false\n'
    printf 'source=\n'
    printf 'headless=\n'
    return 1
}

monitor_eval() {
    local output="$1"
    local mode="$2"
    local position="$3"
    local transform="$4"
    local mirror="${5:-}"
    local expression result

    expression="hl.monitor({ output = '${output}', mode = '${mode}', position = '${position}', scale = '1', transform = ${transform}"
    if [[ -n "$mirror" ]]; then
        expression+=", mirror = '${mirror}'"
    fi
    expression+=" })"

    result="$(hyprctl eval "$expression" 2>&1)" || {
        printf '%s\n' "$result" >&2
        return 1
    }
    if grep -Eqi '(^|[^a-z])(error|invalid|failed|cannot)([^a-z]|$)' <<<"$result"; then
        printf '%s\n' "$result" >&2
        return 1
    fi
}

touch_eval() {
    local name="$1"
    local result

    result="$(hyprctl eval "hl.device({ name = '${name}', transform = 0 })" 2>&1)" || {
        printf '%s\n' "$result" >&2
        return 1
    }
    if grep -Eqi '(^|[^a-z])(error|invalid|failed|cannot)([^a-z]|$)' <<<"$result"; then
        printf '%s\n' "$result" >&2
        return 1
    fi
}

portrait_mode_for() {
    local monitor_json="$1"
    if jq -e '.availableModes[]? | select(test("^1080x1920@(59\\.|60\\.)"))' \
        <<<"$monitor_json" >/dev/null; then
        printf '1080x1920@60|0\n'
    else
        # Most panels advertise their physical landscape mode and are made
        # logically 1080x1920 with a compositor transform.
        printf '1920x1080@60|1\n'
    fi
}

find_external_outputs() {
    jq -r '
        .[]
        | select(.disabled != true)
        | select(
            (.name | test("^(eDP|LVDS|DSI|HEADLESS|WAYLAND|WL-)"; "i"))
            | not
        )
        | .name
    '
}

find_headless_output() {
    jq -r '[.[] | select(.disabled != true and (.name | startswith("HEADLESS-"))) | .name][0] // empty'
}

external_position() {
    local monitors_json="$1"
    jq -r '
        [
            .[]
            | select(.disabled != true)
            | select(.name | test("^(eDP|LVDS|DSI)"; "i"))
            | {
                right: (
                    .x + (
                        if ((.transform // 0) % 2) == 1
                        then (.height / .scale)
                        else (.width / .scale)
                        end
                    )
                ),
                top: .y
            }
        ]
        | if length == 0
          then "auto-right"
          else (max_by(.right) | ((.right | floor | tostring) + "x" + (.top | floor | tostring)))
          end
    ' <<<"$monitors_json"
}

prevent_sleep_enabled() {
    [[ -x "$PREVENT_SLEEP_SCRIPT" ]] || return 1
    "$PREVENT_SLEEP_SCRIPT" status 2>/dev/null | grep -q '^enabled=true$'
}

rollback_enable() {
    local created_headless="${1:-}"
    hyprctl reload >/dev/null 2>&1 || true
    if [[ -n "$created_headless" ]]; then
        hyprctl output remove "$created_headless" >/dev/null 2>&1 || true
    fi
    if [[ -f "$PREVENT_SLEEP_FILE" && "$(<"$PREVENT_SLEEP_FILE")" == "false" ]]; then
        "$PREVENT_SLEEP_SCRIPT" disable >/dev/null 2>&1 || true
    fi
    rm -rf -- "$STATE_DIR"
}

enable() {
    cleanup_stale_state
    if state_belongs_to_current_session; then
        status
        return 0
    fi

    command -v hyprctl >/dev/null 2>&1 || {
        printf 'hyprctl is required\n' >&2
        return 1
    }
    command -v jq >/dev/null 2>&1 || {
        printf 'jq is required\n' >&2
        return 1
    }

    local monitors_json external_outputs source source_json source_position
    local headless created_headless="" source_mode source_transform
    monitors_json="$(hyprctl monitors all -j)"
    mapfile -t external_outputs < <(find_external_outputs <<<"$monitors_json")
    if (( ${#external_outputs[@]} == 0 )); then
        notify "没有检测到外接显示器，Demo mode 未启动"
        printf 'no external display is connected\n' >&2
        return 1
    fi

    source="${external_outputs[0]}"
    source_json="$(jq -c --arg name "$source" '.[] | select(.name == $name)' <<<"$monitors_json")"
    IFS='|' read -r source_mode source_transform < <(portrait_mode_for "$source_json")
    source_position="$(external_position "$monitors_json")"
    headless="$(find_headless_output <<<"$monitors_json")"

    mkdir -p "$STATE_DIR"
    printf '%s\n' "$(current_session)" > "$SESSION_FILE"
    printf '%s\n' "$source" > "$SOURCE_FILE"
    printf '%s\n' "$headless" > "$HEADLESS_FILE"
    touch "$ACTIVE_FILE"
    if prevent_sleep_enabled; then
        printf 'true\n' > "$PREVENT_SLEEP_FILE"
    else
        printf 'false\n' > "$PREVENT_SLEEP_FILE"
        "$PREVENT_SLEEP_SCRIPT" enable >/dev/null
    fi

    if [[ -z "$headless" ]]; then
        local before_json after_json
        before_json="$monitors_json"
        hyprctl output create headless >/dev/null
        sleep 0.2
        after_json="$(hyprctl monitors all -j)"
        headless="$(
            jq -nr --argjson before "$before_json" --argjson after "$after_json" '
                ($before | map(.name)) as $old
                | [$after[] | select((.name | startswith("HEADLESS-")) and (.name as $name | $old | index($name) | not)) | .name][0] // empty
            '
        )"
        if [[ -z "$headless" ]]; then
            rollback_enable
            notify "Headless 输出创建失败"
            printf 'failed to create a headless output\n' >&2
            return 1
        fi
        created_headless="$headless"
        printf '%s\n' "$headless" > "$HEADLESS_FILE"
        printf '%s\n' "$headless" > "$CREATED_HEADLESS_FILE"
    fi

    if ! monitor_eval "$source" "$source_mode" "$source_position" "$source_transform"; then
        rollback_enable "$created_headless"
        notify "外接显示器配置失败，已恢复原布局"
        return 1
    fi

    local output output_json mode transform
    for output in "${external_outputs[@]:1}"; do
        output_json="$(jq -c --arg name "$output" '.[] | select(.name == $name)' <<<"$monitors_json")"
        IFS='|' read -r mode transform < <(portrait_mode_for "$output_json")
        if ! monitor_eval "$output" "$mode" "auto-right" "$transform" "$source"; then
            rollback_enable "$created_headless"
            notify "外接显示器配置失败，已恢复原布局"
            return 1
        fi
    done

    if ! monitor_eval "$headless" "$source_mode" "auto-right" "$source_transform" "$source"; then
        rollback_enable "$created_headless"
        notify "Headless 复制配置失败，已恢复原布局"
        return 1
    fi

    local devices_json touch
    devices_json="$(hyprctl devices -j)"
    while IFS= read -r touch; do
        [[ -n "$touch" ]] || continue
        touch_eval "$touch" || {
            rollback_enable "$created_headless"
            notify "触控方向配置失败，已恢复原布局"
            return 1
        }
    done < <(jq -r '.touch[]?.name' <<<"$devices_json")

    sleep 0.3
    local live_json
    live_json="$(hyprctl monitors -j)"
    if ! jq -e --arg source "$source" --arg headless "$headless" '
        any(.[]; .name == $source and
            ((.width == 1080 and .height == 1920 and .transform == 0) or
             (.width == 1920 and .height == 1080 and .transform == 1))) and
        any(.[]; .name == $headless and .mirrorOf == $source)
    ' <<<"$live_json" >/dev/null; then
        rollback_enable "$created_headless"
        notify "Demo mode 验证失败，已恢复原布局"
        printf 'demo monitor verification failed\n' >&2
        return 1
    fi

    for output in "${external_outputs[@]:1}"; do
        if ! jq -e --arg output "$output" --arg source "$source" '
            any(.[]; .name == $output and .mirrorOf == $source and
                ((.width == 1080 and .height == 1920 and .transform == 0) or
                 (.width == 1920 and .height == 1080 and .transform == 1)))
        ' <<<"$live_json" >/dev/null; then
            rollback_enable "$created_headless"
            notify "Demo mode 逐屏验证失败，已恢复原布局"
            printf 'external mirror verification failed for %s\n' "$output" >&2
            return 1
        fi
    done

    notify "已启用：防睡眠、1080×1920@60、外接复制、Headless 复制、横向触控"
    status
}

disable() {
    cleanup_stale_state
    if ! state_belongs_to_current_session; then
        status || true
        return 0
    fi

    local created_headless="" prevent_sleep_was_enabled="false"
    [[ -f "$CREATED_HEADLESS_FILE" ]] && created_headless="$(<"$CREATED_HEADLESS_FILE")"
    [[ -f "$PREVENT_SLEEP_FILE" ]] && prevent_sleep_was_enabled="$(<"$PREVENT_SLEEP_FILE")"

    hyprctl reload >/dev/null
    if [[ -n "$created_headless" ]]; then
        hyprctl output remove "$created_headless" >/dev/null 2>&1 || true
    fi
    if [[ "$prevent_sleep_was_enabled" != "true" ]]; then
        "$PREVENT_SLEEP_SCRIPT" disable >/dev/null 2>&1 || true
    fi
    rm -rf -- "$STATE_DIR"
    notify "已退出并恢复原显示器、触控与睡眠设置"
    printf 'enabled=false\n'
    printf 'source=\n'
    printf 'headless=\n'
}

toggle() {
    cleanup_stale_state
    if state_belongs_to_current_session; then
        disable
    else
        enable
    fi
}

case "${1:-status}" in
    status)
        status
        ;;
    enable)
        enable
        ;;
    disable)
        disable
        ;;
    toggle)
        toggle
        ;;
    *)
        printf 'Usage: %s [status|enable|disable|toggle]\n' "$0" >&2
        exit 1
        ;;
esac
