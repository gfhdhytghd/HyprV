//! Network icon sources and glyph fallbacks (ui-panels-wifi.md §5, shell.qml:1717-1786,
//! 1925-1960). Sources are keys into [`crate::model::Assets::svgs`], relative to the
//! Fluent theme directory exactly as the QML builds the path.
use crate::model::{NetType, Network};

pub fn signal_bucket(p: i32) -> &'static str {
    if p < 20 {
        "0"
    } else if p < 40 {
        "25"
    } else if p < 60 {
        "50"
    } else if p < 80 {
        "75"
    } else {
        "100"
    }
}

/// `fluentWifiIconSource(name, dark)` relative path.
pub fn fluent(name: &str, dark: bool) -> String {
    if dark {
        format!("symbolic/status/{name}-symbolic.svg")
    } else {
        format!("24/panel/{name}.svg")
    }
}

pub fn wifi_list_icon(signal: i32, secure: bool, dark: bool) -> String {
    fluent(&format!("nm-signal-{}{}", signal_bucket(signal), if secure { "-secure" } else { "" }), dark)
}

pub fn wifi_tray_icon(enabled: bool, hw: bool, connected: bool, strength: f32, secure: bool, dark: bool) -> String {
    let p = (strength * 100.).round() as i32;
    let name = if dark {
        if !hw {
            "network-wireless-hardware-disabled".to_string()
        } else if !enabled {
            "network-wireless-disabled".into()
        } else if !connected {
            "network-wireless-disconnected".into()
        } else {
            format!("nm-signal-{}{}", signal_bucket(p), if secure { "-secure" } else { "" })
        }
    } else if !hw {
        "network-wireless-offline".into()
    } else if !enabled {
        "network-wireless-off".into()
    } else if !connected {
        "network-wireless-disconnected".into()
    } else {
        format!("nm-signal-{}{}", signal_bucket(p), if secure { "-secure" } else { "" })
    };
    fluent(&name, dark)
}

pub fn wired_tray_icon(dark: bool) -> String {
    if dark {
        fluent("network-wired", true)
    } else {
        "assets/tray/network-wired-light.svg".into()
    }
}

pub fn network_tray_icon(n: &Network, dark: bool) -> String {
    match n.active_type {
        NetType::Wired | NetType::Other => wired_tray_icon(dark),
        NetType::Wifi => wifi_tray_icon(true, n.wifi_hardware_enabled, true, n.wifi_signal_strength, n.wifi_secure, dark),
        NetType::Offline => wifi_tray_icon(n.wifi_radio_enabled, n.wifi_hardware_enabled, n.wifi_connected, n.wifi_signal_strength, n.wifi_secure, dark),
    }
}

pub fn wifi_signal_glyph(p: i32) -> &'static str {
    if p < 20 {
        "\u{F092F}"
    } else if p < 40 {
        "\u{F091F}"
    } else if p < 60 {
        "\u{F0922}"
    } else if p < 80 {
        "\u{F0925}"
    } else {
        "\u{F0928}"
    }
}

pub fn wifi_tray_glyph(enabled: bool, connected: bool, strength: f32) -> &'static str {
    if !enabled {
        "\u{F092D}"
    } else if !connected {
        "\u{F092E}"
    } else {
        wifi_signal_glyph((strength * 100.).round() as i32)
    }
}

pub fn network_tray_glyph(n: &Network) -> &'static str {
    match n.active_type {
        NetType::Wired | NetType::Other => "\u{F0200}",
        NetType::Wifi => wifi_signal_glyph((n.wifi_signal_strength * 100.).round() as i32),
        NetType::Offline => wifi_tray_glyph(n.wifi_radio_enabled, n.wifi_connected, n.wifi_signal_strength),
    }
}
