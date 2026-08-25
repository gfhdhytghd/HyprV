#!/usr/bin/env bash

set -euo pipefail

config_file="${HOME}/.config/mihomo/config.yaml"

yaml_value() {
    local key="$1"
    [[ -r "$config_file" ]] || return 0
    sed -n -E "s/^[[:space:]]*${key}:[[:space:]]*['\"]?([^'\"#]+)['\"]?[[:space:]]*(#.*)?$/\1/p" "$config_file" \
        | head -n 1 \
        | sed -E 's/[[:space:]]+$//'
}

controller="$(yaml_value external-controller)"
secret="$(yaml_value secret)"
controller="${controller:-127.0.0.1:9090}"
case "$controller" in
    http://*|https://*) api_base="${controller%/}" ;;
    :*) api_base="http://127.0.0.1${controller}" ;;
    0.0.0.0:*) api_base="http://127.0.0.1:${controller##*:}" ;;
    \[*\]:*) api_base="http://127.0.0.1:${controller##*:}" ;;
    *) api_base="http://${controller%/}" ;;
esac

curl_args=(--silent --show-error --max-time 2)
[[ -n "$secret" ]] && curl_args+=(-H "Authorization: Bearer ${secret}")

api_get() {
    curl "${curl_args[@]}" "$api_base$1"
}

api_patch() {
    curl "${curl_args[@]}" -X PATCH -H 'Content-Type: application/json' -d "$2" "$api_base$1"
}

service_scope="none"
if systemctl --user cat mihomo.service >/dev/null 2>&1; then
    service_scope="user"
elif systemctl cat mihomo.service >/dev/null 2>&1; then
    service_scope="system"
elif pgrep -f 'clash-verge-service|verge-mihomo' >/dev/null 2>&1; then
    service_scope="verge"
fi

status_json() {
    local version_json configs_json connections_json traffic_json
    local online=false version="" mode="" connections=0 upload=0 download=0

    version_json="$(api_get /version 2>/dev/null || true)"
    if jq -e 'type == "object"' >/dev/null 2>&1 <<<"$version_json"; then
        online=true
        version="$(jq -r '.version // ""' <<<"$version_json")"
        configs_json="$(api_get /configs 2>/dev/null || true)"
        connections_json="$(api_get /connections 2>/dev/null || true)"
        # `/traffic` is a stream. One frame is the current rate; keeping curl
        # attached would make every panel refresh wait for its timeout.
        traffic_json="$(curl --no-buffer "${curl_args[@]}" "$api_base/traffic" 2>/dev/null | head -n 1 || true)"
        mode="$(jq -r '.mode // ""' <<<"$configs_json" 2>/dev/null || true)"
        connections="$(jq -r '(.connections // []) | length' <<<"$connections_json" 2>/dev/null || printf 0)"
        upload="$(jq -r '.up // 0' <<<"$traffic_json" 2>/dev/null || printf 0)"
        download="$(jq -r '.down // 0' <<<"$traffic_json" 2>/dev/null || printf 0)"
    fi

    jq -cn \
        --argjson online "$online" \
        --arg service "$service_scope" \
        --arg version "$version" \
        --arg mode "$mode" \
        --argjson connections "${connections:-0}" \
        --argjson upload "${upload:-0}" \
        --argjson download "${download:-0}" \
        --arg dashboard "${api_base}/ui" \
        '{online:$online,service:$service,version:$version,mode:$mode,connections:$connections,upload:$upload,download:$download,dashboard:$dashboard}'
}

action="${1:-status}"
case "$action" in
    status)
        status_json
        ;;
    mode)
        mode="${2:-}"
        case "$mode" in rule|global|direct) ;; *) exit 2 ;; esac
        api_patch /configs "{\"mode\":\"${mode}\"}" >/dev/null
        status_json
        ;;
    start|stop|restart)
        case "$service_scope" in
            user) systemctl --user "$action" mihomo.service ;;
            system) systemctl "$action" mihomo.service ;;
            *) exit 3 ;;
        esac
        status_json
        ;;
    dashboard)
        printf '%s\n' "${api_base}/ui"
        ;;
    *)
        exit 2
        ;;
esac
