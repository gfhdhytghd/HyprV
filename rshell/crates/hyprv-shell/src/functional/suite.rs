#[path = "laptop.rs"]
mod laptop;
use super::*;
use std::{error::Error, path::Path, time::Duration};
pub(super) struct Suite {
    native: native::Native,
    runtime: hyprv_core::tokio::runtime::Runtime,
    results: Vec<Value>,
    directory: std::path::PathBuf,
}
impl Suite {
    fn call(
        &self,
        target: &str,
        function: &str,
        args: Vec<String>,
    ) -> Result<Value, Box<dyn Error>> {
        let response = self.runtime.block_on(ipc::call(
            &ipc::socket_path()?,
            &Request {
                target: target.into(),
                function: function.into(),
                args,
            },
        ))?;
        if !response.ok {
            return Err(response.result.into());
        }
        Ok(serde_json::from_str(&response.result).unwrap_or(json!(response.result)))
    }
    pub(super) fn rpc(&self, name: &str, value: Option<Value>) -> Result<Value, Box<dyn Error>> {
        use std::io::{Read, Write};
        if name == "clear" {
            self.native.completion.set(None);
        }

        let path =
            std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("private runtime")?)
                .join("functional.sock");
        let mut stream = std::os::unix::net::UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(60)))?;
        stream.write_all(&serde_json::to_vec(&Request {
            target: "e2e".into(),
            function: name.into(),
            args: value.into_iter().map(|v| v.to_string()).collect(),
        })?)?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut data = Vec::new();
        stream.read_to_end(&mut data)?;
        let response: ipc::Response = serde_json::from_slice(&data)?;
        if !response.ok {
            return Err(response.result.into());
        }
        Ok(serde_json::from_str(&response.result)?)
    }
    fn compositor(&self, request: &str) -> Result<Value, Box<dyn Error>> {
        use std::io::{Read, Write};
        let runtime =
            std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("runtime")?);
        if !runtime
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("hv-"))
        {
            return Err("test hook refuses non-private runtime".into());
        }
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
        let mut stream = std::os::unix::net::UnixStream::connect(
            runtime.join("hypr").join(signature).join(".socket.sock"),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.write_all(request.as_bytes())?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes)?;
        Ok(serde_json::from_slice(&bytes).unwrap_or(json!(String::from_utf8_lossy(&bytes))))
    }
    fn pixels(&self, namespace: &str, rect: (f64, f64, f64, f64)) -> Result<Value, Box<dyn Error>> {
        self.rpc(
            "pixels",
            Some(json!({"namespace":namespace,"rect":[rect.0,rect.1,rect.2,rect.3]})),
        )
    }
    fn open_calendar(&mut self) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        self.reset_seed(Some("calendar"))?;
        self.call("calendar", "toggle", vec![])?;
        self.settle(650);
        let state = self.rpc("snapshot", None)?;
        let size = &state["panels"][0]["calendar_size"];
        let width = size[0].as_f64().ok_or("calendar width")?;
        self.rpc("clear", None)?;
        Ok((
            (3072. - width) / 2.,
            10.,
            width,
            size[1].as_f64().ok_or("calendar height")?,
        ))
    }
    fn calendar_zone(&mut self, target: &str) -> Result<(f64, f64), Box<dyn Error>> {
        let state = self.rpc("snapshot", None)?;
        let width = state["panels"][0]["calendar_size"][0]
            .as_f64()
            .ok_or("calendar width")?;
        let left = (3072. - width) / 2.;
        for dx in (16..width as usize).step_by(16) {
            self.native.motion(left + dx as f64, 39.)?;
            self.settle(3);
            let state = self.rpc("snapshot", None)?;
            if state["panels"][0]["calendar_hover"] == format!("Some({target})") {
                return Ok((left + dx as f64, 39.));
            }
        }
        Err(format!("calendar toolbar target missing: {target}").into())
    }
    fn island(&self) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        let state = self.rpc("snapshot", None)?;
        let r = state["islands"][0]["rect"]
            .as_array()
            .ok_or("island rect")?;
        Ok((
            r[0].as_f64().unwrap(),
            r[1].as_f64().unwrap(),
            r[2].as_f64().unwrap(),
            r[3].as_f64().unwrap(),
        ))
    }
    fn expanded_music(&mut self) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        self.reset_seed(Some("music"))?;
        let (x, y, w, h) = self.island()?;
        self.native.motion(x + w - 10., y + h / 2.)?;
        self.settle(700);
        if self.rpc("snapshot", None)?["islands"][0]["music_expanded"] != true {
            return Err("native hover did not expand music".into());
        }
        self.rpc("clear", None)?;
        self.island()
    }
    pub(super) fn settle(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
    fn reset(&mut self) -> Result<(), Box<dyn Error>> {
        self.reset_seed(None)
    }
    pub(super) fn reset_seed(&mut self, seed: Option<&str>) -> Result<(), Box<dyn Error>> {
        let fixture = seed
            .map(|s| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/functional")
                    .join(format!("{s}.json"))
            })
            .unwrap_or(std::env::var("HYPRV_NESTED_FIXTURE")?.into());
        self.rpc("reset", Some(json!({"fixture":fixture})))?;
        self.native.motion(1500., 1000.)?;
        self.settle(450);
        self.rpc("clear", None)?;
        Ok(())
    }
    fn card(&self, namespace: &str) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        let state = self.rpc("snapshot", None)?;
        if let Some(r) = state["popup_rects"][namespace].as_array() {
            return Ok((
                r[0].as_f64().unwrap(),
                r[1].as_f64().unwrap(),
                r[2].as_f64().unwrap(),
                r[3].as_f64().unwrap(),
            ));
        }
        let surface = state["surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["namespace"] == namespace && s["input_only"] != true)
            .ok_or_else(|| format!("surface missing {namespace}"))?;
        let r = surface["card"].as_array().ok_or("card not found")?;
        Ok((
            r[0].as_f64().unwrap(),
            r[1].as_f64().unwrap(),
            r[2].as_f64().unwrap(),
            r[3].as_f64().unwrap(),
        ))
    }
    fn open_control(&mut self) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        let (x, y, _) = self.module("ControlPanel")?;
        self.native.click(x, y, 0x110)?;
        self.settle(580);
        let card = self.card("shell:hyprv-control-panel")?;
        self.rpc("clear", None)?;
        Ok(card)
    }
    fn open_wifi(&mut self) -> Result<(f64, f64, f64, f64), Box<dyn Error>> {
        let (x, y, _) = self.module("Wifi")?;
        self.native.click(x, y, 0x110)?;
        self.settle(580);
        let card = self.card("shell:hyprv-wifi")?;
        self.rpc("clear", None)?;
        Ok(card)
    }
    // Locate an existing MouseArea by native hover. This observes the UI; it never
    // calls a component pointer handler or dispatches an action directly.
    fn zone(
        &mut self,
        namespace: &str,
        field: &str,
        zone: &str,
    ) -> Result<(f64, f64), Box<dyn Error>> {
        let (x, y, w, h) = self.card(namespace)?;
        // Independent spec positions, verified through real hover before clicking.
        let hint = match zone {
            "BtPower" => Some((140., 30.)),
            "BtClose" | "Close" => Some((330., 28.)),
            "BtScan" => Some((55., 150.)),
            "BtUnnamed" => Some((170., 150.)),
            "BtAdvanced" => Some((300., 150.)),
            "BtDevice(0)" => Some((340., 206.)),
            "BtDevice(1)" => Some((340., 274.)),
            "BtDevice(2)" => Some((340., 342.)),
            "MhPower" => Some((354., 30.)),
            "MhMode(0)" => Some((68., 141.)),
            "MhMode(1)" => Some((192., 141.)),
            "MhMode(2)" => Some((316., 141.)),
            "MhRefresh" => Some((99., 185.)),
            "MhDashboard" => Some((285., 185.)),
            "MhBack" => Some((35., 230.)),
            "Radio" => Some((58., 142.)),
            "Rescan" => Some((160., 142.)),
            "Advanced" => Some((280., 142.)),
            "Connect(\"Home-5G\")" => Some((340., 198.)),
            "Connect(\"Office\")" => Some((340., 267.)),
            "Connect(\"Cafe Free\")" => Some((340., 336.)),
            "Connect(\"Neighbour\")" => Some((340., 405.)),
            "Field(\"Neighbour\")" => Some((100., 454.)),
            "Join(\"Neighbour\")" => Some((60., 500.)),
            "Cancel(\"Neighbour\")" => Some((150., 500.)),
            _ => None,
        };
        if let Some((dx, dy)) = hint {
            for offset in [0., -8., 8., -16., 16.] {
                self.native.motion(x + dx, y + dy + offset)?;
                self.settle(20);
                if self.rpc("snapshot", None)?["panels"][0][field] == format!("Some({zone})") {
                    return Ok((x + dx, y + dy + offset));
                }
            }
        }
        for dy in (12..h.ceil() as usize).step_by(22) {
            for dx in (12..w.ceil() as usize).step_by(22) {
                self.native.motion(x + dx as f64, y + dy as f64)?;
                self.settle(3);
                let state = self.rpc("snapshot", None)?;
                if state["panels"][0][field] == format!("Some({zone})") {
                    return Ok((x + dx as f64, y + dy as f64));
                }
            }
        }
        Err(format!("could not hover zone {zone} in {namespace}").into())
    }
    fn click_zone(
        &mut self,
        namespace: &str,
        field: &str,
        zone: &str,
    ) -> Result<(), Box<dyn Error>> {
        let (x, y) = self.zone(namespace, field, zone)?;
        self.rpc("clear", None)?;
        self.native.click(x, y, 0x110)
    }
    fn call_setup_page(&self, page: &str) -> Result<(), Box<dyn Error>> {
        // RPC's argument is a JSON string, decoded by the setup endpoint.
        self.rpc("page", Some(json!(page)))?;
        self.settle(800);
        self.rpc("clear", None)?;
        Ok(())
    }
    fn open_bluetooth(&mut self) -> Result<(), Box<dyn Error>> {
        let (x, y, _, _) = self.open_control()?;
        self.native.click(x + 95., y + 103., 0x110)?;
        self.settle(800);
        if self.rpc("snapshot", None)?["panels"][0]["page"] != "Bluetooth" {
            return Err("Bluetooth page did not open".into());
        }
        self.rpc("clear", None)?;
        Ok(())
    }
    fn local(&self, key: &str, value: Value) -> Result<Value, Box<dyn Error>> {
        let mut detail = self.expect(json!([]))?;
        let until = std::time::Instant::now() + Duration::from_secs(3);
        let mut state = self.rpc("snapshot", None)?;
        while state.pointer(key) != Some(&value) && std::time::Instant::now() < until {
            self.settle(30);
            state = self.rpc("snapshot", None)?;
        }
        let actual = state
            .pointer(key)
            .ok_or_else(|| format!("missing state {key}"))?;
        if *actual != value {
            return Err(format!("{key}: expected {value}, actual {actual}").into());
        }
        detail["state_assertion"] = json!({"path":key,"value":value});
        Ok(detail)
    }
    fn module(&self, name: &str) -> Result<(f64, f64, Value), Box<dyn Error>> {
        let state = self.rpc("snapshot", None)?;
        let module = state["bars"][0]["modules"]
            .as_array()
            .ok_or("no modules")?
            .iter()
            .find(|m| m["target"] == name)
            .ok_or_else(|| format!("missing bar module {name}"))?;
        let r = &module["rect"];
        Ok((
            r[0].as_f64().unwrap() + r[2].as_f64().unwrap() / 2.,
            r[1].as_f64().unwrap() + r[3].as_f64().unwrap() / 2.,
            module.clone(),
        ))
    }
    pub(super) fn case(
        &mut self,
        id: &str,
        source: &str,
        test: impl FnOnce(&mut Self) -> Result<Value, Box<dyn Error>>,
    ) {
        if std::env::var("HYPRV_FUNCTIONAL_FILTER")
            .is_ok_and(|f| !f.split(',').any(|part| id.contains(part)))
        {
            return;
        }
        let started = std::time::Instant::now();
        let result = test(self);
        let mut row = match result {
            Ok(detail) => json!({"id":id,"source":source,"status":"PASS","detail":detail}),
            Err(e) => {
                json!({"id":id,"source":source,"status":"FAIL","error":e.to_string(),"state":self.rpc("snapshot",None).ok(),"native_layers":self.compositor("j/layers").ok(),"native_cursor":self.compositor("j/cursorpos").ok()})
            }
        };
        row["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
        eprintln!(
            "{} {id}: {}ms {}",
            row["status"],
            row["elapsed_ms"],
            row["error"].as_str().unwrap_or("")
        );
        self.results.push(row);
        let checkpoint = json!({"pass":false,"suite_complete":false,"cases":self.results,"input":"zwlr_virtual_pointer_v1 + zwp_virtual_keyboard_v1","services":"mock sink; zero native service actors","binary":std::env::current_exe().ok()});
        let _ = std::fs::write(
            self.directory.join("checkpoint.json"),
            serde_json::to_vec_pretty(&checkpoint).unwrap(),
        );
    }
    fn expect(&self, expected: Value) -> Result<Value, Box<dyn Error>> {
        let deadline = std::time::Instant::now() + Duration::from_secs(12);
        let complete = |state: &Value| {
            self.native.completion.get().is_none_or(|kind| {
                state["inputs"].as_array().is_some_and(|events| {
                    events.iter().any(|v| {
                        v.as_str().is_some_and(|v| {
                            if kind == "Motion" {
                                v.contains("Motion") || v.contains("Enter") || v.contains("Leave")
                            } else {
                                v.contains(kind)
                            }
                        })
                    })
                })
            })
        };
        let mut state = self.rpc("snapshot", None)?;
        while !complete(&state) {
            if std::time::Instant::now() > deadline {
                return Err(format!(
                    "native {:?} was not delivered: {}",
                    self.native.completion.get(),
                    state["inputs"]
                )
                .into());
            }
            self.settle(20);
            state = self.rpc("snapshot", None)?;
        }
        // Native receipt precedes scheduled UI work (menu close transitions in
        // particular). Wait for the complete transaction, then observe a quiet
        // interval to detect duplicate dispatch rather than accepting a prefix.
        while state["actions"] != expected && std::time::Instant::now() < deadline {
            self.settle(25);
            state = self.rpc("snapshot", None)?;
        }
        self.settle(100);
        state = self.rpc("snapshot", None)?;
        let actual = &state["actions"];
        if *actual != expected {
            return Err(format!("expected {expected}; actual {actual}").into());
        }
        Ok(json!({"expected":expected,"actual":actual,"native_inputs":state["inputs"]}))
    }
    fn bar_click(
        &mut self,
        name: &str,
        button: u32,
        expected: Value,
    ) -> Result<Value, Box<dyn Error>> {
        self.reset()?;
        let (x, y, _) = self.module(name)?;
        self.native.click(x, y, button)?;
        self.expect(expected)
    }
}
fn action(service: &str, action: &str) -> Value {
    json!({"service":service,"action":action})
}
pub fn run(directory: &Path) {
    let result = (|| -> Result<Value, Box<dyn Error>> {
        let runtime = hyprv_core::tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        std::thread::sleep(Duration::from_millis(700));
        let mut s = Suite {
            native: native::Native::connect()?,
            runtime,
            results: vec![],
            directory: directory.to_path_buf(),
        };
        if std::env::var("HYPRV_IDLE_TEST").as_deref() == Ok("1") {
            s.case("idle.quiet-10s", "idle wakeup regression", |s| {
                s.native.motion(2000., 1000.)?;
                s.settle(2000);
                // The real minute clock remains installed; keep this ten-second
                // unchanged-state window away from its legitimate minute edge.
                let second = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() % 60;
                if second >= 47 { std::thread::sleep(Duration::from_secs(61 - second)); }
                s.rpc("idle-arm", None)?;
                s.settle(200);
                let before = s.rpc("profile-clear", None)?;
                std::thread::sleep(Duration::from_millis(10100));
                let profile = s.rpc("profile", None)?;
                let csv = profile["csv"].as_str().ok_or("missing profile")?;
                let count = |name: &str| csv.lines().skip(1).find_map(|row| {
                    let cells: Vec<_> = row.split(',').collect();
                    (cells.get(1) == Some(&name)).then(|| cells[4].parse::<u64>().unwrap_or(0))
                }).unwrap_or(0);
                let ticks = count("idle.stats_tick");
                let wakes = count("engine.dispatch");
                let paints = count("engine.paint");
                let metadata_wakes = count("idle.stats_ui_wake");
                let projected = count("service.metadata_only");
                let voluntary = profile["main_voluntary"].as_u64().ok_or("missing main thread counter")?
                    - before["main_voluntary"].as_u64().ok_or("missing initial main thread counter")?;
                std::fs::write(s.directory.join("idle.csv"), csv)?;
                if !(9..=11).contains(&ticks) || projected + 1 < ticks || paints != 0 || wakes > 1 || voluntary > 2 || metadata_wakes != 0 {
                    return Err(format!("quiet idle: {ticks} stats ticks, {wakes} dispatches, {voluntary} voluntary switches (RPC boundaries allowed), {paints} paints").into());
                }
                Ok(json!({"seconds":10.1,"stats_ticks":ticks,"projected_unchanged_samples":projected,"stats_metadata_ui_wakes":metadata_wakes,"main_dispatches":wakes,"main_voluntary_switches":voluntary,"paints":paints,"providers":"1Hz mock sampler through production projected bridge; native frame callbacks/deadlines enabled"}))
            });
            return Ok(json!({"pass":s.results.iter().all(|r|r["status"]=="PASS"),"cases":s.results}));
        }
        if std::env::var("HYPRV_FUNCTIONAL_LAYER_COMMITS_ONLY").as_deref() == Ok("1") {
            super::layer_commits::run(&mut s);
            return Ok(json!({"pass":s.results.iter().all(|r|r["status"]=="PASS"),"suite_complete":true,"cases":s.results,"services":"mock; layer resize protocol probe"}));
        }
        if std::env::var("HYPRV_FUNCTIONAL_CALLBACKS_ONLY").as_deref() == Ok("1") {
            super::readiness::run(&mut s);
            return Ok(json!({"pass":s.results.iter().all(|r|r["status"]=="PASS"),"suite_complete":true,"cases":s.results,"services":"UI mock graph; real Ready channel/worker and live Assembly Adapter"}));
        }
        if crate::laptop::active() {
            laptop::run(&mut s);
            return Ok(json!({"pass":s.results.iter().all(|r|r["status"]=="PASS"),"suite_complete":true,"cases":s.results,"input":"zwlr_virtual_pointer_v1 + zwp_virtual_keyboard_v1","services":"UI mock sink plus explicitly labelled real power actors on per-case private buses/sysfs","binary":std::env::current_exe().ok()}));
        }
        for (id, target, button, service, command) in [
            (
                "bar.launcher",
                "Launcher",
                0x110,
                "applications",
                "Launch(Launcher)",
            ),
            (
                "bar.workspace",
                "Workspace(2)",
                0x110,
                "hyprland",
                "WorkspaceActivate(2)",
            ),
            ("bar.media.previous", "Previous", 0x110, "mpris", "Previous"),
            ("bar.media.play", "PlayPause", 0x110, "mpris", "PlayPause"),
            ("bar.media.next", "Next", 0x110, "mpris", "Next"),
            ("bar.volume.mute", "Volume", 0x110, "audio", "ToggleMute"),
            (
                "bar.volume.mixer",
                "Volume",
                0x111,
                "applications",
                "Launch(Pavucontrol)",
            ),
            (
                "bar.notifications.show",
                "Notifications",
                0x110,
                "notifications",
                "ToggleVisibility",
            ),
            (
                "bar.notifications.dnd",
                "Notifications",
                0x111,
                "notifications",
                "ToggleDnd",
            ),
        ] {
            s.case(id, "ui-bar-layout.md", |s| {
                s.bar_click(target, button, json!([action(service, command)]))
            });
        }
        // Every displayed bar module receives real hover and scroll; only volume/tray consume wheels.
        s.reset()?;
        let names: Vec<String> = s.rpc("snapshot", None)?["bars"][0]["modules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["interactive"] == true)
            .map(|m| m["target"].as_str().unwrap().to_owned())
            .collect();
        for name in names {
            s.case(&format!("bar.hover.{name}"), "ui-bar-layout.md", |s| {
                s.reset()?;
                let (x, y, _) = s.module(&name)?;
                s.native.motion(x, y)?;
                s.local("/bars/0/hovered", json!(format!("Some({name})")))
            });
            s.case(&format!("bar.scroll.{name}"), "ui-bar-layout.md", |s| {
                s.reset()?;
                let (x, y, _) = s.module(&name)?;
                s.native.motion(x, y)?;
                s.settle(30);
                s.native.wheel(1, false)?;
                let expected = if name == "Volume" {
                    json!([action("audio", "Increase")])
                } else if name.starts_with("Tray(") {
                    let key = name.strip_prefix("Tray(").unwrap().trim_end_matches(')');
                    json!([action(
                        "tray",
                        &format!("Scroll {{ item: {key}, delta: -1, horizontal: false }}")
                    )])
                } else {
                    json!([])
                };
                let detail = s.expect(expected)?;
                let state = s.rpc("snapshot", None)?;
                if !state["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_str().is_some_and(|v| v.starts_with("Scroll")))
                {
                    return Err("no native scroll delivery".into());
                }
                Ok(detail)
            });
        }
        for (target, button, command) in [
            ("Cpu", 0x111, "Launch(Btop)"),
            ("Memory", 0x111, "Launch(Btop)"),
            ("Temperature", 0x110, "Launch(Btop)"),
            ("Viewflow", 0x111, "Launch(ViewflowWindowsMenu)"),
        ] {
            s.case(
                &format!("bar.click.{target}.{button}"),
                "ui-bar-layout.md",
                |s| s.bar_click(target, button, json!([action("applications", command)])),
            );
        }
        s.case("bar.viewflow.start", "ui-bar-layout.md", |s| {
            s.reset()?;
            let (x, y, m) = s.module("Viewflow")?;
            let r = &m["rect"];
            let ax = r[0].as_f64().unwrap().round() as i64;
            let ay = (r[1].as_f64().unwrap() + r[3].as_f64().unwrap() + 8.).round() as i64;
            s.native.click(x, y, 0x110)?;
            s.expect(json!([action(
                "applications",
                &format!("Launch(ViewflowWindowsStart {{ x: {ax}, y: {ay} }})")
            )]))
        });
        for target in ["Cpu", "Memory", "Network"] {
            s.case(&format!("bar.stats.{target}"), "ui-bar-layout.md", |s| {
                s.reset()?;
                let (x, y, _) = s.module(target)?;
                s.native.click(x, y, 0x110)?;
                s.local("/panels/0/stats", json!(true))
            });
        }
        s.case("bar.control.open", "ui-bar-layout.md", |s| {
            s.reset()?;
            let (x, y, _) = s.module("ControlPanel")?;
            s.native.click(x, y, 0x110)?;
            s.local("/panels/0/control", json!(true))
        });
        s.case("bar.volume.delayed-osd", "ui-bar-layout.md", |s| {
            s.reset()?;
            let (x, y, _) = s.module("Volume")?;
            s.native.click(x, y, 0x110)?;
            let mut detail = s.expect(json!([action("audio", "ToggleMute")]))?;
            let state = s.rpc("snapshot", None)?;
            if state["panels"][0]["quick"] != true {
                return Err("delayed volume OSD was not opened".into());
            }
            detail["osd_open"] = json!(true);
            Ok(detail)
        });
        // Main-page tile centres are independent spec coordinates relative to the real card.
        for (id, dx, dy, service, command) in [
            ("wifi.radio", 25., 31., "network", "SetRadio(false)"),
            ("bluetooth.power", 25., 103., "bluetooth", "SetPower(false)"),
            ("vm", 31., 175., "vm", "Toggle"),
            ("viewflow", 103., 175., "viewflow", "Set(MacOs)"),
            ("record", 31., 247., "recording", "Toggle"),
            ("sleep", 103., 247., "inhibit", "Toggle(Manual)"),
            ("theme", 175., 175., "theme", "ToggleTheme"),
            ("dnd", 247., 175., "notifications", "ToggleDnd"),
            ("demo", 175., 247., "demo", "Toggle"),
            ("profile", 247., 247., "power", "ProfileSet(Performance)"),
            ("media.previous", 163., 117., "mpris", "Previous"),
            ("media.play", 211., 117., "mpris", "PlayPause"),
            ("media.next", 258., 117., "mpris", "Next"),
            ("volume.mute", 175., 319., "audio", "ToggleMute"),
            ("volume.set", 244.5, 319., "audio", "SetVolume(75.0)"),
            (
                "fan.chassis",
                100.5,
                391.,
                "fan",
                "SetLimit { channel: Chassis, percent: 75 }",
            ),
            (
                "fan.pump",
                244.5,
                391.,
                "fan",
                "SetLimit { channel: Pump, percent: 75 }",
            ),
        ] {
            s.case(
                &format!("control.{id}"),
                "ui-panels-control.md §1.4–1.5",
                |s| {
                    s.reset()?;
                    let (x, y, _, _) = s.open_control()?;
                    s.native.click(x + dx, y + dy, 0x110)?;
                    s.expect(json!([action(service, command)]))
                },
            );
        }
        s.case(
            "control.brightness.commit",
            "ui-panels-control.md §1.4",
            |s| {
                s.reset()?;
                let (x, y, _, _) = s.open_control()?;
                s.native.click(x + 100.5, y + 319., 0x110)?;
                s.expect(json!([
                    action(
                        "backlight",
                        "Set { output: \"WAYLAND-1\", percent: 76.0, sequence: 1 }"
                    ),
                    action("backlight", "Finish { output: \"WAYLAND-1\", sequence: 1 }")
                ]))
            },
        );
        s.case("control.outside-close", "ui-panels-control.md §1.1", |s| {
            s.reset()?;
            s.open_control()?;
            s.native.click(1500., 1000., 0x110)?;
            s.settle(400);
            s.local("/panels/0/control", json!(false))
        });
        s.case("control.escape-close", "ui-panels-control.md §1.1", |s| {
            s.reset()?;
            s.open_control()?;
            s.native.key(1)?;
            s.settle(400);
            s.local("/panels/0/control", json!(false))
        });
        s.case("control.wifi-text", "ui-panels-control.md §1.5", |s| {
            s.reset()?;
            let (x, y, _, _) = s.open_control()?;
            s.native.click(x + 95., y + 31., 0x110)?;
            s.settle(400);
            let detail = s.expect(json!([]))?;
            if s.rpc("snapshot", None)?["panels"][0]["wifi"] != true {
                return Err("Wi-Fi tile did not open panel".into());
            }
            Ok(detail)
        });
        // IPC uses the actual core Unix socket, not the test RPC socket.
        for (function, args) in [
            ("showBrightness", vec![]),
            ("showBrightnessIncrease", vec![]),
            ("showBrightnessDecrease", vec![]),
            ("showBrightnessLevel", vec!["42".to_string()]),
            ("showVolume", vec![]),
        ] {
            s.case(
                &format!("ipc.quickAdjust.{function}"),
                "ui-bar.md §4 / hypr/controls.lua:51",
                |s| {
                    s.reset()?;
                    s.call("quickAdjust", function, args)?;
                    s.local("/panels/0/quick", json!(true))
                },
            );
        }
        for (target, function, args) in [
            ("bar", "toggle", vec!["WAYLAND-1".into()]),
            ("bar", "layoutStatus", vec!["WAYLAND-1".into()]),
            ("bar", "inputStatus", vec![]),
            ("bar", "dataStatus", vec![]),
        ] {
            s.case(&format!("ipc.{target}.{function}"), "ui-bar.md §4", |s| {
                s.reset()?;
                let response = s.call(target, function, args)?;
                let mut detail = s.expect(json!([]))?;
                detail["response"] = response;
                Ok(detail)
            });
        }
        s.case(
            "ipc.controlPanel.toggle",
            "ui-bar.md §4 / hyprland-bind.lua:54",
            |s| {
                s.reset()?;
                s.call("controlPanel", "toggle", vec![])?;
                s.local("/panels/0/control", json!(true))
            },
        );
        s.case("ipc.calendar.toggle", "ui-bar.md §4", |s| {
            s.reset()?;
            s.call("calendar", "toggle", vec![])?;
            s.local("/bars/0/calendar", json!(true))
        });
        for (id, seed, zone, expected) in [
            (
                "power-off",
                "bt",
                "BtPower",
                json!([action("bluetooth", "SetPower(false)")]),
            ),
            (
                "power-on",
                "bt-off",
                "BtPower",
                json!([action("bluetooth", "SetPower(true)")]),
            ),
            ("scan", "bt", "BtScan", json!([action("bluetooth", "Scan")])),
            (
                "advanced",
                "bt",
                "BtAdvanced",
                json!([action("applications", "Launch(BluetoothManager)")]),
            ),
            (
                "disconnect",
                "bt",
                "BtDevice(0)",
                json!([action(
                    "bluetooth",
                    "Disconnect { address: \"AC:80:0A:11:22:33\" }"
                )]),
            ),
            (
                "connect-paired",
                "bt",
                "BtDevice(1)",
                json!([action(
                    "bluetooth",
                    "Connect { address: \"D4:11:22:33:44:55\", paired: true }"
                )]),
            ),
            (
                "connect-unpaired",
                "bt",
                "BtDevice(2)",
                json!([action(
                    "bluetooth",
                    "Connect { address: \"5C:AA:BB:CC:DD:EE\", paired: false }"
                )]),
            ),
        ] {
            s.case(
                &format!("bluetooth.{id}"),
                "ui-panels-control.md §3–5",
                |s| {
                    s.reset_seed(Some(seed))?;
                    s.open_bluetooth()?;
                    s.click_zone("shell:hyprv-control-panel", "control_hover", zone)?;
                    s.expect(expected)
                },
            );
        }
        s.case(
            "bluetooth.unnamed-filter",
            "ui-panels-control.md §3",
            |s| {
                s.reset_seed(Some("bt"))?;
                s.open_bluetooth()?;
                s.click_zone("shell:hyprv-control-panel", "control_hover", "BtUnnamed")?;
                s.local("/panels/0/bluetooth_show_unnamed", json!(false))?;
                s.local("/panels/0/bluetooth_visible_devices", json!(2))
            },
        );
        s.case("bluetooth.back", "ui-panels-control.md §3", |s| {
            s.reset_seed(Some("bt"))?;
            s.open_bluetooth()?;
            s.click_zone("shell:hyprv-control-panel", "control_hover", "BtClose")?;
            s.settle(800);
            s.local("/panels/0/page", json!("Main"))
        });
        s.case("bluetooth.list-scroll", "ui-panels-control.md §3", |s| {
            s.reset_seed(Some("bt-many"))?;
            s.open_bluetooth()?;
            let (x, y, _, _) = s.card("shell:hyprv-control-panel")?;
            s.native.motion(x + 180., y + 300.)?;
            s.native.wheel(3, false)?;
            s.expect(json!([]))
        });
        for (id, zone, expected) in [
            (
                "stop",
                "MhPower",
                json!([action("mihomo", "Service(Stop)")]),
            ),
            (
                "mode-rule",
                "MhMode(0)",
                json!([action("mihomo", "Mode(\"rule\")")]),
            ),
            (
                "mode-global",
                "MhMode(1)",
                json!([action("mihomo", "Mode(\"global\")")]),
            ),
            (
                "mode-direct",
                "MhMode(2)",
                json!([action("mihomo", "Mode(\"direct\")")]),
            ),
            ("refresh", "MhRefresh", json!([action("mihomo", "Refresh")])),
            (
                "dashboard",
                "MhDashboard",
                json!([action("mihomo", "OpenDashboard")]),
            ),
        ] {
            s.case(
                &format!("mihomo.{id}"),
                "ui-panels-control.md §7 (unreachable legacy page; setup-only mount)",
                |s| {
                    s.reset()?;
                    s.open_control()?;
                    s.call_setup_page("mihomo")?;
                    s.click_zone("shell:hyprv-control-panel", "control_hover", zone)?;
                    s.expect(expected)
                },
            );
        }
        for (id, zone, expected) in [
            (
                "radio",
                "Radio",
                json!([action("network", "SetRadio(false)")]),
            ),
            ("rescan", "Rescan", json!([action("network", "Rescan")])),
            (
                "advanced",
                "Advanced",
                json!([action("applications", "Launch(NetworkEditor)")]),
            ),
        ] {
            s.case(&format!("wifi.{id}"), "ui-panels-wifi.md §2", |s| {
                s.reset_seed(Some("wifi"))?;
                s.open_wifi()?;
                s.click_zone("shell:hyprv-wifi", "wifi_hover", zone)?;
                s.expect(expected)
            });
        }
        s.case(
            "wifi.open-populated-no-rescan",
            "ui-panels-wifi.md §1",
            |s| {
                s.reset_seed(Some("wifi"))?;
                let (x, y, _) = s.module("Wifi")?;
                s.native.click(x, y, 0x110)?;
                s.expect(json!([]))
            },
        );
        s.case("wifi.close-button", "ui-panels-wifi.md §1", |s| {
            s.reset_seed(Some("wifi"))?;
            s.open_wifi()?;
            s.click_zone("shell:hyprv-wifi", "wifi_hover", "Close")?;
            s.settle(400);
            s.local("/panels/0/wifi", json!(false))
        });
        s.case("wifi.escape-close", "ui-panels-wifi.md §1", |s| {
            s.reset_seed(Some("wifi"))?;
            s.open_wifi()?;
            let (x, y, _, _) = s.card("shell:hyprv-wifi")?;
            s.native.click(x + 50., y + 25., 0x110)?;
            s.native.key(1)?;
            s.settle(400);
            s.local("/panels/0/wifi", json!(false))
        });
        s.case("wifi.outside-close", "ui-panels-wifi.md §1", |s| {
            s.reset_seed(Some("wifi"))?;
            s.open_wifi()?;
            s.native.click(1500., 1000., 0x110)?;
            s.settle(400);
            s.local("/panels/0/wifi", json!(false))
        });
        s.case("wifi.list-scroll", "ui-panels-wifi.md §2", |s| {
            s.reset_seed(Some("wifi-many"))?;
            s.open_wifi()?;
            let (x, y, _, _) = s.card("shell:hyprv-wifi")?;
            s.native.motion(x + 150., y + 280.)?;
            s.native.wheel(3, false)?;
            s.expect(json!([]))
        });

        for (id,zone,expected) in [
            ("disconnect","Connect(\"Home-5G\")","Disconnect"),
            ("known-connect","Connect(\"Office\")","Connect { ssid: \"Office\", password: None, security: \"WPA2\", enterprise: None }"),
            ("open-connect","Connect(\"Cafe Free\")","Connect { ssid: \"Cafe Free\", password: None, security: \"\", enterprise: None }"),
        ]{s.case(&format!("wifi.{id}"),"ui-panels-wifi.md §3–4",|s|{s.reset_seed(Some("wifi"))?;s.open_wifi()?;s.click_zone("shell:hyprv-wifi","wifi_hover",zone)?;s.expect(json!([action("network",expected)]))});}
        s.case("wifi.secure-password-return","ui-panels-wifi.md §3–4",|s|{
            s.reset_seed(Some("secure"))?;s.open_wifi()?;s.click_zone("shell:hyprv-wifi","wifi_hover","Connect(\"Neighbour\")")?;s.settle(350);
            if s.rpc("snapshot",None)?["panels"][0]["wifi_expanded"]!="Neighbour" {return Err("secure connect did not expand password form".into());}
            s.click_zone("shell:hyprv-wifi","wifi_hover","Field(\"Neighbour\")")?;s.native.text("testpass123")?;s.native.key(14)?;s.native.text("4")?;s.native.key(28)?;
            s.expect(json!([action("network","Connect { ssid: \"Neighbour\", password: Some(\"testpass124\"), security: \"WPA3\", enterprise: None }")]))
        });
        s.case("wifi.password-cancel", "ui-panels-wifi.md §3", |s| {
            s.reset_seed(Some("secure"))?;
            s.open_wifi()?;
            s.click_zone("shell:hyprv-wifi", "wifi_hover", "Connect(\"Neighbour\")")?;
            s.settle(350);
            s.click_zone("shell:hyprv-wifi", "wifi_hover", "Cancel(\"Neighbour\")")?;
            s.local("/panels/0/wifi_expanded", json!(""))
        });
        for (id, button, service) in [
            ("lock", 0x110, "Launch(Lock)"),
            ("power", 0x110, "Launch(Logout)"),
        ] {
            s.case(&format!("island.idle.{id}"), "ui-island.md §3.9", |s| {
                s.reset_seed(Some("idle"))?;
                let (x, y, w, h) = s.island()?;
                let px = if id == "lock" { x + 15. } else { x + w - 15. };
                s.native.click(px, y + h / 2., button)?;
                s.expect(json!([action("applications", service)]))
            });
        }
        s.case("island.clock-calendar", "ui-island.md §3.9", |s| {
            s.reset_seed(Some("idle"))?;
            let (x, y, w, h) = s.island()?;
            s.native.click(x + w / 2., y + h / 2., 0x110)?;
            s.local("/bars/0/calendar", json!(true))
        });
        s.case("island.music-expand-collapse", "ui-island.md §4", |s| {
            s.expanded_music()?;
            s.native.motion(1500., 800.)?;
            s.settle(650);
            s.local("/islands/0/music_expanded", json!(false))
        });
        for (id, dx, command) in [
            ("previous", -60., "Previous"),
            ("play", 0., "PlayPause"),
            ("next", 60., "Next"),
        ] {
            s.case(&format!("island.music.{id}"), "ui-island.md §3.11", |s| {
                let (x, y, w, h) = s.expanded_music()?;
                s.native.click(x + w / 2. + dx, y + h - 35., 0x110)?;
                s.expect(json!([action("mpris", command)]))
            });
        }
        s.case("island.music.focus", "ui-island.md §3.11", |s| {
            let (x, y, _, _) = s.expanded_music()?;
            s.native.click(x + 45., y + 35., 0x110)?;
            s.expect(json!([action("mpris", "Raise")]))
        });
        s.case("island.music.seek", "ui-island.md §3.11", |s| {
            let (x, y, w, _) = s.expanded_music()?;
            s.native.click(x + w / 2., y + 101., 0x110)?;
            let expected = 225. / 2.;
            s.expect(json!([action("mpris", &format!("Seek({expected})"))]))
        });
        s.case(
            "island.agent-expand-focus",
            "ui-island.md §3.7 / §6",
            |s| {
                s.reset_seed(Some("agent"))?;
                let (x, y, _, h) = s.island()?;
                s.native.motion(x + 20., y + h / 2.)?;
                s.settle(700);
                let state = s.rpc("snapshot", None)?;
                if state["islands"][0]["agent_expanded"] != true {
                    return Err("native hover did not expand agent".into());
                }
                let (x, y, _, h) = s.island()?;
                s.rpc("clear", None)?;
                s.native.click(x + 80., y + h - 45., 0x110)?;
                s.expect(json!([action(
                    "agent",
                    "FocusSession(\"codex-session-0\")"
                )]))
            },
        );

        for (id, zone, expected) in [
            (
                "previous",
                "Prev",
                json!([action("calendar", "PreviousWeek")]),
            ),
            ("next", "Next", json!([action("calendar", "NextWeek")])),
            (
                "new-event",
                "Create",
                json!([action("calendar", "NewEvent")]),
            ),
            ("expand-app", "Expand", json!([action("calendar", "Show")])),
        ] {
            s.case(
                &format!("calendar.{id}"),
                "ui-panels-calendar.md §1",
                |s| {
                    s.open_calendar()?;
                    let (x, y) = s.calendar_zone(zone)?;
                    s.rpc("clear", None)?;
                    s.native.click(x, y, 0x110)?;
                    s.expect(expected)
                },
            );
        }
        s.case("calendar.range-close", "ui-panels-calendar.md §1", |s| {
            s.open_calendar()?;
            let (x, y) = s.calendar_zone("Range")?;
            s.native.click(x, y, 0x110)?;
            s.settle(500);
            s.local("/bars/0/calendar", json!(false))
        });
        s.case(
            "calendar.vertical-scroll",
            "ui-panels-calendar.md §1",
            |s| {
                let (x, y, _, _) = s.open_calendar()?;
                s.native.motion(x + 250., y + 350.)?;
                s.settle(400);
                let rect = (x + 80., y + 200., 300., 300.);
                let before = s.pixels("hyprv-quickshell", rect)?;
                s.native.wheel(3, false)?;
                s.settle(500);
                let after = s.pixels("hyprv-quickshell", rect)?;
                if before["hash"] == after["hash"] {
                    return Err("calendar timeline pixels did not scroll".into());
                }
                let mut detail = s.expect(json!([]))?;
                detail["before"] = before;
                detail["after"] = after;
                Ok(detail)
            },
        );
        s.case("calendar.outside-close", "ui-panels-calendar.md §1", |s| {
            s.open_calendar()?;
            s.native.click(800., 400., 0x110)?;
            s.settle(500);
            s.local("/bars/0/calendar", json!(false))
        });
        // Every tray item uses the actual bar geometry to verify global SNI coordinates.
        for button in [0x110, 0x111] {
            s.case(
                &format!("tray.activate-{button}"),
                "ui-bar-layout.md §tray",
                |s| {
                    s.reset()?;
                    let target = "Tray(\"discord\")";
                    let (x, y, m) = s.module(target)?;
                    let r = &m["rect"];
                    let px = r[0].as_f64().unwrap().round() as i32;
                    let py = (r[1].as_f64().unwrap() + r[3].as_f64().unwrap()).round() as i32;
                    s.native.click(x, y, button)?;
                    s.expect(json!([action(
                        "tray",
                        &format!(
                            "{} {{ item: \"discord\", x: {px}, y: {py} }}",
                            if button == 0x110 {
                                "Activate"
                            } else {
                                "SecondaryActivate"
                            }
                        )
                    )]))
                },
            );
        }
        s.case("tray.horizontal-scroll", "ui-bar-layout.md §tray", |s| {
            s.reset()?;
            let (x, y, _) = s.module("Tray(\"discord\")")?;
            s.native.motion(x, y)?;
            s.settle(30);
            s.native.wheel(1, true)?;
            s.expect(json!([action(
                "tray",
                "Scroll { item: \"discord\", delta: -1, horizontal: false }"
            )]))
        });
        for (id, row, command) in [
            (
                "activate",
                0,
                Some("MenuEvent { item: \"app-00\", entry: 1, event: \"clicked\" }"),
            ),
            (
                "toggle",
                1,
                Some("MenuEvent { item: \"app-00\", entry: 2, event: \"clicked\" }"),
            ),
            ("disabled", 2, None),
        ] {
            s.case(&format!("tray.menu.{id}"), "ui-bar-popups.md §1", |s| {
                s.reset_seed(Some("tray"))?;
                let (x, y, _) = s.module("Tray(\"app-00\")")?;
                s.native.click(x, y, 0x111)?;
                let opened = s.expect(json!([action("tray", "MenuOpen { item: \"app-00\" }")]))?;
                s.settle(250);
                let (x, y, _, _) = s.card("shell:hyprv-tray-menu")?;
                s.rpc("clear", None)?;
                s.native
                    .click(x + 100., y + 26. + 35. * f64::from(row), 0x110)?;
                let expected = if let Some(command) = command {
                    s.settle(500);
                    json!([
                        action("tray", command),
                        action("tray", "MenuClose { item: \"app-00\" }")
                    ])
                } else {
                    // Disabled delegate does not accept the click: QML's
                    // full-window backdrop closes the menu (shell.qml:914).
                    json!([action("tray", "MenuClose { item: \"app-00\" }")])
                };
                let mut detail = s.expect(expected)?;
                detail["open"] = opened;
                Ok(detail)
            });
        }
        s.case("tray.menu.escape", "ui-bar-popups.md §1", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Tray(\"app-00\")")?;
            s.native.click(x, y, 0x111)?;
            s.settle(450);
            let (x, y, _, _) = s.card("shell:hyprv-tray-menu")?;
            // Focus the OnDemand layer with a real press. Escape dismisses
            // before the release, so this does not activate a menu entry.
            s.native.motion(x + 150., y + 26.)?;
            s.native.button(0x110, true)?;
            s.settle(100);
            s.rpc("clear", None)?;
            s.native.key(1)?;
            s.settle(350);
            let detail = s.expect(json!([action("tray", "MenuClose { item: \"app-00\" }")]))?;
            s.native.button(0x110, false)?;
            if s.rpc("snapshot", None)?["popups"]["menu"] != false {
                return Err("menu still mapped after Escape".into());
            }
            Ok(detail)
        });
        s.case("tray.overflow.open-close", "ui-bar-popups.md §2", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Overflow")?;
            s.native.click(x, y, 0x110)?;
            s.local("/popups/overflow", json!(true))?;
            s.native.click(1500., 1000., 0x110)?;
            s.local("/popups/overflow", json!(false))
        });
        s.case("tray.overflow.activate", "ui-bar-popups.md §2", |s| {
            s.reset_seed(Some("tray"))?;
            let state = s.rpc("snapshot", None)?;
            let visible = state["bars"][0]["modules"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["target"].as_str().unwrap().starts_with("Tray("))
                .count();
            let (x, y, _) = s.module("Overflow")?;
            s.native.click(x, y, 0x110)?;
            s.settle(100);
            let (x, y, _, _) = s.card("shell:hyprv-tray-overflow")?;
            s.rpc("clear", None)?;
            s.native.click(x + 17., y + 27., 0x110)?;
            s.expect(json!([action(
                "tray",
                &format!(
                    "Activate {{ item: \"app-{visible:02}\", x: {}, y: {} }}",
                    (x + 8.).round() as i32,
                    (y + 8. + 38.).round() as i32
                )
            )]))
        });
        s.case("battery.open-close", "ui-bar-popups.md §3", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Battery")?;
            let profile = hyprv_ui::diagnostics::enabled();
            if profile { s.rpc("profile-clear", None)?; }
            s.native.click(x, y, 0x110)?;
            if profile {
                let until = std::time::Instant::now() + Duration::from_secs(3);
                loop {
                    let p = s.rpc("profile", None)?;
                    let csv = p["csv"].as_str().unwrap_or("");
                    if csv.contains("event,frame.submitted,shell:hyprv-battery-info,") {
                        std::fs::write(s.directory.join("battery-open-profile.csv"), csv)?;
                        break;
                    }
                    if std::time::Instant::now() >= until { std::fs::write(s.directory.join("battery-open-profile.csv"), csv)?; return Err("battery first frame timeout".into()); }
                    s.settle(1);
                }
            }
            s.local("/popups/battery", json!(true))?;
            // Timing runs cover the completed reveal and its fully visible card,
            // not only an opening animation interrupted by the outside click.
            if profile { s.settle(600); }
            s.native.click(1500., 1000., 0x110)?;
            s.settle(400);
            let result = s.local("/popups/battery", json!(false))?;
            if profile {
                let p = s.rpc("profile", None)?;
                let csv = p["csv"].as_str().unwrap_or("");
                std::fs::write(s.directory.join("battery-cycle-profile.csv"), csv)?;
                if let Ok(budget) = std::env::var("HYPRV_FRAME_BUDGET_MS") {
                    let budget = budget.parse::<f64>()?;
                    let mut starts = std::collections::HashMap::new();
                    let mut frames = Vec::new();
                    for line in csv.lines().skip(1) {
                        let fields: Vec<_> = line.split(',').collect();
                        if fields.len() < 4 || fields[0] != "event" { continue; }
                        let at = fields[3].parse::<u64>()?;
                        match fields[1] {
                            "frame.begin" => { starts.insert(fields[2], at); },
                            "frame.submitted" => {
                                if let Some(start) = starts.remove(fields[2]) {
                                    frames.push(json!({"surface": fields[2], "ms": (at-start) as f64 / 1e6}));
                                }
                            },
                            _ => (),
                        }
                    }
                    std::fs::write(s.directory.join("battery-frame-budget.json"), serde_json::to_vec_pretty(&frames)?)?;
                    if frames.is_empty() { return Err("no measured frames".into()); }
                    if let Some(slow) = frames.iter().find(|frame| frame["ms"].as_f64().unwrap() > budget) {
                        return Err(format!("frame exceeds {budget}ms budget: {slow}").into());
                    }
                }
            }
            Ok(result)
        });
        for (function, expected) in [
            ("showVolume", json!([action("audio", "SetVolume(75.0)")])),
            (
                "showBrightness",
                json!([action(
                    "backlight",
                    "Set { output: \"WAYLAND-1\", percent: 76.0, sequence: 1 }"
                )]),
            ),
        ] {
            s.case(
                &format!("quick-adjust.{function}.slider"),
                "QuickAdjustPopup.qml:216 (adjustmentFinished is unconnected)",
                |s| {
                    s.reset()?;
                    s.call("quickAdjust", function, vec![])?;
                    s.settle(250);
                    let (x, y, w, h) = s.card("shell:hyprv-quick-adjust")?;
                    s.rpc("clear", None)?;
                    s.native.click(x + w * 0.75, y + h / 2., 0x110)?;
                    s.expect(expected)
                },
            );
        }
        for id in [1, 3, 4, 5] {
            s.case(
                &format!("bar.workspace.{id}"),
                "ui-bar-layout.md §workspaces",
                |s| {
                    s.bar_click(
                        &format!("Workspace({id})"),
                        0x110,
                        json!([action("hyprland", &format!("WorkspaceActivate({id})"))]),
                    )
                },
            );
        }

        s.case("stats.escape-close", "ui-panels-stats.md", |s| {
            s.reset()?;
            let (x, y, _) = s.module("Cpu")?;
            s.native.click(x, y, 0x110)?;
            s.settle(500);
            let (x, y, _, _) = s.card("shell:hyprv-system-stats")?;
            s.native.click(x + 20., y + 20., 0x110)?;
            s.rpc("clear", None)?;
            s.native.key(1)?;
            s.local("/panels/0/stats", json!(false))
        });
        s.case(
            "quick-adjust.volume-icon",
            "ui-panels-control.md §quick-adjust",
            |s| {
                s.reset()?;
                s.call("quickAdjust", "showVolume", vec![])?;
                s.settle(250);
                let (x, y, _, h) = s.card("shell:hyprv-quick-adjust")?;
                s.rpc("clear", None)?;
                s.native.click(x + 40., y + h / 2., 0x110)?;
                s.expect(json!([action("audio", "ToggleMute")]))
            },
        );
        s.case(
            "quick-adjust.auto-hide",
            "ui-panels-control.md §quick-adjust",
            |s| {
                s.reset()?;
                s.call("quickAdjust", "showVolume", vec![])?;
                s.settle(3400);
                s.local("/panels/0/quick", json!(false))
            },
        );
        s.case("tray.menu.open", "ui-bar-popups.md §1", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Tray(\"app-00\")")?;
            s.native.click(x, y, 0x111)?;
            s.expect(json!([action("tray", "MenuOpen { item: \"app-00\" }")]))
        });
        s.case("tray.menu.scroll", "ui-bar-popups.md §1", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Tray(\"app-00\")")?;
            s.native.click(x, y, 0x111)?;
            s.expect(json!([action("tray", "MenuOpen { item: \"app-00\" }")]))?;
            s.settle(400);
            let (x, y, _, _) = s.card("shell:hyprv-tray-menu")?;
            s.rpc("clear", None)?;
            s.native.motion(x + 150., y + 150.)?;
            s.settle(20);
            s.native.wheel(3, false)?;
            let mut detail = s.expect(json!([]))?;
            let state = s.rpc("snapshot", None)?;
            let scroll = state["popups"]["menu_scroll"]
                .as_f64()
                .ok_or("menu scroll")?;
            if scroll <= 0. {
                return Err("menu content did not scroll".into());
            }
            detail["scroll"] = json!(scroll);
            Ok(detail)
        });
        s.case("tray.menu.submenu-back", "ui-bar-popups.md §1", |s| {
            s.reset_seed(Some("tray"))?;
            let (x, y, _) = s.module("Tray(\"app-00\")")?;
            s.native.click(x, y, 0x111)?;
            s.expect(json!([action("tray", "MenuOpen { item: \"app-00\" }")]))?;
            s.settle(350);
            let (x, y, _, _) = s.card("shell:hyprv-tray-menu")?;
            s.rpc("clear", None)?;
            s.native.click(x + 100., y + 131., 0x110)?;
            let opened = s.expect(json!([action(
                "tray",
                "MenuAboutToShow { item: \"app-00\", entry: 4 }"
            )]))?;
            s.settle(350);
            let (x, y, _, _) = s.card("shell:hyprv-tray-menu")?;
            s.rpc("clear", None)?;
            s.native.click(x + 100., y + 26., 0x110)?;
            let mut detail = s.local("/popups/menu_stack", json!(0))?;
            detail["submenu_open"] = opened;
            Ok(detail)
        });

        s.case(
            "bar.volume.touchpad-phases",
            "ui-bar-layout.md / Qt Wayland axis semantics",
            |s| {
                s.reset()?;
                let (x, y, _) = s.module("Volume")?;
                s.native.motion(x, y)?;
                s.settle(20);
                s.native.finger(12.)?;
                let detail = s.expect(json!([action("audio", "Increase")]))?;
                let state = s.rpc("snapshot", None)?;
                for phase in ["Begin", "Update", "End"] {
                    if !state["inputs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v.as_str().is_some_and(|v| v.contains(phase)))
                    {
                        return Err(format!("missing native scroll {phase}").into());
                    }
                }
                Ok(detail)
            },
        );
        for (id, x0, y0, x1, service, first, last) in [
            (
                "volume",
                244.5,
                319.,
                157.4,
                "audio",
                "SetVolume(75.0)",
                "SetVolume(10.0)",
            ),
            (
                "chassis",
                100.5,
                391.,
                13.4,
                "fan",
                "SetLimit { channel: Chassis, percent: 75 }",
                "SetLimit { channel: Chassis, percent: 10 }",
            ),
            (
                "pump",
                244.5,
                391.,
                157.4,
                "fan",
                "SetLimit { channel: Pump, percent: 75 }",
                "SetLimit { channel: Pump, percent: 10 }",
            ),
        ] {
            s.case(
                &format!("control.slider-drag.{id}"),
                "ui-panels-control.md §2.3",
                |s| {
                    s.reset()?;
                    let (x, y, _, _) = s.open_control()?;
                    s.native.motion(x + x0, y + y0)?;
                    s.native.button(0x110, true)?;
                    s.settle(40);
                    s.native.motion(x + x1, y + y0)?;
                    s.settle(40);
                    s.native.button(0x110, false)?;
                    s.expect(json!([action(service, first), action(service, last)]))
                },
            );
        }
        s.case(
            "control.slider-drag.brightness",
            "ui-panels-control.md §2.3",
            |s| {
                s.reset()?;
                let (x, y, _, _) = s.open_control()?;
                s.native.motion(x + 100.5, y + 319.)?;
                s.native.button(0x110, true)?;
                s.settle(40);
                s.native.motion(x + 13.4, y + 319.)?;
                s.settle(40);
                s.native.button(0x110, false)?;
                s.expect(json!([
                    action(
                        "backlight",
                        "Set { output: \"WAYLAND-1\", percent: 76.0, sequence: 1 }"
                    ),
                    action(
                        "backlight",
                        "Set { output: \"WAYLAND-1\", percent: 10.0, sequence: 2 }"
                    ),
                    action("backlight", "Finish { output: \"WAYLAND-1\", sequence: 2 }")
                ]))
            },
        );
        s.case(
            "calendar.horizontal-scroll-week",
            "ui-panels-calendar.md §2.4",
            |s| {
                let (x, y, _, _) = s.open_calendar()?;
                s.native.motion(x + 200., y + 250.)?;
                s.native.wheel(20, true)?;
                s.expect(json!([action("calendar", "NextWeek")]))
            },
        );
        for (id, target, uid, start, top, bottom) in [
            (
                "all-day",
                "AllDayEvent(0)",
                "e1",
                "2026-10-23T00:00:00Z",
                110,
                170,
            ),
            (
                "timed",
                "TimedEvent(0)",
                "e3",
                "2026-10-23T09:30:00",
                180,
                380,
            ),
        ] {
            s.case(
                &format!("calendar.open-event.{id}"),
                "ui-panels-calendar.md §2",
                |s| {
                    let (left, _, _, _) = s.open_calendar()?;
                    let mut point = None;
                    'search: for dy in (top..bottom).step_by(10) {
                        for dx in (305..380).step_by(10) {
                            s.native.motion(left + f64::from(dx), f64::from(dy))?;
                            s.settle(3);
                            if s.rpc("snapshot", None)?["panels"][0]["calendar_hover"]
                                == format!("Some({target})")
                            {
                                point = Some((left + f64::from(dx), f64::from(dy)));
                                break 'search;
                            }
                        }
                    }
                    let (x, y) = point.ok_or_else(|| format!("event not visible: {target}"))?;
                    s.rpc("clear", None)?;
                    s.native.click(x, y, 0x110)?;
                    s.expect(json!([action(
                        "calendar",
                        &format!("OpenEvent {{ uid: {uid:?}, start_iso: {start:?} }}")
                    )]))
                },
            );
        }

        s.case("wifi.secure-password-join","ui-panels-wifi.md §3 / WifiFallback.qml:921",|s|{s.reset_seed(Some("secure"))?;s.open_wifi()?;s.click_zone("shell:hyprv-wifi","wifi_hover","Connect(\"Neighbour\")")?;s.settle(350);s.click_zone("shell:hyprv-wifi","wifi_hover","Field(\"Neighbour\")")?;s.native.text("testpass124")?;s.click_zone("shell:hyprv-wifi","wifi_hover","Join(\"Neighbour\")")?;s.expect(json!([action("network","Connect { ssid: \"Neighbour\", password: Some(\"testpass124\"), security: \"WPA3\", enterprise: None }")]))});
        s.case("wifi.empty-password-return", "ui-panels-wifi.md §3", |s| {
            s.reset_seed(Some("secure"))?;
            s.open_wifi()?;
            s.click_zone("shell:hyprv-wifi", "wifi_hover", "Connect(\"Neighbour\")")?;
            s.settle(350);
            s.click_zone("shell:hyprv-wifi", "wifi_hover", "Field(\"Neighbour\")")?;
            s.native.key(28)?;
            s.local("/panels/0/wifi_expanded", json!("Neighbour"))
        });

        s.case(
            "quick-adjust.passthrough",
            "QuickAdjustPopup.qml:112",
            |s| {
                s.reset()?;
                s.call("quickAdjust", "showVolume", vec![])?;
                s.settle(250);
                let (x, y, _) = s.module("Workspace(3)")?;
                s.rpc("clear", None)?;
                s.native.click(x, y, 0x110)?;
                let mut detail = s.expect(json!([action("hyprland", "WorkspaceActivate(3)")]))?;
                if s.rpc("snapshot", None)?["panels"][0]["quick"] != true {
                    return Err("OSD dismissed an unrelated bar click".into());
                }
                detail["osd_still_mapped"] = json!(true);
                Ok(detail)
            },
        );
        s.case("control.profile-cycle", "ControlPanelPopup.qml:297", |s| {
            s.reset()?;
            let (x, y, _, _) = s.open_control()?;
            let mut steps = vec![];
            for (command, profile) in [
                ("ProfileSet(Performance)", "performance"),
                ("ProfileSet(PowerSaver)", "power-saver"),
                ("ProfileSet(Balanced)", "balanced"),
            ] {
                s.rpc("clear", None)?;
                s.native.click(x + 247., y + 247., 0x110)?;
                steps.push(s.expect(json!([action("power", command)]))?);
                // A successful mocked provider acknowledges the requested state.
                // Without this, the optimistic override correctly expires back to
                // the fixed fixture's balanced state before a slow native click.
                s.rpc("patch", Some(json!({"services":{"powerProfile":profile}})))?;
                s.settle(100);
            }
            Ok(json!({"steps":steps,"provider_acknowledgements":true}))
        });
        s.case("mihomo.back", "MihomoControlPanel.qml:513", |s| {
            s.reset()?;
            s.open_control()?;
            s.call_setup_page("mihomo")?;
            s.click_zone("shell:hyprv-control-panel", "control_hover", "MhBack")?;
            s.settle(700);
            s.local("/panels/0/page", json!("Main"))
        });
        s.case("mihomo.start", "MihomoControlPanel.qml:513", |s| {
            s.reset_seed(Some("mihomo-offline"))?;
            s.open_control()?;
            s.call_setup_page("mihomo")?;
            s.click_zone("shell:hyprv-control-panel", "control_hover", "MhPower")?;
            s.expect(json!([action("mihomo", "Service(Start)")]))
        });
        s.case("bar.volume.scroll-decrease", "shell.qml:4138", |s| {
            s.reset()?;
            let (x, y, _) = s.module("Volume")?;
            s.native.motion(x, y)?;
            s.native.wheel(-1, false)?;
            s.expect(json!([action("audio", "Decrease")]))
        });
        for target in ["Battery", "Overflow"] {
            for wheel in [false, true] {
                s.case(
                    &format!("bar.{}.{}", if wheel { "scroll" } else { "hover" }, target),
                    "shell.qml:4152 / shell.qml:4260",
                    |s| {
                        s.reset_seed(Some("tray"))?;
                        let (x, y, _) = s.module(target)?;
                        s.native.motion(x, y)?;
                        if wheel {
                            s.native.wheel(1, false)?;
                            s.expect(json!([]))
                        } else {
                            s.local("/bars/0/hovered", json!(format!("Some({target})")))
                        }
                    },
                );
            }
        }
        for (value, variant) in [("v1", "V1"), ("v2", "V2"), ("v3", "V3"), ("bt", "Bt")] {
            s.case(
                &format!("ipc.desktop.setVariant.{value}"),
                "hypr/scripts/toggle-battery-mode.sh / native desktop IPC",
                |s| {
                    s.reset()?;
                    s.call("desktop", "setVariant", vec![value.into()])?;
                    s.expect(json!([action("theme", &format!("SetVariant({variant})"))]))
                },
            );
        }
        s.case(
            "ipc.desktop.deskflowReflow",
            "hypr/scripts/rotate-screen-ccw.sh / native desktop IPC",
            |s| {
                s.reset()?;
                s.call("desktop", "deskflowReflow", vec![])?;
                s.expect(json!([action("deskflow", "Reflow")]))
            },
        );
        s.case("navigation.bar-wifi", "shell.qml:4182", |s| {
            s.reset()?;
            let (x, y, _) = s.module("Wifi")?;
            s.native.click(x, y, 0x110)?;
            s.local("/panels/0/wifi", json!(true))
        });
        s.case(
            "navigation.bluetooth-page",
            "ControlPanelPopup.qml:580",
            |s| {
                s.reset()?;
                let (x, y, _, _) = s.open_control()?;
                s.native.click(x + 95., y + 103., 0x110)?;
                s.settle(700);
                s.local("/panels/0/page", json!("Bluetooth"))
            },
        );
        for (id, scroll) in [("secondary", false), ("scroll", true)] {
            s.case(&format!("tray.overflow.{id}"), "TrayButton.qml:46 / shell.qml:1204", |s| {
                s.reset_seed(Some("tray"))?;
                let state = s.rpc("snapshot", None)?;
                let visible = state["bars"][0]["modules"].as_array().unwrap().iter()
                    .filter(|m| m["target"].as_str().unwrap().starts_with("Tray("))
                    .count();
                let (x, y, _) = s.module("Overflow")?;
                s.native.click(x, y, 0x110)?;
                s.settle(150);
                let (x, y, _, _) = s.card("shell:hyprv-tray-overflow")?;
                s.rpc("clear", None)?;
                if scroll {
                    s.native.motion(x + 17., y + 27.)?;
                    s.native.wheel(1, false)?;
                    s.expect(json!([action("tray", &format!("Scroll {{ item: \"app-{visible:02}\", delta: -1, horizontal: false }}"))]))
                } else {
                    s.native.click(x + 17., y + 27., 0x111)?;
                    s.expect(json!([action("tray", &format!("SecondaryActivate {{ item: \"app-{visible:02}\", x: {}, y: {} }}", (x + 8.).round() as i32, (y + 46.).round() as i32))]))
                }
            });
        }
        // Per-output lifecycle mutations target only the private compositor socket.
        s.case("outputs.add", "ui-bar.md §1.1", |s| {
            s.reset()?;
            // A newly created nested Wayland output may have no preferred mode.
            // Install its deterministic physical mode before hotplug, through
            // the private compositor's native configuration hook.
            s.compositor("/eval hl.monitor({output=\"WAYLAND-1\",mode=\"3072x1728@60\",position=\"0x0\",scale=1});hl.monitor({output=\"E2E-SECOND\",mode=\"3072x1728@60\",position=\"3072x0\",scale=1})")?;
            let response = s.compositor("/output create wayland E2E-SECOND")?;
            // wl_output hotplug is asynchronous. Wait for host registration,
            // rather than assuming it arrives within a fixed animation delay.
            let until = std::time::Instant::now() + Duration::from_secs(10);
            let state = loop {
                let state = s.rpc("snapshot", None)?;
                if state["outputs"].as_array().unwrap().len() == 2 && state["owners"]["bars"] == 2 { break state; }
                if std::time::Instant::now() >= until { break state; }
                s.settle(50);
            };
            if state["outputs"].as_array().unwrap().len() != 2 || state["owners"]["bars"] != 2 {
                return Err(format!("output add failed: {response}; {state}").into());
            }
            let native = s.compositor("j/monitors")?;
            let monitors = native.as_array().ok_or("native monitors")?;
            for (name, x) in [("WAYLAND-1", 0), ("E2E-SECOND", 3072)] {
                let host = state["outputs"].as_array().unwrap().iter().find(|o| o["name"] == name).ok_or("host output")?;
                let monitor = monitors.iter().find(|o| o["name"] == name).ok_or("native output")?;
                if host["x"].as_f64() != Some(x as f64) || monitor["x"] != x {
                    return Err(format!("initial output origins not calibrated: host {}; native {native}", state["outputs"]).into());
                }
            }
            Ok(json!({"compositor":response,"outputs":state["outputs"],"owners":state["owners"],"native":native}))
        });
        s.case(
            "outputs.input-second",
            "ui-bar.md §per-output instancing",
            |s| {
                let state = s.rpc("snapshot", None)?;
                let output = state["outputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|o| o["name"] == "E2E-SECOND")
                    .ok_or("second output missing")?;
                let bar = state["bars"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|b| b["output"] == "E2E-SECOND")
                    .ok_or("second bar missing")?;
                let module = bar["modules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|m| m["target"] == "Workspace(3)")
                    .ok_or("second workspace")?;
                let r = &module["rect"];
                let gx = output["x"].as_f64().unwrap();
                let gy = output["y"].as_f64().unwrap();
                s.native.extent = (6144, 1728);
                s.rpc("clear", None)?;
                s.native.click(
                    gx + r[0].as_f64().unwrap() + r[2].as_f64().unwrap() / 2.,
                    gy + r[1].as_f64().unwrap() + r[3].as_f64().unwrap() / 2.,
                    0x110,
                )?;
                s.expect(json!([action("hyprland", "WorkspaceActivate(3)")]))
            },
        );
        s.case("outputs.reorder","ui-bar.md §1.1",|s|{let response=s.compositor("/eval hl.monitor({output=\"E2E-SECOND\",mode=\"3072x1728@60\",position=\"0x0\",scale=1});hl.monitor({output=\"WAYLAND-1\",mode=\"3072x1728@60\",position=\"3072x0\",scale=1})")?;s.settle(500);let state=s.rpc("snapshot",None)?;if state["outputs"].as_array().unwrap().len()!=2{return Err(format!("lost outputs: {response}").into());}let native=s.compositor("j/monitors")?;let monitors=native.as_array().ok_or("native monitor list")?;let left=monitors.iter().find(|o|o["name"]=="E2E-SECOND").ok_or("second native output")?;let right=monitors.iter().find(|o|o["name"]=="WAYLAND-1").ok_or("primary native output")?;if left["x"]!=0||right["x"]!=3072||state["primary"]!="WAYLAND-1"{return Err(format!("reorder not applied: {native}").into());}for o in state["outputs"].as_array().unwrap(){let native=monitors.iter().find(|m|m["name"]==o["name"]).unwrap();if o["x"].as_f64()!=native["x"].as_f64(){return Err("host global origin did not follow monitor reorder".into());}}Ok(json!({"response":response,"state":state["outputs"],"primary":state["primary"],"native":native}))});
        s.case("outputs.remove","ui-bar.md §1.1",|s|{let before=s.rpc("snapshot",None)?;let bar=before["bars"].as_array().unwrap().iter().find(|b|b["output"]=="E2E-SECOND").ok_or("second bar")?;let m=bar["modules"].as_array().unwrap().iter().find(|m|m["target"]=="ControlPanel").ok_or("control module")?;let r=&m["rect"];s.native.motion(2000.,1000.)?;s.native.click(r[0].as_f64().unwrap()+r[2].as_f64().unwrap()/2.,r[1].as_f64().unwrap()+r[3].as_f64().unwrap()/2.,0x110)?;s.settle(700);let opened=s.rpc("snapshot",None)?;if !opened["panels"].as_array().unwrap().iter().any(|p|p["output"]=="E2E-SECOND"&&p["control"]==true){return Err("second-output control panel did not open before removal".into());}s.rpc("clear",None)?;let response=s.compositor("/output remove E2E-SECOND")?;s.settle(800);let state=s.rpc("snapshot",None)?;if state["outputs"].as_array().unwrap().len()!=1||state["owners"]["panels"]!=1||state["owners"]["islands"]!=1{return Err(format!("orphan output owners after remove: {response}; {state}").into());}s.compositor("/eval hl.monitor({output=\"WAYLAND-1\",mode=\"3072x1728@60\",position=\"0x0\",scale=1})")?;s.native.extent=(3072,1728);if state["actions"]!=json!([]){return Err(format!("unexpected actions on unplug: {}",state["actions"]).into());}Ok(json!({"response":response,"owners":state["owners"],"open_panel_removed":true}))});
        for scale in [1.5, 2.] {
            s.case(&format!("outputs.scale-{scale}"),"ui-bar.md §0 / fractional-scale-v1",|s|{
            // The compositor remaps the old pointer coordinate on scale change.
            // Park it off the bar and close hover popups before the800ms settle,
            // otherwise a stats dismiss surface can consume the workspace click.
            s.reset()?;
            let response=s.compositor(&format!("/eval hl.monitor({{output=\"WAYLAND-1\",mode=\"3072x1728@60\",position=\"0x0\",scale={scale}}})"))?;s.settle(800);let state=s.rpc("snapshot",None)?;
            let bar=state["surfaces"].as_array().unwrap().iter().find(|v|v["namespace"]=="hyprv-quickshell").ok_or("no bar after scale")?;
            if bar["scale"].as_f64().is_none_or(|v|(v-scale).abs()>0.001){return Err(format!("scale failed {response}; surface {bar}").into());}
            let logical=state["outputs"][0]["width"].as_f64().ok_or("logical width")?;s.native.extent=(logical.round() as u32,state["outputs"][0]["height"].as_f64().unwrap().round() as u32);s.rpc("clear",None)?;let(x,y,_)=s.module("Workspace(3)")?;s.native.click(x,y,0x110)?;let input=s.expect(json!([action("hyprland","WorkspaceActivate(3)")]))?;Ok(json!({"response":response,"outputs":state["outputs"],"bar_surface":bar,"input":input}))
        });
        }
        let pass = s.results.iter().all(|r| r["status"] == "PASS");
        Ok(
            json!({"pass":pass,"suite_complete":true,"cases":s.results,"input":"zwlr_virtual_pointer_v1 + zwp_virtual_keyboard_v1","services":"mock sink; zero native service actors","binary":std::env::current_exe().ok()}),
        )
    })();
    let manifest = match result {
        Ok(v) => v,
        Err(e) => json!({"pass":false,"error":e.to_string(),"cases":[]}),
    };
    let _ = std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    );
    let runtime = hyprv_core::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let acknowledgement = runtime.block_on(ipc::call(
        &ipc::socket_path().unwrap(),
        &Request {
            target: "shell".into(),
            function: "kill".into(),
            args: vec![],
        },
    ));
    let receipt = match acknowledgement {
        Ok(response) => json!({"acknowledged":response.ok,"result":response.result}),
        Err(error) => json!({"acknowledged":false,"error":error.to_string()}),
    };
    let _ = std::fs::write(
        directory.join("shutdown-ack.json"),
        serde_json::to_vec(&receipt).unwrap(),
    );
}
