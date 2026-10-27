use crate::{format, model::*};
use hyprv_ui::{animation::Color, geometry::Rect};
use hyprv_widgets::{
    foundations::{rgb, Palette},
    primitives::{FontSpec, Label},
};
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub target: Target,
    pub label: String,
    pub frame: Rect,
    pub px: f32,
    pub color: Color,
    pub padding: (f32, f32),
    pub hoverable: bool,
    pub interactive: bool,
    pub highlighted: bool,
    pub highlight: Color,
    pub highlighted_text: Color,
}
impl Module {
    pub fn width(label: &str, px: f32, pad: (f32, f32), minimum: f32) -> f32 {
        (measure(label, px, 700).0 + pad.0 + pad.1).max(minimum)
    }
}
pub fn measure(text: &str, px: f32, weight: i32) -> (f32, f32) {
    let m = Label::new(
        text,
        FontSpec {
            pixel_size: px,
            weight,
            ..Default::default()
        },
        Color::default(),
    )
    .measure();
    (m.width, m.height)
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrayGeometry {
    pub total: usize,
    pub visible: usize,
    pub overflow: usize,
    pub requested: f32,
    pub spacing: f32,
    pub container_width: f32,
    pub budget: f32,
}
pub fn tray_geometry(total: usize, budget: f32) -> TrayGeometry {
    let fixed = |v: usize| v as f32 * 18. + if total > v { 22. } else { 0. };
    let gaps = |v: usize| (v + usize::from(total > v)).saturating_sub(1) as f32;
    let visible = (0..=total)
        .rev()
        .find(|v| fixed(*v) + gaps(*v) * 3. <= budget)
        .unwrap_or(0);
    let overflow = total - visible;
    let min = fixed(visible) + gaps(visible) * 3.;
    let pref = fixed(visible) + gaps(visible) * 7.;
    let requested = if total == 0 {
        0.
    } else if overflow > 0 || pref > budget {
        min.max(budget)
    } else {
        pref
    };
    TrayGeometry {
        total,
        visible,
        overflow,
        requested,
        spacing: if gaps(visible) > 0. {
            ((requested - fixed(visible)) / gaps(visible)).max(0.)
        } else {
            0.
        },
        container_width: if total > 0 { requested + 2. } else { 0. },
        budget,
    }
}
pub fn tray_key(item: &TrayItem) -> String {
    format!(
        "{} {} {} {} {}",
        item.id, item.title, item.tooltip_title, item.tooltip_description, item.icon
    )
    .to_lowercase()
}
pub fn tray_priority(i: &TrayItem) -> u8 {
    let k = tray_key(i);
    if i.id == "chrome_status_icon_1"
        && i.tooltip_title.is_empty()
        && i.tooltip_description.is_empty()
    {
        0
    } else if k.contains("discord") {
        1
    } else if k.contains("keepass") || k.contains("password.kdbx") {
        2
    } else if ["local-ai-service", "lais_gui", "preferences-system"]
        .iter()
        .any(|s| k.contains(s))
    {
        3
    } else if k.contains("懒猫") || k.contains("微服") {
        4
    } else if ["nm-applet", "wi-fi", "网络"].iter().any(|s| k.contains(s)) {
        5
    } else if k.contains("fcitx") || k.contains("输入法") {
        6
    } else if k.contains("wechat") || k.contains("微信") {
        7
    } else {
        100
    }
}
pub fn sorted_tray(m: &Model) -> Vec<TrayItem> {
    let mut items: Vec<_> = m
        .tray
        .items
        .iter()
        .filter(|item| {
            !m.network_visible()
                || !["nm-applet", "networkmanager", "network-manager"]
                    .iter()
                    .any(|s| tray_key(item).contains(s))
        })
        .cloned()
        .collect();
    items.sort_by_key(tray_priority);
    items
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub modules: Vec<Module>,
    pub pills: Vec<Rect>,
    pub center: Rect,
    pub title: Rect,
    pub supplementary: bool,
    pub threshold: f32,
    pub spacing: f32,
    pub tray: TrayGeometry,
    pub tray_items: Vec<TrayItem>,
    pub content_y: f32,
    pub input_regions: Vec<Rect>,
    pub quick_adjust_anchor: Rect,
}
impl Layout {
    pub fn target(&self, target: &Target) -> Option<Rect> {
        self.modules.iter().find(|n| &n.target == target).map(|n| {
            Rect::new(
                n.frame.x,
                n.frame.y + self.content_y,
                n.frame.width,
                n.frame.height,
            )
        })
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<Target> {
        let y = y - self.content_y;
        self.modules
            .iter()
            .rev()
            .find(|n| (n.interactive || n.target == Target::CpuPower) && contains(n.frame, x, y))
            .map(|n| n.target.clone())
    }
}
pub fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
}
pub fn compute(m: &Model, theme: &Theme) -> Layout {
    let p = Palette::new(theme.dark);
    let w = m.output.width;
    let title = m.title();
    let workspaces: Vec<_> = m
        .desktop
        .workspaces
        .iter()
        .filter(|s| {
            s.id >= 0
                && (!s.name.starts_with("special:") || (QUIRK_LAPTOP_SPECIAL_WS_UNFILTERED && m.laptop()))
                && s.name != "viewflow-underlay"
        })
        .collect();
    let mut workspaces = workspaces;
    workspaces.sort_by_key(|s| s.id);
    let workspace_labels: Vec<(i32, String)> = if workspaces.is_empty() {
        vec![(1, "1".into())]
    } else {
        workspaces
            .iter()
            .map(|s| {
                (
                    s.id,
                    if s.name.is_empty() {
                        s.id.to_string()
                    } else {
                        s.name.clone()
                    },
                )
            })
            .collect()
    };
    let launcher = Module::width("\u{f303}", 17., (7., 4.), 0.);
    let viewflow = if QUIRK_LAPTOP_NO_VIEWFLOW_BUTTON && m.laptop() {
        0.
    } else {
        Module::width("\u{f05b3}", 17., (7., 7.), 0.)
    };
    let primary = launcher
        + viewflow
        + 8.
        + workspace_labels
            .iter()
            .map(|(_, s)| Module::width(s, 16., (5., 5.), 32.))
            .sum::<f32>();
    let cpu = format!(
        "\u{f2db} {}%",
        format::js_round(m.stats.cpu_percent.unwrap_or(0.))
    );
    let mem = format!(
        "\u{f0c9} {}%",
        format::js_round(m.stats.memory_percent.unwrap_or(0.))
    );
    let network = format::network_label(m);
    let temp = m.stats.temperature_c.unwrap_or(0.);
    let temperature = format!(
        "{} {}°C",
        if temp >= 70. { "\u{f2c7}" } else { "\u{f2c9}" },
        format::js_round(temp)
    );
    let battery = format::power_label(m);
    let volume = format::volume(m);
    let notification = if m.dnd() { "\u{f009b}" } else { "\u{f0a2}" };
    let bell_w = measure(notification, 14., 700).0 + 10.;
    let wifi_w = if m.network_visible() { 30. } else { 0. };
    let play = if m.media.playing {
        "\u{f04c}"
    } else {
        "\u{f04b}"
    };
    let cpu_w = Module::width(&cpu, 16., (12., 4.), 0.);
    let mem_w = Module::width(&mem, 16., (6., 4.), 0.);
    let net_w = Module::width(&network, 16., (6., 12.), 0.);
    let full_left = 10. + primary + 9.5 + cpu_w + mem_w + net_w;
    let cpu_power = format::cpu_power_text(m);
    let cpu_power_shown = m.laptop() && m.cpu_power_available();
    let full_right = 9.5
        + if m.laptop() {
            // laptop-bar.md §7: CPU W counts only when available; temperature is gone.
            if cpu_power_shown {
                Module::width(&cpu_power, 16., (12., 8.), 0.)
            } else {
                0.
            }
        } else {
            Module::width(&temperature, 16., (10., 5.), 0.)
        }
        + Module::width(&battery, 16., (5., 12.), 0.)
        + 9.5
        + Module::width("\u{f137}", 16., (9., 5.), 0.)
        + Module::width(play, 16., (5., 0.), 0.)
        + Module::width("\u{f138}", 16., (5., 0.), 0.)
        + Module::width(volume, 16., (8., 12.), 0.)
        + 9.5
        + 4.
        + wifi_w
        + 28.
        + bell_w
        + 6.;
    let full_right = full_right
        + if QUIRK_TRAY_EXCLUDED_FROM_SUPPLEMENTARY_THRESHOLD {
            0.
        } else {
            let count = sorted_tray(m).len();
            count as f32 * 18. + count.saturating_sub(1) as f32 * 7.
        };
    let threshold =
        if m.calendar_presented { 520. } else { 448. } + 19. + 2. * full_left.max(full_right);
    let supplementary = w >= threshold;
    let spacing = if supplementary { 9.5 } else { 5. };
    let (center_w, center_h, offset) = if m.calendar_presented {
        (m.calendar_width, m.calendar_height, 0.)
    } else {
        (
            m.island.width,
            m.island.height,
            m.island.attached_center_offset,
        )
    };
    let center = Rect::new(
        hyprv_ui::geometry::qt_center_half(w) - hyprv_ui::geometry::qt_center_half(center_w)
            + offset,
        10.,
        center_w,
        center_h,
    );
    let content_y = -((center.height + 10.).max(68.)) as f64 * (1. - m.reveal)
        - ((center.height + 10.).max(68.)) as f64 * m.fullscreen_hide * m.reveal;
    let content_y = content_y as f32;
    let mut modules = Vec::new();
    let mut pills = Vec::new();
    let mut add = |target,
                   label: String,
                   frame,
                   px,
                   color,
                   padding,
                   hoverable,
                   interactive,
                   highlighted,
                   highlight: Color,
                   highlighted_text: Color| {
        modules.push(Module {
            target,
            label,
            frame,
            px,
            color,
            padding,
            hoverable,
            interactive,
            highlighted,
            highlight,
            highlighted_text,
        })
    };
    let mut x = 10.;
    pills.push(Rect::new(x, 10., primary, 38.));
    add(
        Target::Launcher,
        "\u{f303}".into(),
        Rect::new(x, 10., launcher, 38.),
        17.,
        p.accent,
        (7., 4.),
        false,
        true,
        false,
        p.active_workspace,
        p.active_workspace_text,
    );
    x += launcher;
    if viewflow > 0. { add(
        Target::Viewflow,
        "\u{f05b3}".into(),
        Rect::new(x, 10., viewflow, 38.),
        17.,
        p.accent,
        (7., 7.),
        true,
        true,
        false,
        p.active_workspace,
        p.active_workspace_text,
    ); }
    x += viewflow + 4.;
    for (id, label) in workspace_labels {
        let width = Module::width(&label, 16., (5., 5.), 32.);
        let urgent = m.urgent_workspaces.contains(&id) && m.focused_workspace() != id;
        add(
            Target::Workspace(id),
            label,
            Rect::new(x, 13., width, 32.),
            16.,
            p.muted_workspace_text,
            (5., 5.),
            true,
            true,
            id == m.focused_workspace() || urgent,
            if urgent {
                rgb("#a6e3a1")
            } else {
                p.active_workspace
            },
            if urgent {
                rgb("#11111b")
            } else {
                p.active_workspace_text
            },
        );
        x += width;
    }
    x = 10. + primary + spacing;
    // Laptop only (user request, not legacy): a widened island would cover the stats pill,
    // so the network segment yields; memory then closes the pill with the network padding.
    let net_hidden = m.laptop() && x + cpu_w + mem_w + net_w + spacing > center.x;
    let stats: Vec<_> = if net_hidden {
        let mem_w = Module::width(&mem, 16., (6., 12.), 0.);
        vec![(Target::Cpu, cpu, cpu_w, (12., 4.)), (Target::Memory, mem, mem_w, (6., 12.))]
    } else {
        vec![
            (Target::Cpu, cpu, cpu_w, (12., 4.)),
            (Target::Memory, mem, mem_w, (6., 4.)),
            (Target::Network, network, net_w, (6., 12.)),
        ]
    };
    pills.push(Rect::new(x, 10., stats.iter().map(|s| s.2).sum(), 38.));
    for (target, label, width, padding) in stats {
        add(
            target,
            label,
            Rect::new(x, 10., width, 38.),
            16.,
            p.primary_text,
            padding,
            false,
            true,
            false,
            p.active_workspace,
            p.active_workspace_text,
        );
        x += width;
    }
    let left_end = x;
    let temp_w = if m.laptop() {
        if cpu_power_shown {
            Module::width(&cpu_power, 16., (12., 8.), 0.)
        } else {
            0.
        }
    } else if supplementary {
        Module::width(&temperature, 16., (10., 5.), 0.)
    } else {
        0.
    };
    let battery_pad = if supplementary { 5. } else { 12. };
    let battery_w = Module::width(&battery, 16., (battery_pad, 12.), 0.);
    let status_w = temp_w + battery_w;
    let prev_pad = if supplementary { 9. } else { 12. };
    let next_pad = if supplementary { 0. } else { 17.5 };
    let prev_w = Module::width("\u{f137}", 16., (prev_pad, 5.), 0.);
    let play_w = Module::width(play, 16., (5., 0.), 0.);
    let next_w = Module::width("\u{f138}", 16., (5., next_pad), 0.);
    let vol_w = if supplementary {
        Module::width(volume, 16., (8., 12.), 0.)
    } else {
        0.
    };
    let media_w = prev_w + play_w + next_w + vol_w;
    let items = sorted_tray(m);
    let fixed = status_w + media_w + 2. * spacing + 4. + wifi_w + 28. + bell_w + 6.;
    let available = (w - 9.5 - (center.x + center.width) - 9.5).max(0.);
    let tray = tray_geometry(items.len(), (available - fixed).max(0.));
    let traypill_w = 4. + wifi_w + tray.container_width + 28. + bell_w + 6.;
    let right_w = status_w + spacing + media_w + spacing + traypill_w;
    x = w - 9.5 - right_w;
    pills.push(Rect::new(x, 10., status_w, 38.));
    let severity = |w: Option<f64>| match w {
        Some(w) if w >= 90. => p.critical,
        Some(w) if w >= 60. => p.usage_medium,
        Some(w) if w.is_finite() => p.usage_low,
        _ => p.primary_text,
    };
    if m.laptop() {
        if cpu_power_shown {
            add(
                Target::CpuPower,
                cpu_power.clone(),
                Rect::new(x, 10., temp_w, 38.),
                16.,
                severity(m.stats.package_power_w.or(Some(m.last_package_power_w))),
                (12., 8.),
                false,
                false,
                false,
                p.active_workspace,
                p.active_workspace_text,
            );
            x += temp_w;
        }
    } else if supplementary {
        add(
            Target::Temperature,
            temperature,
            Rect::new(x, 10., temp_w, 38.),
            16.,
            if temp >= 70. {
                p.critical
            } else {
                p.primary_text
            },
            (10., 5.),
            false,
            true,
            false,
            p.active_workspace,
            p.active_workspace_text,
        );
        x += temp_w;
    }
    let power_color =
        if m.battery_available() {
            if m.battery_percent() <= 20. && m.battery_on_battery() {
                p.critical
            } else {
                p.battery
            }
        } else {
            severity(m.stats.package_power_w.or_else(|| {
                QUIRK_UNAVAILABLE_POWER_RETAINS_SEVERITY.then_some(m.last_package_power_w)
            }))
        };
    add(
        Target::Battery,
        battery,
        Rect::new(x, 10., battery_w, 38.),
        16.,
        power_color,
        (battery_pad, 12.),
        false,
        m.battery_available(),
        false,
        p.active_workspace,
        p.active_workspace_text,
    );
    x += battery_w + spacing;
    pills.push(Rect::new(x, 10., media_w, 38.));
    for (target, label, width, padding, color) in [
        (
            Target::Previous,
            "\u{f137}",
            prev_w,
            (prev_pad, 5.),
            p.primary_text,
        ),
        (Target::PlayPause, play, play_w, (5., 0.), p.primary_text),
        (
            Target::Next,
            "\u{f138}",
            next_w,
            (5., next_pad),
            p.primary_text,
        ),
        (Target::Volume, volume, vol_w, (8., 12.), p.accent),
    ] {
        if width > 0. {
            add(
                target,
                label.into(),
                Rect::new(x, 10., width, 38.),
                16.,
                color,
                padding,
                false,
                true,
                false,
                p.active_workspace,
                p.active_workspace_text,
            );
            x += width;
        }
    }
    x += spacing;
    let quick_adjust_anchor = Rect::new(x, 10. + content_y, traypill_w, 38.);
    pills.push(Rect::new(x, 10., traypill_w, 38.));
    x += 4.;
    if wifi_w > 0. {
        add(
            Target::Wifi,
            format::wifi_glyph(m).into(),
            Rect::new(x, 10., wifi_w, 38.),
            24.,
            p.primary_text,
            (0., 0.),
            true,
            true,
            false,
            p.active_workspace,
            p.active_workspace_text,
        );
        x += wifi_w;
    }
    if !items.is_empty() {
        let start = x;
        let mut tx = x + 1.;
        for item in items.iter().take(tray.visible) {
            add(
                Target::Tray(item.key.clone()),
                item.id
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".into()),
                Rect::new(tx, 10., 18., 38.),
                10.,
                p.primary_text,
                (0., 0.),
                false,
                true,
                false,
                p.active_workspace,
                p.active_workspace_text,
            );
            tx += 18. + tray.spacing;
        }
        if tray.overflow > 0 {
            add(
                Target::Overflow,
                "⋯".into(),
                Rect::new(tx, 10., 22., 38.),
                18.,
                p.primary_text,
                (0., 0.),
                false,
                true,
                false,
                p.active_workspace,
                p.active_workspace_text,
            );
        }
        x = start + tray.container_width;
    }
    add(
        Target::ControlPanel,
        String::new(),
        Rect::new(x, 10., 28., 38.),
        16.,
        p.primary_text,
        (0., 0.),
        false,
        true,
        false,
        p.active_workspace,
        p.active_workspace_text,
    );
    x += 28.;
    add(
        Target::Notifications,
        notification.into(),
        Rect::new(x, 10., bell_w, 38.),
        14.,
        p.primary_text,
        (0., 0.),
        false,
        true,
        false,
        p.active_workspace,
        p.active_workspace_text,
    );
    let available_title = (center.x - left_end - 2. * spacing).max(0.);
    let title_w = if title.is_empty() {
        0.
    } else {
        available_title.min((measure(title, 16., 700).0 + 24.).max(38.))
    };
    let title = Rect::new(left_end + spacing, 10., title_w, 38.);
    let mut input_regions = vec![
        Rect::new(0., content_y.floor(), w.ceil(), 58.),
        Rect::new(center.x, center.y + content_y, center.width, center.height),
    ];
    if m.calendar_open {
        input_regions.push(Rect::new(0., content_y, w, m.surface_height()));
    }
    Layout {
        modules,
        pills,
        center,
        title,
        supplementary,
        threshold,
        spacing,
        tray,
        tray_items: items,
        content_y,
        input_regions,
        quick_adjust_anchor,
    }
}
