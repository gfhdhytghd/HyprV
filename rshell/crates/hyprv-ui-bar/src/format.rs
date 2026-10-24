use crate::model::*;
pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}
pub fn human_rate(v: f64) -> String {
    let v = if v.is_finite() { v.max(0.) } else { 0. };
    if v < 1024. {
        format!("{} B/s", js_round(v))
    } else if v < 1048576. {
        format!("{:.1}kB/s", v / 1024.)
    } else if v < 1073741824. {
        format!("{:.1}MB/s", v / 1048576.)
    } else {
        format!("{:.1}GB/s", v / 1073741824.)
    }
}
pub fn wireless(iface: &str) -> bool {
    iface.starts_with("wl") || iface.starts_with("wlan") || iface.starts_with("wifi")
}
pub fn network_label(m: &Model) -> String {
    let iface = m
        .stats
        .iface
        .as_deref()
        .or(m.network.default_iface.as_deref())
        .unwrap_or("");
    if iface.is_empty() {
        "\u{f092e} nocon".into()
    } else {
        format!(
            "{} {}",
            if wireless(iface) {
                "\u{f05a9}"
            } else {
                "\u{f0200}"
            },
            human_rate(m.stats.rx_bytes_per_s.unwrap_or(0.) + m.stats.tx_bytes_per_s.unwrap_or(0.))
        )
    }
}
pub fn volume(m: &Model) -> &'static str {
    if !m.audio.available {
        "\u{f026}"
    } else if m.audio.muted {
        "\u{f466}"
    } else if m.audio.volume_percent <= 20 {
        "\u{f026}"
    } else if m.audio.volume_percent <= 50 {
        "\u{f027}"
    } else {
        "\u{f028}"
    }
}
pub fn battery_glyph(percent: f64) -> char {
    let code = [
        0xf008e, 0xf007a, 0xf007b, 0xf007c, 0xf007d, 0xf007e, 0xf007f, 0xf0080, 0xf0081, 0xf0082,
        0xf0079,
    ][js_round(percent / 10.).clamp(0., 10.) as usize];
    char::from_u32(code).unwrap()
}
/// `cpuPackagePowerText`.
pub fn cpu_power_text(m: &Model) -> String {
    match m.stats.package_power_w.filter(|v| v.is_finite() && (*v >= 0. || !m.laptop())) {
        Some(w) if w >= 100. => format!("{w:.0} W"),
        Some(w) => format!("{w:.1} W"),
        None => "-- W".into(),
    }
}
pub fn power_label(m: &Model) -> String {
    if m.battery_available() {
        let p = js_round(m.battery_percent());
        let glyph = if !m.battery_on_battery() {
            '\u{f1e6}'
        } else {
            battery_glyph(p)
        };
        format!("{glyph} {p}%")
    } else {
        cpu_power_text(m)
    }
}
pub fn wifi_glyph(m: &Model) -> &'static str {
    if m.network.wired_connected || m.network.other_connected {
        return "\u{f0200}";
    }
    if !m.network.enabled {
        return "\u{f092d}";
    }
    if !m.network.connected {
        return "\u{f092e}";
    }
    match m.network.signal {
        0..=19 => "\u{f092f}",
        20..=39 => "\u{f091f}",
        40..=59 => "\u{f0922}",
        60..=79 => "\u{f0925}",
        _ => "\u{f0928}",
    }
}
pub fn wifi_asset_key(m: &Model, dark: bool) -> String {
    if m.network.wired_connected || m.network.other_connected {
        return if dark {
            "network-wired-symbolic"
        } else {
            "network-wired-light"
        }
        .into();
    }
    let suffix = if dark { "-symbolic" } else { "" };
    let name = if !m.network.hardware_enabled {
        if dark {
            "network-wireless-hardware-disabled"
        } else {
            "network-wireless-offline"
        }
        .into()
    } else if !m.network.enabled {
        if dark {
            "network-wireless-disabled"
        } else {
            "network-wireless-off"
        }
        .into()
    } else if !m.network.connected {
        "network-wireless-disconnected".into()
    } else {
        let bucket = match m.network.signal {
            0..=19 => "0",
            20..=39 => "25",
            40..=59 => "50",
            60..=79 => "75",
            _ => "100",
        };
        format!(
            "nm-signal-{bucket}{}",
            if m.network.security.is_empty() || m.network.security == "open" {
                ""
            } else {
                "-secure"
            }
        )
    };
    format!("{name}{suffix}")
}
pub fn duration(s: f64) -> String {
    if !s.is_finite() || s < 0. {
        return "计算中".into();
    }
    let minutes = js_round(s / 60.) as u64;
    let (h, r) = (minutes / 60, minutes % 60);
    if h > 0 && r > 0 {
        format!("{h}小时{r}分钟")
    } else if h > 0 {
        format!("{h}小时")
    } else {
        format!("{minutes}分钟")
    }
}
pub fn watts(v: Option<f64>) -> String {
    match v.filter(|v| v.is_finite()) {
        Some(v) => format!(
            "{}{:.*} W",
            if v > 0.004 {
                "+"
            } else if v < -0.004 {
                "-"
            } else {
                ""
            },
            if v.abs() >= 10. { 1 } else { 2 },
            v.abs()
        ),
        None => "--".into(),
    }
}
pub fn battery_status(info: &BatteryDetail) -> String {
    match info.mode {
        BatteryMode::Charging => "正在充电",
        BatteryMode::Discharging => "电池供电中",
        BatteryMode::Full => "已充满",
        BatteryMode::Plugged => "已接通电源，当前未充电",
        _ => {
            if info.status.is_empty() {
                "电池状态未知"
            } else {
                &info.status
            }
        }
    }
    .into()
}
/// `battery-info.sh` prints `"estimateSeconds": null` while estimating, and QML's
/// `Number(null)` is 0, so the popup shows "0分钟 (按均值)" rather than 计算中
/// (origin/laptop@24972f7 `lap-battery-popup-estimating`; same code on main).
pub const QUIRK_NULL_ESTIMATE_IS_ZERO: bool = true;
pub fn estimate(info: &BatteryDetail) -> String {
    match info.mode {
        BatteryMode::Full => "已充满".into(),
        BatteryMode::Plugged => "已接通电源，当前未充电".into(),
        _ => match info
            .estimate_s
            .or(QUIRK_NULL_ESTIMATE_IS_ZERO.then_some(0.))
            .filter(|s| s.is_finite() && *s >= 0.)
        {
            None => "计算中".into(),
            Some(s) => format!(
                "{} ({})",
                duration(s),
                if info.estimate_basis == "current" {
                    "按当前功率"
                } else {
                    "按均值"
                }
            ),
        },
    }
}
pub fn sample_window(info: &BatteryDetail) -> String {
    if !info.available {
        String::new()
    } else if info.window_s >= 1800 {
        "最近 30 分钟采样".into()
    } else if info.window_s >= 60 {
        format!("已采样 {}", duration(info.window_s as f64))
    } else {
        "开始采样中".into()
    }
}
