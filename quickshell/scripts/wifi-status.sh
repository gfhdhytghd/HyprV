#!/bin/sh

set -eu

iface="$(nmcli -t -f DEVICE,TYPE dev status 2>/dev/null | awk -F: '$2 == "wifi" { print $1; exit }')"
radio_status="$(nmcli -t -f WIFI,WIFI-HW general status 2>/dev/null || printf 'enabled:enabled')"
wifi_enabled="$(printf '%s' "$radio_status" | cut -d: -f1)"
wifi_hw_enabled="$(printf '%s' "$radio_status" | cut -d: -f2)"

tmp_networks="$(mktemp)"
tmp_known="$(mktemp)"

cleanup() {
    rm -f "$tmp_networks" "$tmp_known"
}

trap cleanup EXIT INT TERM

if [ -n "$iface" ]; then
    nmcli -m multiline -f IN-USE,SSID,SIGNAL,SECURITY,BARS dev wifi list ifname "$iface" --rescan no 2>/dev/null | awk '
        function trim(text) {
            sub(/^[[:space:]]+/, "", text);
            sub(/[[:space:]]+$/, "", text);
            return text;
        }

        function emit() {
            if (!seen || ssid == "" || ssid == "--") {
                active = "";
                ssid = "";
                signal = "0";
                security = "";
                bars = "";
                seen = 0;
                return;
            }

            gsub(/\t/, " ", ssid);
            gsub(/\t/, " ", security);
            gsub(/\t/, " ", bars);
            printf "%s\t%s\t%s\t%s\t%s\n", (active == "*" ? "1" : "0"), ssid, signal + 0, security, bars;

            active = "";
            ssid = "";
            signal = "0";
            security = "";
            bars = "";
            seen = 0;
        }

        /^IN-USE:/ {
            emit();
            active = trim(substr($0, index($0, ":") + 1));
            seen = 1;
            next;
        }

        /^SSID:/ {
            ssid = trim(substr($0, index($0, ":") + 1));
            next;
        }

        /^SIGNAL:/ {
            signal = trim(substr($0, index($0, ":") + 1));
            next;
        }

        /^SECURITY:/ {
            security = trim(substr($0, index($0, ":") + 1));
            next;
        }

        /^BARS:/ {
            bars = trim(substr($0, index($0, ":") + 1));
            next;
        }

        END {
            emit();
        }
    ' > "$tmp_networks"

    nmcli -t -f UUID,TYPE connection show 2>/dev/null | awk -F: '$2 == "802-11-wireless" { print $1 }' | while IFS= read -r uuid; do
        nmcli -g 802-11-wireless.ssid connection show "$uuid" 2>/dev/null || true
    done | sed '/^$/d' | sort -u > "$tmp_known"
fi

jq -Rn \
    --arg iface "$iface" \
    --arg wifiEnabled "$wifi_enabled" \
    --arg wifiHwEnabled "$wifi_hw_enabled" \
    --rawfile networks "$tmp_networks" \
    --rawfile known "$tmp_known" '
    def known_ssids:
        $known
        | split("\n")
        | map(select(length > 0));

    def parsed_networks:
        $networks
        | split("\n")
        | map(select(length > 0) | split("\t"))
        | map({
            active: (.[0] == "1"),
            ssid: .[1],
            signal: (.[2] | tonumber),
            security: (if .[3] == "--" then "" else .[3] end),
            bars: (.[4] // "")
        })
        | map(. as $network | . + {
            secure: ($network.security | length > 0),
            known: (known_ssids | index($network.ssid) != null),
            enterprise: (($network.security | contains("802.1X")) or ($network.security | contains("EAP")))
        })
        | sort_by(.ssid)
        | group_by(.ssid)
        | map(sort_by([if .active then 0 else 1 end, -(.signal)]) | .[0])
        | sort_by([if .active then 0 else 1 end, -(.signal), .ssid]);

    def active_network:
        parsed_networks
        | map(select(.active))
        | .[0] // null;

    {
        present: ($iface | length > 0),
        iface: $iface,
        enabled: ($wifiEnabled == "enabled"),
        hardwareEnabled: ($wifiHwEnabled == "enabled"),
        connected: (active_network != null),
        ssid: (active_network.ssid // ""),
        signal: (active_network.signal // 0),
        security: (active_network.security // ""),
        networks: parsed_networks
    }
'
