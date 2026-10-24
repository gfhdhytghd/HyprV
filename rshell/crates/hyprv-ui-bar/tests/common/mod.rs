#![allow(dead_code)]
use hyprv_svc_sys::hyprland::{DesktopMonitor, DesktopWindow, Workspace};
use hyprv_ui::{geometry::Rect, scene::Node};
use hyprv_ui_bar::{
    input::PopupCommand,
    model::*,
    motion::Motion,
    popups::{Kind, Popups},
    surfaces::{self, Corner},
    view,
};
use std::time::Duration;
pub fn baseline() -> Model {
    let mut m = Model::default();
    m.desktop.connected = true;
    m.desktop.workspaces = (1..=5)
        .map(|id| Workspace {
            id,
            name: id.to_string(),
            monitor: m.output.name.clone(),
            active: id == 1,
            windows: 1,
            tooltip: String::new(),
            state: if id == 1 { "active" } else { "occupied" }.into(),
        })
        .collect();
    m.desktop.monitors = vec![DesktopMonitor {
        name: m.output.name.clone(),
        id: 0,
        geometry: hyprv_svc_sys::hyprland::Rect {
            x: 0.,
            y: 0.,
            width: m.output.width as f64,
            height: m.output.height as f64,
        },
        scale: 2.,
        transform: 0,
    }];
    m.desktop.windows = vec![DesktopWindow {
        address: "0x1".into(),
        pid: None,
        class: "editor".into(),
        title: "HyprV — Rust shell".into(),
        monitor: 0,
        geometry: Default::default(),
        fullscreen: false,
    }];
    m.desktop.active_window = Some("0x1".into());
    m.stats.cpu_percent = Some(7.);
    m.stats.memory_percent = Some(42.);
    m.stats.temperature_c = Some(45.);
    m.stats.package_power_w = Some(12.3);
    m.stats.iface = Some("eth0".into());
    m.stats.rx_bytes_per_s = Some(0.);
    m.stats.tx_bytes_per_s = Some(0.);
    m.network.enabled = true;
    m.network.hardware_enabled = true;
    m.network.wired_connected = true;
    m.network.default_iface = Some("eth0".into());
    m.audio.available = true;
    m.audio.volume_percent = 75;
    m.tray.items = (0..6)
        .map(|i| TrayItem {
            key: format!("tray-{i}"),
            id: format!("App{i}"),
            title: format!("App {i}"),
            has_menu: true,
            menu: Some("/menu".into()),
            ..Default::default()
        })
        .collect();
    m
}
pub fn menu_entries() -> Vec<TrayMenuEntry> {
    vec![
        TrayMenuEntry {
            id: 1,
            text: "Open application".into(),
            visible: true,
            enabled: true,
            ..Default::default()
        },
        TrayMenuEntry {
            id: 2,
            text: "Enabled check".into(),
            visible: true,
            enabled: true,
            button_type: ButtonType::CheckBox,
            check_state: CheckState::Checked,
            ..Default::default()
        },
        TrayMenuEntry {
            id: 3,
            text: "Radio option".into(),
            visible: true,
            enabled: true,
            button_type: ButtonType::RadioButton,
            check_state: CheckState::Checked,
            ..Default::default()
        },
        TrayMenuEntry {
            id: 4,
            is_separator: true,
            visible: true,
            ..Default::default()
        },
        TrayMenuEntry {
            id: 5,
            text: "Disabled entry".into(),
            visible: true,
            enabled: false,
            ..Default::default()
        },
        TrayMenuEntry {
            id: 6,
            text: "Submenu".into(),
            visible: true,
            enabled: true,
            has_children: true,
            children: vec![TrayMenuEntry {
                id: 61,
                text: "Child item".into(),
                visible: true,
                enabled: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        TrayMenuEntry {
            id: 7,
            text: "Long label to exercise trailing ellipsis and clipping".into(),
            visible: true,
            enabled: true,
            icon_name: "sample".into(),
            ..Default::default()
        },
    ]
}
pub struct Fixture {
    pub name: String,
    pub node: Node,
    pub width: f32,
    pub height: f32,
}
fn bar_fixture(name: &str, m: Model) -> Fixture {
    let height = if m.calendar_presented {
        (m.calendar_height + 20.).max(58.)
    } else {
        (m.island.height + 20.).max(58.)
    };
    Fixture {
        name: name.into(),
        width: m.output.width,
        height,
        node: view::build(&m, &m.theme),
    }
}
fn popup_fixture(name: &str, m: &Model, p: &Popups, kind: Kind) -> Fixture {
    let card = p.card_rect(m, kind);
    let mut wrapper = view::build(&Model::default(), &Theme::default());
    wrapper.content = hyprv_ui::scene::Content::Group;
    wrapper.children.clear();
    wrapper.frame = Rect::new(0., 0., card.width, card.height);
    let mut node = p.build(m, &m.theme, kind);
    node.frame.x = -card.x;
    node.frame.y = -card.y;
    wrapper.add(node);
    Fixture {
        name: name.into(),
        width: card.width,
        height: card.height,
        node: wrapper,
    }
}
pub fn fixtures(dark: bool, scale: f32) -> Vec<Fixture> {
    let mut m = baseline();
    m.theme.dark = dark;
    m.output.scale = scale;
    m.assets = fixture_assets(dark, scale);
    let mut fixtures = vec![bar_fixture("baseline", m.clone())];
    macro_rules! case {
        ($name:expr,$change:expr) => {{
            let mut model = m.clone();
            ($change)(&mut model);
            fixtures.push(bar_fixture($name, model));
        }};
    }
    case!("icons-fallback", |m: &mut Model| m.assets =
        Assets::default());
    case!("metrics", |m: &mut Model| m.stats.package_power_w = None);
    case!("workspace-hover", |m: &mut Model| m.hovered =
        Some(Target::Workspace(2)));
    case!("workspace-urgent", |m: &mut Model| m.urgent_workspaces =
        vec![3]);
    case!("workspace-named", |m: &mut Model| m.desktop.workspaces[1]
        .name =
        "web — 工作".into());
    case!("title-long", |m: &mut Model| m.desktop.windows[0].title =
        "Very long active window title 中文测试 ".repeat(12));
    case!("title-empty", |m: &mut Model| m.desktop.active_window =
        None);
    case!("temperature-critical", |m: &mut Model| m
        .stats
        .temperature_c =
        Some(75.));
    for (w, name) in [
        (59., "power-low"),
        (65., "power-medium"),
        (95., "power-critical"),
        (110., "power-integer"),
    ] {
        case!(name, |m: &mut Model| m.stats.package_power_w = Some(w));
    }
    case!("media-playing", |m: &mut Model| m.media.playing = true);
    case!("media-paused", |m: &mut Model| m.media.playing = false);
    for (level, name) in [
        (10, "volume-low"),
        (35, "volume-medium"),
        (80, "volume-high"),
    ] {
        case!(name, |m: &mut Model| m.audio.volume_percent = level);
    }
    case!("volume-muted", |m: &mut Model| m.audio.muted = true);
    case!("notifications-dot", |m: &mut Model| m.notifications.alt =
        "notification".into());
    case!("notifications-dnd", |m: &mut Model| m.notifications.alt =
        "dnd-notification".into());
    for signal in [0, 25, 50, 75, 100] {
        for secure in [false, true] {
            case!(
                &format!("wifi-{signal}{}", if secure { "-secure" } else { "" }),
                |m: &mut Model| {
                    m.network.wired_connected = false;
                    m.network.connected = true;
                    m.network.present = true;
                    m.network.signal = signal;
                    m.network.security = if secure { "wpa2" } else { "open" }.into();
                    m.stats.iface = Some("wlan0".into());
                }
            );
        }
    }
    case!("wifi-disconnected", |m: &mut Model| {
        m.network.present = true;
        m.network.wired_connected = false;
        m.network.connected = false;
    });
    case!("wifi-radio-off", |m: &mut Model| {
        m.network.present = true;
        m.network.wired_connected = false;
        m.network.enabled = false;
    });
    case!("wifi-hardware-blocked", |m: &mut Model| {
        m.network.present = true;
        m.network.hardware_enabled = false;
        m.network.wired_connected = false;
    });
    case!("network-hidden", |m: &mut Model| m.network_widget_visible =
        Some(false));
    case!("network-offline", |m: &mut Model| {
        m.stats.iface = None;
        m.network.default_iface = None;
        m.network.wired_connected = false;
        m.network_widget_visible = Some(true);
    });
    case!("wifi-hover", |m: &mut Model| m.hovered = Some(Target::Wifi));
    case!("viewflow-hover", |m: &mut Model| m.hovered =
        Some(Target::Viewflow));
    case!("narrow", |m: &mut Model| m.output.width = 1280.);
    case!("tray-overflow", |m: &mut Model| m.output.width = 800.);
    case!("tray-squeezed", |m: &mut Model| {
        m.output.width = 1500.;
        m.tray.items.extend(m.tray.items.clone());
    });
    let mut model = m.clone();
    let mut motion = Motion::new(&model);
    model.fullscreen = true;
    motion.sync(&model, Duration::ZERO);
    for t in [0, 100, 200, 350, 700] {
        motion.advance(&mut model, Duration::from_millis(t));
        fixtures.push(bar_fixture(
            &format!("fullscreen-enter-{t:04}"),
            model.clone(),
        ));
    }
    model.fullscreen = false;
    motion.sync(&model, Duration::from_millis(1000));
    for t in [0, 100, 200, 350, 700] {
        motion.advance(&mut model, Duration::from_millis(1000 + t));
        fixtures.push(bar_fixture(
            &format!("fullscreen-exit-{t:04}"),
            model.clone(),
        ));
    }
    case!("calendar-host", |m: &mut Model| {
        m.calendar_open = true;
        m.calendar_presented = true;
        m.calendar_progress = 1.;
        m.calendar_width = 520.;
        m.calendar_height = 500.;
        m.calendar_radius = 30.;
    });
    case!("island-expanded-host", |m: &mut Model| m.island =
        IslandGeometry {
            width: 392.,
            height: 176.,
            attached_center_offset: 0.,
            expanded: true
        });
    for (corner, name) in [
        (Corner::TopLeft, "corner-top-left"),
        (Corner::TopRight, "corner-top-right"),
        (Corner::BottomLeft, "corner-bottom-left"),
        (Corner::BottomRight, "corner-bottom-right"),
    ] {
        fixtures.push(Fixture {
            name: name.into(),
            node: surfaces::corner_node_at_scale(corner, scale),
            width: 27.,
            height: 27.,
        });
    }
    let mut menu_model = m.clone();
    menu_model.output.width = 800.;
    menu_model.output.height = 600.;
    menu_model.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
        item_key: "tray-0".into(),
        revision: 1,
        root: TrayMenuEntry {
            children: menu_entries(),
            ..Default::default()
        },
    }];
    let mut popup = Popups::default();
    popup.command(
        &menu_model,
        PopupCommand::Menu {
            item: "tray-0".into(),
            anchor: Rect::new(500., 10., 18., 38.),
            source: Target::Tray("tray-0".into()),
        },
        Duration::ZERO,
    );
    popup.update(&menu_model, Duration::ZERO);
    popup.update(&menu_model, Duration::from_millis(16));
    for t in [0, 20, 60, 120, 200] {
        popup.update(&menu_model, Duration::from_millis(16 + t));
        fixtures.push(popup_fixture(
            &format!("menu-open-{t:04}"),
            &menu_model,
            &popup,
            Kind::Menu,
        ));
    }
    popup.menu.hovered = Some(2);
    fixtures.push(popup_fixture("menu-hover", &menu_model, &popup, Kind::Menu));
    popup.menu.stack = vec![6];
    popup.update(&menu_model, Duration::from_millis(300));
    fixtures.push(popup_fixture(
        "menu-submenu",
        &menu_model,
        &popup,
        Kind::Menu,
    ));
    popup.menu.stack.clear();
    popup.update(&menu_model, Duration::from_millis(400));
    popup.escape(Kind::Menu, Duration::from_millis(500));
    for t in [0, 45, 90, 150, 180, 200] {
        popup.update(&menu_model, Duration::from_millis(500 + t));
        fixtures.push(popup_fixture(
            &format!("menu-close-{t:04}"),
            &menu_model,
            &popup,
            Kind::Menu,
        ));
    }
    for n in [1, 6, 7] {
        let mut model = m.clone();
        model.output.width = 400.;
        model.output.height = 600.;
        model.tray.items = (0..n)
            .map(|i| TrayItem {
                key: format!("item-{i}"),
                id: format!("Item{i}"),
                ..Default::default()
            })
            .collect();
        let mut popup = Popups::default();
        popup.command(
            &model,
            PopupCommand::Overflow {
                anchor: Rect::new(330., 10., 22., 38.),
                source: Target::Overflow,
            },
            Duration::ZERO,
        );
        fixtures.push(popup_fixture(
            &format!("overflow-{n}"),
            &model,
            &popup,
            Kind::Overflow,
        ));
    }
    for (mode, name) in [
        (BatteryMode::Charging, "battery-charging"),
        (BatteryMode::Discharging, "battery-critical"),
        (BatteryMode::Full, "battery-full"),
    ] {
        let mut model = m.clone();
        model.output.width = 800.;
        model.output.height = 600.;
        model.power.battery_present = true;
        model.power.on_battery = mode == BatteryMode::Discharging;
        model.power.battery_percent = Some(if mode == BatteryMode::Discharging {
            15.
        } else {
            75.
        });
        model.battery.available = true;
        model.battery.capacity_percent = model.power.battery_percent;
        model.battery.mode = mode;
        model.battery.power_w = Some(if model.power.on_battery { -4.23 } else { 8.12 });
        model.battery.average_power_w = Some(if model.power.on_battery { -5.12 } else { 6.32 });
        model.battery.estimate_s = Some(7500.);
        model.battery.estimate_basis = "average".into();
        model.battery.window_s = 1800;
        let mut popup = Popups::default();
        popup.command(
            &model,
            PopupCommand::Battery {
                anchor: Rect::new(560., 10., 70., 38.),
                source: Target::Battery,
            },
            Duration::ZERO,
        );
        popup.update(&model, Duration::ZERO);
        popup.update(&model, Duration::from_millis(16));
        popup.update(&model, Duration::from_millis(236));
        fixtures.push(popup_fixture(name, &model, &popup, Kind::Battery));
    }
    fixtures
}

// Asset I/O belongs only to this offscreen fixture host, never the UI library.
pub fn fixture_assets(dark: bool, scale: f32) -> Assets {
    thread_local! {static CACHE:std::cell::RefCell<std::collections::BTreeMap<(bool,u32),Assets>>=Default::default();}
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((dark, scale.to_bits()))
            .or_insert_with(|| load_fixture_assets(dark, scale))
            .clone()
    })
}
fn load_fixture_assets(dark: bool, scale: f32) -> Assets {
    use hyprv_ui::skia;
    let mut assets = Assets::default();
    // Canonical crop host renders scaled logical windows at effective DPR1.
    // QQuickImagePrivate::currentPix was independently dumped as 32x32 at
    // both capture scales. Production instead bakes sourceSize32 * output DPR.
    let key=if dark {"control-panel-dark"}else{"control-panel-light"};
    assets.images.insert(key.into(),hyprv_ui_bar::baked::image(key,16,1.,"source").unwrap());
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let theme =
        home.join(".local/share/icons")
            .join(if dark { "Fluent-dark" } else { "Fluent-light" });
    let mut names = vec![
        "network-wireless-hardware-disabled".to_string(),
        "network-wireless-disabled".into(),
        "network-wireless-offline".into(),
        "network-wireless-off".into(),
        "network-wireless-disconnected".into(),
        "network-wired".into(),
    ];
    for bucket in [0, 25, 50, 75, 100] {
        for suffix in ["", "-secure"] {
            names.push(format!("nm-signal-{bucket}{suffix}"));
        }
    }
    for name in names {
        let key = if dark {
            format!("{name}-symbolic")
        } else {
            name.clone()
        };
        let path = if dark {
            theme.join("symbolic/status").join(format!("{key}.svg"))
        } else {
            theme.join("24/panel").join(format!("{name}.svg"))
        };
        let Ok(svg) = std::fs::read_to_string(path) else {
            continue;
        };
        let size = if dark { 16. } else { 24. };
        if let Some(image) = hyprv_ui_bar::baked::image(&key, size as u32, scale, "source") {
            assets.images.insert(key, image);
            continue;
        }
        let Some(picture) = view::svg_picture(&svg, size, size) else {
            continue;
        };
        let mut surface = skia::surfaces::raster_n32_premul((
            (size * scale).ceil() as i32,
            (size * scale).ceil() as i32,
        ))
        .unwrap();
        surface.canvas().scale((scale, scale));
        surface.canvas().draw_picture(&picture, None, None);
        assets.images.insert(key, surface.image_snapshot());
    }
    if !dark {
        if let Some(image) = hyprv_ui_bar::baked::image("network-wired-light", 24, scale, "source")
        {
            assets.images.insert("network-wired-light".into(), image);
        }
    }
    let info = skia::ImageInfo::new(
        (2, 2),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Unpremul,
        None,
    );
    let image = skia::images::raster_from_data(
        &info,
        skia::Data::new_copy(&[
            127, 155, 255, 255, 180, 210, 255, 255, 180, 210, 255, 255, 127, 155, 255, 255,
        ]),
        8,
    )
    .unwrap();
    assets.images.insert("menu:tray-0:7".into(), image.clone());
    assets.images.insert("tray-0".into(), image);
    assets
}

pub fn render_all(output: &std::path::Path) -> Result<usize, Box<dyn std::error::Error>> {
    use hyprv_ui::{render::Renderer, scene::Scene};
    std::fs::create_dir_all(output)?;
    let mut renderer = Renderer::new();
    let mut manifest = vec![];
    for dark in [false, true] {
        for scale in [1., 1.5, 2.] {
            for fixture in fixtures(dark, scale) {
                let file = format!(
                    "{}--{}@{scale}.png",
                    fixture.name,
                    if dark { "dark" } else { "light" }
                );
                let scene = Scene::new(fixture.node);
                renderer.png(
                    &scene,
                    fixture.width,
                    fixture.height,
                    scale.into(),
                    output.join(&file),
                )?;
                let bytes = std::fs::read(output.join(&file))?;
                assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
                assert_eq!(
                    u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
                    (fixture.width * scale).ceil() as u32
                );
                assert_eq!(
                    u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
                    (fixture.height * scale).ceil() as u32
                );
                manifest.push(serde_json::json!({"file":file,"width":fixture.width,"height":fixture.height,"scale":scale,"foreign_slots":"empty","status":"RENDERED; QML reference parity pending"}));
            }
        }
    }
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest.len())
}

pub fn crate_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("src/view.rs").exists())
        .expect("bar crate root")
        .to_path_buf()
}
