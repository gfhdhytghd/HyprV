//! Laptop-profile fixtures (parity/laptop, origin/laptop@24972f7): replay the QML virtual
//! timeline (settle, then steps) against the production Motion/Model.
use super::*;
use hyprv_ui_bar::ipc::ToggleGate;

pub fn is_laptop(fixture: &Value) -> bool {
    str_value(&fixture["source_ref"]).starts_with("origin/laptop")
}

/// parity-host: `advance(settle_ms)`, one pointer tick, then the steps.
const TICK_MS: f64 = 1000. / 60.;

#[derive(Default)]
struct Snapshots {
    minimal: usize,
    full: usize,
    prev_cpu: Option<(f64, f64)>,
    prev_net: Option<(f64, f64, f64)>,
}

fn section<'a>(data: &'a str, name: &str) -> Option<&'a str> {
    let start = data.find(&format!("{name}\n"))? + name.len() + 1;
    let rest = &data[start..];
    Some(rest.split("\n__").next().unwrap_or(rest))
}

impl Snapshots {
    /// One PollCommand run of `system-snapshot.sh` (laptop shell.qml parseSystemSnapshot).
    fn run(&mut self, fixture: &Value, full: bool, at_ms: f64, m: &mut Model) {
        let key = if full { "system-snapshot.sh --full" } else { "system-snapshot.sh --minimal" };
        let Some(entry) = fixture["processes"]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["match"] == key))
        else {
            return;
        };
        let counter = if full { &mut self.full } else { &mut self.minimal };
        let seq = entry["sequence"].as_array().cloned().unwrap_or_default();
        let data = seq
            .get((*counter).min(seq.len().saturating_sub(1)))
            .and_then(|v| v["stdout"].as_str())
            .or_else(|| entry["stdout"].as_str())
            .unwrap_or("")
            .to_string();
        *counter += 1;
        if let Some(cpu) = section(&data, "__STAT__")
            .and_then(|s| s.lines().find(|l| l.starts_with("cpu ")))
        {
            let v: Vec<f64> = cpu.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
            let total: f64 = v.iter().sum();
            let idle = v.get(3).copied().unwrap_or(0.) + v.get(4).copied().unwrap_or(0.);
            if let Some((pt, pi)) = self.prev_cpu {
                let (dt, di) = (total - pt, idle - pi);
                if dt > 0. {
                    m.stats.cpu_percent = Some(((dt - di) / dt * 100.).clamp(0., 100.));
                }
            }
            self.prev_cpu = Some((total, idle));
        }
        if let Some(mem) = section(&data, "__MEM__") {
            let field = |k: &str| {
                mem.lines()
                    .find_map(|l| l.strip_prefix(k))
                    .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            };
            if let (Some(total), Some(avail)) = (field("MemTotal:"), field("MemAvailable:")) {
                if total > 0. {
                    m.stats.memory_percent = Some((total - avail) / total * 100.);
                }
            }
        }
        if let Some(t) = section(&data, "__TEMP__").and_then(|s| s.trim().parse::<f64>().ok()) {
            m.stats.temperature_c = Some(if t > 1000. { t / 1000. } else { t });
        }
        let power = section(&data, "__CPU_POWER__")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|w| w.is_finite() && *w >= 0.);
        m.stats.package_power_w = power;
        if let Some(w) = power {
            m.last_package_power_w = w;
        }
        m.stats.iface = section(&data, "__ROUTE__").and_then(|r| {
            r.lines()
                .skip(1)
                .find(|l| l.split_whitespace().nth(1) == Some("00000000"))
                .and_then(|l| l.split_whitespace().next().map(Into::into))
        });
        m.network.default_iface = m.stats.iface.as_deref().map(str::to_owned);
        match (section(&data, "__NET__"), m.stats.iface.clone()) {
            (Some(net), Some(iface)) => {
                if let Some(values) = net
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(&format!("{iface}:")))
                {
                    let v: Vec<f64> = values.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                    if v.len() > 8 {
                        if let Some((rx, tx, at)) = self.prev_net {
                            let s = ((at_ms - at) / 1000.).max(0.001);
                            m.stats.rx_bytes_per_s = Some((v[0] - rx).max(0.) / s);
                            m.stats.tx_bytes_per_s = Some((v[8] - tx).max(0.) / s);
                        }
                        self.prev_net = Some((v[0], v[8], at_ms));
                    }
                }
            }
            _ => {
                self.prev_net = None;
                m.stats.rx_bytes_per_s = None;
                m.stats.tx_bytes_per_s = None;
            }
        }
    }
}

/// Services the laptop QML reads natively (UPower DisplayDevice, Networking devices).
fn services(fixture: &Value, m: &mut Model) {
    let s = &fixture["services"];
    let up = &s["upower"];
    let d = &up["displayDevice"];
    let present = d["ready"] == true && d["isPresent"] == true && d["isLaptopBattery"] == true;
    let on_battery = up["onBattery"] == true;
    m.power.on_battery = on_battery;
    m.power.battery_present = present;
    m.power.battery_percent = present.then(|| (d["percentage"].as_f64().unwrap_or(0.) * 100.).clamp(0., 100.));
    m.power.battery_mode = hyprv_svc_power::power::battery_mode(present, d["state"].as_u64().unwrap_or(0) as u32, on_battery);
    let w = &s["wifi"];
    let devices = w["devices"].as_array().cloned().unwrap_or_default();
    let wifi = devices.iter().find(|d| d["type"] == "wifi");
    m.network = Default::default();
    m.network.enabled = w["enabled"] != false;
    m.network.hardware_enabled = w["hardwareEnabled"] != false;
    m.network.present = wifi.is_some();
    m.network.connected = wifi.is_some_and(|d| d["connected"] == true);
    if let Some(n) = wifi
        .and_then(|d| d["networks"].as_array())
        .and_then(|n| n.iter().find(|n| n["connected"] == true))
    {
        m.network.signal = (n["signal"].as_f64().unwrap_or(0.) * 100.).round() as u8;
        m.network.security = str_value(&n["security"]).into();
    }
    m.network.wired_connected = devices.iter().any(|d| d["type"] == "wired" && d["connected"] == true);
    for p in fixture["processes"].as_array().into_iter().flatten() {
        if str_value(&p["match"]).contains("swaync") {
            if let Some(v) = p["lines"].as_array().and_then(|l| l.last())
                .and_then(|l| serde_json::from_str::<Value>(str_value(&l["data"])).ok())
            {
                m.notifications.alt = str_value(&v["alt"]).into();
            }
        }
    }
}

/// Replay to the capture of `entry`; returns the model plus the hovered tooltip
/// module's hover age in ms (TextModule tooltipDelay), if any.
pub fn replay(fixture: &Value, entry: &Value, mut m: Model) -> (Model, Option<f64>) {
    m.profile = Profile::Laptop;
    // Laptop references are captured in a real scale-N window (PARITY_SCENE_IN_WINDOW), so
    // the control icon uses the production sourceSize×DPR raster, not the DPR-1 fixture one.
    m.assets.images.remove("control-panel-dark");
    m.assets.images.remove("control-panel-light");
    m.output.name = str_value(&fixture["services"]["screens"][0]["name"]).into();
    m.desktop.monitors[0].name = m.output.name.clone();
    for w in &mut m.desktop.workspaces {
        w.monitor = m.output.name.clone();
    }
    m.stats = Default::default();
    m.last_package_power_w = 0.;
    services(fixture, &mut m);
    let wall = chrono::NaiveDateTime::parse_from_str(str_value(&fixture["now"]), "%Y-%m-%dT%H:%M:%S")
        .unwrap_or_default();
    use chrono::{Datelike, Timelike};
    m.clock = WallClock {
        weekday: wall.weekday().num_days_from_monday() as u8,
        hour: wall.hour() as u8,
        minute: wall.minute() as u8,
    };
    let fullscreen = fixture["services"]["hyprland"]["fullscreen"] == true;
    m.fullscreen = fullscreen;
    m.fullscreen_hide = if fullscreen { 1. } else { 0. };
    m.expanded = false;
    m.reveal = 0.;
    let mut motion = Motion::new(&m);
    let settle = fixture["settle_ms"].as_f64().unwrap_or(1500.);
    // Timeline of (time, event); stats follow detailedStatsVisible = anyBarExpanded.
    let target = {
        let frame = str_value(&entry["frame"]);
        let mut t = settle + TICK_MS;
        let mut found = None;
        for step in fixture["steps"].as_array().into_iter().flatten() {
            if step.get("set").is_some() || step.get("hover").is_some() {
                // instantaneous
            }
            if let Some(a) = step["advance"].as_f64() {
                t += a;
            }
            if step.get("capture").is_some() && frame.is_empty() {
                found = Some(t);
            }
            if let Some(f) = step.get("frames") {
                let name = str_value(&f["name"]);
                let start = t;
                for at in f["at"].as_array().into_iter().flatten() {
                    let at = at.as_f64().unwrap_or(0.);
                    if frame == format!("{name}-t{:04}", at as i64) {
                        found = Some(start + at);
                    }
                    t = t.max(start + at);
                }
            }
        }
        found.unwrap_or(t)
    };
    let mut snaps = Snapshots::default();
    let mut gate = ToggleGate::default();
    let mut detailed = false;
    let mut next_poll = 0.;
    let mut hover_at: Option<f64> = None;
    let mut hover_label: Option<String> = None;
    let ms = |t: f64| Duration::from_secs_f64(t.max(0.) / 1000.);
    // Advance polls (and motion) up to `until`, inclusive.
    let run_to = |until: f64, m: &mut Model, motion: &mut Motion, snaps: &mut Snapshots, detailed: bool, next_poll: &mut f64| {
        while *next_poll <= until {
            snaps.run(fixture, detailed, *next_poll, m);
            *next_poll += if detailed { 1000. } else { 5000. };
        }
        motion.advance(m, ms(until));
    };
    let mut t = settle + TICK_MS;
    run_to(t.min(target), &mut m, &mut motion, &mut snaps, detailed, &mut next_poll);
    'steps: for step in fixture["steps"].as_array().into_iter().flatten() {
        if t > target {
            break;
        }
        if let Some(ipc) = step["set"]["ipc"].as_array() {
            let args: Vec<&str> = ipc.iter().map(str_value).collect();
            if args.len() >= 3 && args[0] == "bar" && args[1] == "toggle" && args[2] == m.output.name
                && gate.allow(t.round() as u64)
            {
                hyprv_ui_bar::ipc::toggle(&mut m);
                motion.sync(&m, ms(t));
                if m.expanded != detailed {
                    detailed = m.expanded;
                    snaps.prev_net = None;
                    next_poll = t;
                }
                run_to(t, &mut m, &mut motion, &mut snaps, detailed, &mut next_poll);
            }
        }
        if let Some(text) = step["hover"]["contains"].as_str() {
            hover_label = Some(text.into());
            hover_at = Some(t);
        }
        if let Some(a) = step["advance"].as_f64() {
            let until = (t + a).min(target);
            run_to(until, &mut m, &mut motion, &mut snaps, detailed, &mut next_poll);
            t += a;
        }
        if let Some(f) = step.get("frames") {
            for at in f["at"].as_array().into_iter().flatten() {
                let when = t + at.as_f64().unwrap_or(0.);
                if when >= target {
                    break 'steps;
                }
            }
        }
    }
    run_to(target, &mut m, &mut motion, &mut snaps, detailed, &mut next_poll);
    let hover = match (&hover_label, hover_at) {
        (Some(label), Some(at)) => {
            let l = layout::compute(&m, &m.theme);
            if let Some(module) = l.modules.iter().find(|n| n.label.contains(label.as_str())) {
                m.hovered = Some(module.target.clone());
                m.tooltip_visible = target - at >= hyprv_ui_bar::tooltip::DELAY.as_millis() as f64 - 1e-6;
                Some(target - at)
            } else {
                None
            }
        }
        _ => None,
    };
    (m, hover)
}

/// Compose the production island (media + agents) at the replayed time, like the
/// desktop `island-expanded-host` mount. Returns true when the slot was mounted.
pub fn mount_island(fixture: &Value, entry: &Value, m: &mut Model, workspace: &Path) -> bool {
    use hyprv_ui_island::{Agent, Inputs, Island, Media, Pointer};
    use std::rc::Rc;
    let processes = fixture["processes"].as_array().cloned().unwrap_or_default();
    let find = |key: &str| processes.iter().find(|p| str_value(&p["match"]).contains(key)).cloned();
    let text = find("media-status.sh").and_then(|p| p["stdout"].as_str().map(str::to_owned)).unwrap_or_default();
    let field = |key: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).unwrap_or("").to_string();
    let media = Media {
        available: field("available") == "true",
        playing: field("playing") == "true",
        title: field("title"),
        artist: field("artist"),
        player: field("player"),
        art_url: field("art_url"),
        position_s: field("position").parse().unwrap_or(0.),
        length_s: field("length").parse().unwrap_or(0.),
    };
    let agents: Vec<Agent> = find("agent-island-watch.py")
        .and_then(|p| p["lines"].as_array()?.last()?["data"].as_str().map(str::to_owned))
        .and_then(|d| serde_json::from_str::<Value>(&d).ok())
        .map(|v| {
            v["sessions"].as_array().into_iter().flatten().map(|s| Agent {
                id: str_value(&s["id"]).into(),
                source: str_value(&s["source"]).into(),
                source_label: str_value(&s["source_label"]).into(),
                title: str_value(&s["title"]).into(),
                project: str_value(&s["project"]).into(),
                status: str_value(&s["status"]).into(),
                current_tool: str_value(&s["current_tool"]).into(),
                detail: str_value(&s["detail"]).into(),
                progress: s["progress"].as_f64(),
            }).collect()
        })
        .unwrap_or_default();
    let cava = find("command -v cava").is_some_and(|p| p["exitCode"].as_i64() == Some(0));
    let spectrum: Vec<f32> = if cava && media.playing {
        find("audio-spectrum.sh")
            .and_then(|p| p["lines"].as_array()?.last()?["data"].as_str().map(str::to_owned))
            .unwrap_or_default()
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect()
    } else {
        vec![]
    };
    let art = media.art_url.strip_prefix("file://@ASSETS@/").and_then(|p| {
        std::fs::read(workspace.join("parity/fixtures/assets").join(p)).ok()
    }).and_then(|b| skia::Image::from_encoded(skia::Data::new_copy(&b)));
    let mut island = Island::new(
        Inputs {
            dark: m.theme.dark,
            now_hh_mm: format!("{:02}:{:02}", m.clock.hour, m.clock.minute),
            art,
            spectrum,
            media,
            agents,
            ..Default::default()
        },
        m.output.width,
        Duration::ZERO,
    );
    island.clip_hover_to_input_region = false;
    let target = m.now;
    let settle = fixture["settle_ms"].as_f64().unwrap_or(1500.) + TICK_MS;
    // Optional hover step (`{"hover":{"text":"09:41","x":-40}}`), as host.rs replays it.
    let mut t = settle;
    let mut hover_at = None;
    for step in fixture["steps"].as_array().into_iter().flatten() {
        if step["hover"]["text"].is_string() {
            hover_at = Some(t);
        }
        if let Some(a) = step["advance"].as_f64() {
            t += a;
        }
    }
    let _ = entry;
    let ms = |t: f64| Duration::from_secs_f64(t.max(0.) / 1000.);
    let end = target.as_secs_f64() * 1000.;
    let mut now = 0.;
    let mut hovered = false;
    while now < end {
        now = (now + TICK_MS).min(end);
        if let Some(h) = hover_at.filter(|h| !hovered && now >= *h) {
            island.frame(ms(h));
            island.pointer(Pointer::Motion { x: m.output.width / 2. - 40., y: 29. }, ms(h));
            hovered = true;
        }
        island.frame(ms(now));
    }
    let g = island.geometry();
    m.island = IslandGeometry {
        width: g.width,
        height: g.height,
        expanded: g.expanded,
        attached_center_offset: g.attached_center_offset,
    };
    m.slots.island = Some(Rc::new(move || {
        let mut node = island.build();
        node.frame.x = 0.;
        node.frame.y = 0.;
        node
    }));
    true
}

/// `lap-battery-popup-*`: click the module (full bar) or the strip text (compact) whose
/// label contains the step text, then settle; detail comes from canned `battery-info.sh`.
pub fn battery_popup(fixture: &Value, m: &mut Model) -> Option<Node> {
    use hyprv_ui_bar::{PopupCommand, PopupKind, Popups};
    let label = fixture["steps"].as_array()?.iter().find_map(|s| s["click"]["contains"].as_str())?;
    let info = fixture["processes"].as_array()?.iter()
        .find(|p| str_value(&p["match"]).contains("battery-info.sh"))
        .and_then(|p| serde_json::from_str::<Value>(str_value(&p["stdout"])).ok())?;
    m.battery = BatteryDetail {
        available: info["available"] == true,
        status: str_value(&info["status"]).into(),
        mode: match str_value(&info["mode"]) {
            "charging" => BatteryMode::Charging,
            "discharging" => BatteryMode::Discharging,
            "full" => BatteryMode::Full,
            "plugged" => BatteryMode::Plugged,
            _ => BatteryMode::Idle,
        },
        capacity_percent: info["capacity"].as_f64(),
        power_w: info["powerW"].as_f64(),
        average_power_w: info["averagePowerW"].as_f64(),
        samples: info["sampleCount"].as_u64().unwrap_or(0) as u32,
        window_s: info["sampleWindowSeconds"].as_u64().unwrap_or(0) as u32,
        window_complete: info["windowComplete"] == true,
        estimate_s: info["estimateSeconds"].as_f64(),
        estimate_basis: str_value(&info["estimateBasis"]).into(),
        energy_now_wh: info["energyNowWh"].as_f64(),
        energy_full_wh: info["energyFullWh"].as_f64(),
    };
    let anchor = if m.compact_strip_visible() {
        let strip = surfaces::compact_strip(m);
        strip.items.iter().find(|(t, _)| *t == Target::Battery).map(|(_, r)| *r)?
    } else {
        let l = layout::compute(m, &m.theme);
        let module = l.modules.iter().find(|n| n.label.contains(label))?;
        l.target(&module.target)?
    };
    let mut p = Popups::default();
    p.command(m, PopupCommand::Battery { anchor, source: Target::Battery }, Duration::ZERO);
    let mut t = 0.;
    while t < 1500. {
        p.update(m, Duration::from_secs_f64(t / 1000.));
        t += TICK_MS;
    }
    p.update(m, Duration::from_millis(1500));
    Some(p.build(m, &m.theme, PopupKind::Battery))
}
