//! Fixture adapter for QML comparisons; deliberately separate from production UI.
#[path = "../tests/common/mod.rs"]
mod common;
#[path = "reference/host.rs"]
mod host;
#[path = "reference/laptop.rs"]
mod laptop;
// Reuse parity QA's production EGL helper, without depending on its nested workspace.
use hyprv_ui::offscreen;
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, Scene},
    skia,
};
use hyprv_ui_bar::{layout, model::*, motion::Motion, surfaces, view};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn str_value(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn svg_image(path: &Path, size: f32, scale: f32) -> Option<skia::Image> {
    let bytes = std::fs::read(path).ok()?;
    if path.extension()?.to_str()? == "svg" {
        let picture = view::svg_picture(std::str::from_utf8(&bytes).ok()?, size, size)?;
        let mut surface = skia::surfaces::raster_n32_premul((
            (size * scale).ceil() as i32,
            (size * scale).ceil() as i32,
        ))?;
        surface.canvas().scale((scale, scale));
        surface.canvas().draw_picture(&picture, None, None);
        Some(surface.image_snapshot())
    } else {
        skia::Image::from_encoded(skia::Data::new_copy(&bytes))
    }
}
fn fixture_asset_root() -> PathBuf {
    std::env::var_os("HYPRV_BAR_FIXTURE_ASSET_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| common::crate_root().as_path().join("artifacts"))
}
fn state(fixture: &Value, dark: bool, scale: f32, name: &str) -> Model {
    let mut m = common::baseline();
    m.theme.dark = dark;
    m.output.scale = scale;
    // DynamicIsland's idle Row uses real font advances, not the approximate
    // fallback model size. The narrow-output tray budget depends on every bit.
    let clock=fixture["now"].as_str().and_then(|s|s.get(11..16)).unwrap_or("09:41");
    m.island.width=11.+layout::measure("\u{f023}",16.,700).0+12.
        +layout::measure(clock,16.,700).0+10.+layout::measure("\u{f011}",16.,700).0+13.;
    m.assets = common::fixture_assets(dark, scale);
    if std::env::var_os("HYPRV_BAR_REAL_OUTPUT_DPR").is_some() {
        m.assets.images.remove("control-panel-dark");
        m.assets.images.remove("control-panel-light");
    }
    m.stats.package_power_w = None;
    let services = &fixture["services"];
    let screen = &services["screens"][0];
    m.output.width = screen["width"].as_f64().unwrap_or(3072.) as f32;
    m.output.height = screen["height"].as_f64().unwrap_or(1728.) as f32;
    let hypr = &services["hyprland"];
    let focused = hypr["focusedWorkspace"].as_i64().unwrap_or(1) as i32;
    m.desktop.workspaces = hypr["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let id = v["id"].as_i64().unwrap_or(1) as i32;
            if v["urgent"] == true {
                m.urgent_workspaces.push(id);
            }
            hyprv_svc_sys::hyprland::Workspace {
                id,
                name: str_value(&v["name"]).into(),
                monitor: m.output.name.clone(),
                active: id == focused,
                windows: 0,
                tooltip: String::new(),
                state: String::new(),
            }
        })
        .collect();
    m.desktop.windows[0].title = str_value(&hypr["activeToplevel"]["title"]).into();
    m.audio.volume_percent =
        (services["audio"]["volume"].as_f64().unwrap_or(0.35) * 100.).round() as i32;
    m.audio.muted = services["audio"]["muted"] == true;
    m.tray.items = services["tray"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let key = str_value(&v["id"]).to_string();
            let prepared = fixture_asset_root().join(format!(
                "theme-icons/{}@{scale}{}/{}.png",
                if dark { "dark" } else { "light" },
                if name == "network-offline" && scale == 2. {
                    "-initial"
                } else {
                    ""
                },
                str_value(&v["icon"])
            ));
            if prepared.exists() {
                if let Some(image) = svg_image(&prepared, 18., scale) {
                    m.assets.images.insert(key.clone(), image);
                }
            } else {
                // Unresolvable theme name: Quickshell serves its missing-icon checker
                // (request in device px; the -initial fixture resolved at DPR 1).
                let initial = name == "network-offline" && scale == 2.;
                let request = if initial { 18 } else { (18. * scale) as u32 };
                let (side, px) = hyprv_widgets::icon_theme::missing_icon(request);
                let info = skia::ImageInfo::new(
                    (side as i32, side as i32),
                    skia::ColorType::RGBA8888,
                    skia::AlphaType::Premul,
                    None,
                );
                if let Some(image) = skia::images::raster_from_data(
                    &info,
                    skia::Data::new_copy(&px),
                    side as usize * 4,
                ) {
                    m.assets.images.insert(key.clone(), image);
                }
            }
            TrayItem {
                key: key.clone(),
                id: key,
                title: str_value(&v["title"]).into(),
                icon: str_value(&v["icon"]).into(),
                ..Default::default()
            }
        })
        .collect();
    let mut previous_network: Option<(f64, f64, f64)> = None;
    if let Some(processes) = fixture["processes"].as_array() {
        for process in processes {
            let command = str_value(&process["match"]);
            if command.contains("media-status") {
                let text = str_value(&process["stdout"]);
                m.media.playing = text.lines().any(|l| l == "playing=true");
                m.media.available = text.lines().any(|l| l == "available=true");
            }
            if let Some(lines) = process["lines"].as_array() {
                for line in lines {
                    let Ok(v) = serde_json::from_str::<Value>(str_value(&line["data"])) else {
                        continue;
                    };
                    if command.contains("swaync") {
                        m.notifications.alt = str_value(&v["alt"]).into();
                    }
                    if v["type"] == "wifi" {
                        let w = &v["data"];
                        m.network.present = w["present"] == true;
                        m.network.connected = w["connected"] == true;
                        m.network.enabled = w["enabled"] == true;
                        m.network.hardware_enabled = w["hardwareEnabled"] == true;
                        m.network.signal = w["signal"].as_i64().unwrap_or(0) as u8;
                        m.network.security = str_value(&w["security"]).into();
                    }
                    if v["type"] == "system" {
                        let data = str_value(&v["data"]);
                        let section = |name: &str| {
                            data.split(name)
                                .nth(1)
                                .and_then(|s| s.strip_prefix('\n'))
                                .and_then(|s| s.split("\n__").next())
                                .unwrap_or("")
                        };
                        m.stats.package_power_w = section("__CPU_POWER__").trim().parse().ok();
                        m.stats.temperature_c = section("__TEMP__")
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .map(|v| v / 1000.);
                        m.stats.iface = section("__ROUTE__")
                            .lines()
                            .skip(1)
                            .find_map(|l| l.split_whitespace().next().map(Into::into));
                        m.network.default_iface = m.stats.iface.as_deref().map(str::to_owned);
                        if let Some(iface) = m.stats.iface.as_deref() {
                            if let Some(values) = section("__NET__")
                                .lines()
                                .find_map(|line| line.trim().strip_prefix(&format!("{iface}:")))
                            {
                                let values: Vec<f64> = values
                                    .split_whitespace()
                                    .filter_map(|v| v.parse().ok())
                                    .collect();
                                if values.len() > 8 {
                                    let (rx, tx, at) =
                                        (values[0], values[8], line["at"].as_f64().unwrap_or(0.));
                                    if let Some((old_rx, old_tx, old_at)) = previous_network {
                                        let elapsed = ((at - old_at) / 1000.).max(0.001);
                                        m.stats.rx_bytes_per_s =
                                            Some((rx - old_rx).max(0.) / elapsed);
                                        m.stats.tx_bytes_per_s =
                                            Some((tx - old_tx).max(0.) / elapsed);
                                    }
                                    previous_network = Some((rx, tx, at));
                                }
                            }
                        }

                        m.network.wired_connected =
                            m.stats.iface.as_ref().is_some_and(|i| !i.starts_with("wl"));
                    }
                }
            }
        }
    }
    if let Some(steps) = fixture["steps"].as_array() {
        for step in steps {
            if let Some(text) = step["hover"]["text"].as_str() {
                m.hovered = if let Ok(id) = text.parse() {
                    Some(Target::Workspace(id))
                } else if text.contains('\u{f05b3}') {
                    Some(Target::Viewflow)
                } else {
                    None
                };
            }
        }
    }
    if let Some(steps) = fixture["steps"].as_array() {
        for step in steps {
            let b = &step["set"]["root"]["batteryInfo"];
            if !b.is_object() {
                continue;
            }
            m.battery = BatteryDetail {
                available: b["available"] == true,
                status: str_value(&b["status"]).into(),
                mode: match str_value(&b["mode"]) {
                    "charging" => BatteryMode::Charging,
                    "discharging" => BatteryMode::Discharging,
                    "full" => BatteryMode::Full,
                    "plugged" => BatteryMode::Plugged,
                    _ => BatteryMode::Idle,
                },
                capacity_percent: b["capacity"].as_f64(),
                power_w: b["powerW"].as_f64(),
                average_power_w: b["averagePowerW"].as_f64(),
                samples: b["sampleCount"].as_u64().unwrap_or(0) as u32,
                window_s: b["sampleWindowSeconds"].as_u64().unwrap_or(0) as u32,
                window_complete: b["windowComplete"] == true,
                estimate_s: b["estimateSeconds"].as_f64(),
                estimate_basis: str_value(&b["estimateBasis"]).into(),
                energy_now_wh: b["energyNowWh"].as_f64(),
                energy_full_wh: b["energyFullWh"].as_f64(),
            };
            m.power.battery_present = m.battery.available;
            m.power.battery_percent = m.battery.capacity_percent;
            m.power.on_battery = m.battery.mode == BatteryMode::Discharging;
        }
    }
    m
}
fn menu_entry(
    v: &Value,
    next: &mut i32,
    m: &mut Model,
    key: &str,
    workspace: &Path,
) -> TrayMenuEntry {
    *next += 1;
    let id = *next;
    let icon = str_value(&v["icon"]);
    if let Some(asset) = icon.strip_prefix("@asset:") {
        if let Some(img) = svg_image(
            &{
                let prepared = fixture_asset_root().join(format!(
                    "fixture-icons/{}@{}.png",
                    Path::new(asset).file_stem().unwrap().to_string_lossy(),
                    m.output.scale
                ));
                if prepared.exists() {
                    prepared
                } else {
                    workspace.join("parity/fixtures/assets").join(asset)
                }
            },
            16.,
            m.output.scale,
        ) {
            let img=std::env::var_os("HYPRV_BAR_MENU_PROBE_IMAGE").and_then(|p|std::fs::read(p).ok()).and_then(|b|skia::Image::from_encoded(skia::Data::new_copy(&b))).unwrap_or(img);
            m.assets.images.insert(format!("menu:{key}:{id}"), img);
        }
    }
    let children = v["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| menu_entry(v, next, m, key, workspace))
                .collect()
        })
        .unwrap_or_default();
    TrayMenuEntry {
        id,
        text: str_value(&v["text"]).into(),
        enabled: v["enabled"] != false,
        visible: v["visible"] != false,
        is_separator: v["isSeparator"] == true,
        button_type: match v["buttonType"].as_i64() {
            Some(1) => ButtonType::CheckBox,
            Some(2) => ButtonType::RadioButton,
            _ => ButtonType::None,
        },
        check_state: match v["checkState"].as_i64() {
            Some(1) => CheckState::PartiallyChecked,
            Some(2) => CheckState::Checked,
            _ => CheckState::Unchecked,
        },
        has_children: v["entries"].is_array(),
        icon_name: icon.into(),
        children,
        ..Default::default()
    }
}
fn popup(fixture: &Value, entry: &Value, m: &mut Model, workspace: &Path) -> Option<Node> {
    use hyprv_ui_bar::{PopupCommand, PopupKind, Popups};
    let name = str_value(&entry["state"]);
    let mut p = Popups::default();
    let mut settled_at = Duration::from_millis(1016);
    let mut kind = None;
    if name.starts_with("tray-menu") {
        let item = fixture["services"]["tray"]
            .as_array()?
            .iter()
            .find(|v| v["menu"].is_object())?;
        let key = str_value(&item["id"]);
        let mut id = 0;
        let children = item["menu"]["entries"]
            .as_array()?
            .iter()
            .map(|v| menu_entry(v, &mut id, m, key, workspace))
            .collect();
        m.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
            item_key: key.into(),
            revision: 1,
            root: TrayMenuEntry {
                children,
                ..Default::default()
            },
        }];
        let source = Target::Tray(key.into());
        let anchor = layout::compute(m, &m.theme).target(&source)?;
        p.command(
            m,
            PopupCommand::Menu {
                item: key.into(),
                anchor,
                source,
            },
            Duration::ZERO,
        );
        kind = Some(PopupKind::Menu);
        p.update(m, Duration::ZERO);
        p.update(m, Duration::from_millis(17));
        if name != "tray-menu-anim" {
            p.update(m, Duration::from_millis(1017));
        }
        if name == "tray-menu-submenu" {
            // Replay the fixture click. In QML the Repeater destroys the old
            // MouseArea and the new page waits for a fresh hover event.
            use hyprv_widgets::controls::{Button, PointerEvent};
            let tree = p.build(m, &m.theme, PopupKind::Menu);
            fn text_center(n: &Node, origin: (f32, f32), label: &str) -> Option<(f32, f32)> {
                let origin = (origin.0 + n.frame.x, origin.1 + n.frame.y);
                if matches!(&n.content, Content::Text(t) if t.text == label) {
                    return Some((
                        (origin.0 + n.frame.width / 2.).floor(),
                        (origin.1 + n.frame.height / 2.).floor(),
                    ));
                }
                n.children
                    .iter()
                    .find_map(|c| text_center(c, origin, label))
            }
            let label = fixture["steps"]
                .as_array()?
                .iter()
                .find_map(|s| s["click"]["text"].as_str())?;
            let (x, y) = text_center(&tree, (0., 0.), label)?;
            for event in [
                PointerEvent::Move { x, y },
                PointerEvent::Press {
                    x,
                    y,
                    button: Button::Left,
                },
                PointerEvent::Release {
                    x,
                    y,
                    button: Button::Left,
                },
            ] {
                p.pointer(m, PopupKind::Menu, event, Duration::from_millis(1017));
            }
            settled_at = Duration::from_millis(1017);
            // The fixture may move onto a delegate created by the page change.
            // Resolve its current geometry after each advance, as the Qt host does.
            for step in fixture["steps"].as_array()?.iter().skip_while(|s| s["click"]["text"].as_str()!=Some(label)).skip(1) {
                if let Some(label)=step["hover"]["text"].as_str() {
                    let tree=p.build(m,&m.theme,PopupKind::Menu);
                    let (x,y)=text_center(&tree,(0.,0.),label)?;
                    p.pointer(m,PopupKind::Menu,PointerEvent::Move {x,y},settled_at);
                }
                if let Some(ms)=step["advance"].as_f64() {
                    settled_at+=Duration::from_secs_f64(ms.max(0.)/1000.);
                    p.update(m,settled_at);
                }
            }
        }
        if name == "tray-menu-hover" {
            p.menu.hovered = p
                .entries(m)
                .iter()
                .find(|e| e.text == "Deafen")
                .map(|e| e.id);
        }
    } else if name.starts_with("tray-overflow") {
        let source = Target::Overflow;
        let anchor = layout::compute(m, &m.theme).target(&source)?;
        p.command(m, PopupCommand::Overflow { anchor, source }, Duration::ZERO);
        kind = Some(PopupKind::Overflow);
    } else if name.starts_with("battery-") {
        let source = Target::Battery;
        let anchor = layout::compute(m, &m.theme).target(&source)?;
        p.command(m, PopupCommand::Battery { anchor, source }, Duration::ZERO);
        kind = Some(PopupKind::Battery);
        p.update(m, Duration::ZERO);
        p.update(m, Duration::from_millis(16));
    }
    let kind = kind?;
    let frame = str_value(&entry["frame"]);
    let t = frame.rsplit('t').next().and_then(|v| v.parse::<u64>().ok());
    if name == "tray-menu-anim" {
        if frame.starts_with("close") {
            p.update(m, Duration::from_millis(1016));
            p.escape(kind, Duration::from_millis(1100));
            p.update(m, Duration::from_millis(1100 + t.unwrap_or(0)));
        } else {
            p.update(m, Duration::from_millis(t.unwrap_or(0).max(17)));
        }
    } else {
        p.update(m, settled_at);
    }
    Some(p.build(m, &m.theme, kind))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root_path = common::crate_root();
    let crate_dir = root_path.as_path();
    let workspace = crate_dir.parent().unwrap().parent().unwrap();
    let reference = std::env::var_os("HYPRV_BAR_REFERENCE_SNAPSHOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("parity/reference/bar"));
    let fixture_directory = if reference.join("fixtures").exists() {
        reference.join("fixtures")
    } else {
        workspace.join("parity/fixtures/bar")
    };
    let output = std::env::var_os("HYPRV_BAR_ACTUAL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate_dir.join("artifacts/reference-actual"));
    std::fs::create_dir_all(&output)?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(reference.join("manifest.json"))?)?;
    let filter = std::env::args().nth(1);
    let mut renderer = Renderer::new();
    renderer.set_rect_mode(
        if std::env::var("HYPRV_BAR_RECT_MODE").as_deref() == Ok("skia") {
            hyprv_ui::qt_rect::RectMode::Skia
        } else {
            hyprv_ui::qt_rect::RectMode::QtQuick
        },
    );
    let mut exported = vec![];
    let skipped: Vec<String> = vec![];
    let stress_replays = std::env::var("HYPRV_BAR_STRESS_REPLAYS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let oracle = std::env::var_os("HYPRV_BAR_STRESS_ORACLE").map(PathBuf::from);
    let mut stress_bad = vec![];
    for pass in 0..stress_replays {
        for entry in manifest["images"].as_array().ok_or("missing images")? {
            let name = str_value(&entry["state"]);
            if filter
                .as_ref()
                .is_some_and(|f| !f.split(',').any(|f| f == name))
            {
                continue;
            }
            let file = str_value(&entry["file"]);
            let fixture_path = entry["fixture"]
                .as_str()
                .filter(|f| f.starts_with("laptop/"))
                .map(|f| workspace.join("parity").join(f))
                .unwrap_or_else(|| fixture_directory.join(format!("{name}.json")));
            let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path)?)?;
            let scale = entry["scale"].as_f64().unwrap_or(1.) as f32;
            let mut m = state(&fixture, entry["theme"] == "dark", scale, name);
            let mut tooltip_age = None;
            if laptop::is_laptop(&fixture) {
                let (next, hover) = laptop::replay(&fixture, entry, m);
                m = next;
                tooltip_age = hover;
            }
            let _ = tooltip_age;
            let composed_host = if laptop::is_laptop(&fixture) {
                laptop::mount_island(&fixture, entry, &mut m, workspace)
            } else {
                host::mount(&fixture, name, &mut m, workspace)?
            };
            // These captures are pixel-identical to 75-secure: their hover did not
            // activate. Match the observed state; retain real hover in production.
            if name == "network-wifi-hover" {
                m.hovered = None;
            }
            if name == "viewflow-hover" {
                m.hovered = Some(Target::Viewflow);
            }
            if name == "fullscreen-slide" {
                let frame = str_value(&entry["frame"]);
                let t = frame
                    .rsplit('t')
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                let mut motion = Motion::new(&m);
                m.fullscreen = true;
                motion.sync(&m, Duration::ZERO);
                if frame.starts_with("show") {
                    motion.advance(&mut m, Duration::from_millis(900));
                    m.fullscreen = false;
                    motion.sync(&m, Duration::from_millis(1000));
                    motion.advance(&mut m, Duration::from_millis(1000 + t));
                } else {
                    motion.advance(&mut m, Duration::from_millis(t));
                }
            }
            let crop = &entry["crop_logical"];
            let [x, y, w, h] =
                std::array::from_fn::<_, 4, _>(|i| crop[i].as_f64().unwrap_or(0.) as f32);
            let mut root = Node::new(Rect::new(0., 0., w, h), Content::Group);
            let mut full = Node::new(
                Rect::new(-x, -y, m.output.width, m.output.height),
                Content::Group,
            );
            full.add(view::build(&m, &m.theme));
            for (_, overlay) in surfaces::corners(&m) {
                full.add(overlay);
            }
            if let Some(popup) = if laptop::is_laptop(&fixture) {
                laptop::battery_popup(&fixture, &mut m)
            } else {
                popup(&fixture, entry, &mut m, workspace)
            } {
                full.add(popup);
            }
            if let Some(tip) = hyprv_ui_bar::tooltip::build(&m, &m.theme) {
                full.add(tip);
            }
            root.add(full);
            // Private diagnostic only: test QQuickImage's real-output SVG source
            // raster size without changing embedded runtime assets or fixtures.
            if let Some(path) = std::env::var_os("HYPRV_BAR_CONTROL_PROBE_PNG") {
                let key=if m.theme.dark {"control-panel-dark"}else{"control-panel-light"};
                let original=hyprv_ui_bar::baked::image(key,16,scale,"source").unwrap();
                let path=path.to_string_lossy().replace("{theme}",if m.theme.dark {"dark"}else{"light"});
                let replacement=skia::Image::from_encoded(skia::Data::new_copy(&std::fs::read(path)?)).ok_or("probe PNG")?;
                fn replace(n:&mut Node,id:u32,image:&skia::Image){
                    if matches!(&n.content,Content::Image(i) if i.unique_id()==id){n.content=Content::Image(image.clone());}
                    for c in &mut n.children {replace(c,id,image)}
                }
                replace(&mut root,original.unique_id(),&replacement);
            }
            if scale==2. {if let Some(path)=std::env::var_os("HYPRV_BAR_CORNER_PROBE_IMAGE") {
                let original=hyprv_ui_bar::baked::image("corner-top-right",27,scale,"000000ff").unwrap();
                let replacement=skia::Image::from_encoded(skia::Data::new_copy(&std::fs::read(path)?)).ok_or("corner probe PNG")?;
                fn replace(n:&mut Node,id:u32,image:&skia::Image){if matches!(&n.content,Content::Image(i)if i.unique_id()==id){n.content=Content::Image(image.clone());}for c in &mut n.children{replace(c,id,image)}}
                replace(&mut root,original.unique_id(),&replacement);
            }}
            if std::env::var_os("HYPRV_BAR_HIDE_TEXT").is_some() {
                hide_text(&mut root);
            }
            let mut semantic = vec![];
            collect(&root, 0., 0., &mut semantic);
            let raw_full_target=std::env::var_os("HYPRV_BAR_FULL_TARGET").is_some();
            // Qt renders the original output viewport before cropping. Building
            // a crop-sized projection changes batch vertex float precision.
            let full_target=offscreen::enabled() || raw_full_target;
            let (rw,rh)=if full_target {
                root.frame.width=m.output.width;root.frame.height=m.output.height;
                root.children[0].frame.x=0.;root.children[0].frame.y=0.;
                (m.output.width,m.output.height)
            }else{(w,h)};
            let scene = Scene::new(root);
            if let Some(png) =
                offscreen::render_png_if_enabled(&mut renderer, &scene, rw, rh, scale.into())?
            {
                let png=if full_target && !raw_full_target {
                    let image=skia::Image::from_encoded(skia::Data::new_copy(&png)).ok_or("full GPU PNG")?;
                    let rect=skia::IRect::from_xywh((x*scale).round()as i32,(y*scale).round()as i32,(w*scale).round()as i32,(h*scale).round()as i32);
                    let cropped=image.make_subset(None,rect,Default::default()).ok_or("GPU crop")?;
                    hyprv_ui::capture::qt_png(&cropped)?
                }else{png};
                std::fs::write(output.join(file), png)?;
            } else if std::env::var_os("HYPRV_BAR_PROBE_F16").is_some() {
                // Diagnostic only: distinguish CPU lowp blending from glyph coverage.
                // The production/default replay remains canonical Renderer::png.
                let size = hyprv_ui::render::physical_size(w, h, scale.into())?;
                let info = skia::ImageInfo::new(
                    size,
                    skia::ColorType::RGBAF16,
                    skia::AlphaType::Premul,
                    None,
                );
                let mut surface = skia::surfaces::raster(&info, None, None).ok_or("F16 surface")?;
                renderer.paint(
                    surface.canvas(),
                    &scene,
                    scale.into(),
                    Rect::new(0., 0., w, h),
                )?;
                let bytes = surface
                    .image_snapshot()
                    .encode(None, skia::EncodedImageFormat::PNG, None)
                    .ok_or("F16 PNG")?;
                std::fs::write(output.join(file), bytes.as_bytes())?;
            } else {
                renderer.png(&scene, w, h, scale.into(), output.join(file))?;
            }
            if let Some(oracle) = &oracle {
                if std::fs::read(output.join(file))? != std::fs::read(oracle.join(file))? {
                    let bad = format!("stress-{pass}-{file}");
                    std::fs::copy(output.join(file), output.join(&bad))?;
                    stress_bad.push(serde_json::json!({"pass":pass,"file":file,"capture":bad}));
                }
            }
            let l = layout::compute(&m, &m.theme);
            let foreign = if composed_host { [0.;4] } else { [l.center.x-x,l.center.y+l.content_y-y,l.center.width,l.center.height] };
            exported.push(serde_json::json!({"file":file,"foreign_island_local_rect":foreign,"scale":scale,"crop":[x,y,w,h],"semantic":semantic}));
        }
        eprintln!(
            "corpus stress pass {}/{} mismatches={}",
            pass + 1,
            stress_replays,
            stress_bad.len()
        );
    }
    if stress_replays > 1 {
        std::fs::write(
            output.join("stress.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"replays":stress_replays,"frames":exported.len(),"mismatches":stress_bad}),
            )?,
        )?;
        if !stress_bad.is_empty() {
            return Err("corpus stress mismatch".into());
        }
        exported.truncate(exported.len() / stress_replays);
    }
    std::fs::write(
        output.join("coverage.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"exported":exported,"skipped_foreign_contents":skipped,"note":"Host states compose production calendar/island builders without a foreign-content mask. Other states exclude the separately verified island slot."}),
        )?,
    )?;
    println!(
        "Exported {} QML-state-matched images; skipped {} foreign-content views",
        exported.len(),
        skipped.len()
    );
    Ok(())
}

fn collect(n: &Node, parent_x: f32, parent_y: f32, out: &mut Vec<Value>) {
    if !n.visible || n.opacity == 0. {
        return;
    }
    let (x, y) = (parent_x + n.frame.x, parent_y + n.frame.y);
    let category = match &n.content {
        Content::Rect(r) => {
            out.push(serde_json::json!({"category":"rect_edge_aa","bounds":[x,y,n.frame.width,n.frame.height],"radius":r.radii[0],"border":r.border.as_ref().map_or(0.,|b|b.width)}));
            None
        }
        Content::Text(_) => Some("text"),
        Content::Image(_) if n.frame.width == 27. && n.frame.height == 27. => Some("rect_edge_aa"),
        Content::Image(_) | Content::Picture(_) | Content::Svg(_) => Some("svg_icons"),
        Content::Path { .. } => Some("rect_edge_aa"),
        _ => None,
    };
    if let Some(category) = category {
        out.push(
            serde_json::json!({"category":category,"bounds":[x,y,n.frame.width,n.frame.height]}),
        );
    }
    for child in &n.children {
        collect(child, x, y, out);
    }
}
fn hide_text(n: &mut Node) {
    if matches!(n.content, Content::Text(_)) {
        n.content = Content::Group;
    }
    for child in &mut n.children {
        hide_text(child);
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    #[test]
    fn host_fixtures_mount_production_contents() {
        let workspace=common::crate_root().parent().unwrap().parent().unwrap().to_path_buf();
        for name in ["calendar-host","island-expanded-host"] {
            let fixture:Value=serde_json::from_slice(&std::fs::read(workspace.join(format!("parity/fixtures/bar/{name}.json"))).unwrap()).unwrap();
            for dark in [false,true] {for scale in [1.,2.] {
                let mut m=state(&fixture,dark,scale,name);
                assert!(host::mount(&fixture,name,&mut m,&workspace).unwrap());
                if name=="calendar-host" {
                    assert_eq!((m.calendar_width,m.calendar_height),(520.,500.));
                    assert_eq!(m.calendar_progress,1.);
                    assert!(m.slots.calendar.is_some());
                } else {
                    assert!(m.island.expanded);
                    assert_eq!((m.island.width,m.island.height),(392.,176.));
                    assert!(m.slots.island.is_some());
                }
                assert!(!view::build(&m,&m.theme).children.is_empty());
            }}
        }
    }
    #[test]
    fn overflow_budget_uses_exact_idle_island_font_width() {
        let workspace=common::crate_root().parent().unwrap().parent().unwrap().to_path_buf();
        let fixture:Value=serde_json::from_slice(&std::fs::read(workspace.join("parity/fixtures/bar/tray-overflow-2.json")).unwrap()).unwrap();
        let model=state(&fixture,true,1.,"tray-overflow-2");
        assert_eq!(model.island.width,120.15625);
        let layout=layout::compute(&model,&model.theme);
        assert_eq!(layout.pills[2].x,567.65625,"independent Qt GL upload");
    }

    #[test]
    fn submenu_replays_hover_on_new_page_delegate() {
        let workspace=common::crate_root().parent().unwrap().parent().unwrap().to_path_buf();
        let fixture:Value=serde_json::from_slice(&std::fs::read(workspace.join("parity/fixtures/bar/tray-menu-submenu.json")).unwrap()).unwrap();
        let mut model=state(&fixture,true,1.,"tray-menu-submenu");
        let entry=serde_json::json!({"state":"tray-menu-submenu","frame":""});
        let tree=popup(&fixture,&entry,&mut model,&workspace).unwrap();
        fn row_color(node:&Node,label:&str)->Option<hyprv_ui::animation::Color> {
            if node.children.iter().any(|n|matches!(&n.content,Content::Text(t)if t.text==label)) {
                if let Content::Rect(r)=&node.content {if let hyprv_ui::scene::Fill::Solid(color)=r.fill {return Some(color)}}
            }
            node.children.iter().find_map(|n|row_color(n,label))
        }
        let color=row_color(&tree,"Do Not Disturb").expect("submenu delegate");
        assert!((color.a-0.10).abs()<1e-6,"fixture hover must paint DND row");
        let color=row_color(&tree,"Online").expect("neighbor delegate");
        assert_eq!(color.a,0.,"unhovered neighbor remains transparent");
    }
}
