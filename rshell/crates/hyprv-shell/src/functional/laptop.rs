#[path = "laptop_backend.rs"]
mod backend;
use super::*;
fn check(ok: bool, why: &str) -> Result<(), Box<dyn Error>> {
    if ok {
        Ok(())
    } else {
        Err(why.into())
    }
}
fn primary(s: &Suite) -> Result<String, Box<dyn Error>> {
    Ok(s.rpc("snapshot", None)?["primary"]
        .as_str()
        .ok_or("primary")?
        .to_owned())
}
fn expand(s: &mut Suite) -> Result<(), Box<dyn Error>> {
    let name = primary(s)?;
    s.call("bar", "toggle", vec![name])?;
    s.settle(850);
    Ok(())
}
fn profile_probe(profile: Option<&str>, root: &Path) -> Result<Value, Box<dyn Error>> {
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.arg("--profile-status").env("HYPRV_SYSFS_ROOT", root);
    if let Some(p) = profile {
        cmd.env("HYPRV_PROFILE", p);
    } else {
        cmd.env_remove("HYPRV_PROFILE");
    }
    let out = cmd.output()?;
    check(out.status.success(), &String::from_utf8_lossy(&out.stderr))?;
    Ok(serde_json::from_slice(&out.stdout)?)
}
pub(super) fn run(s: &mut Suite) {
    s.native.extent = (1440, 900);
    for (id, profile, product, battery, want) in [
        (
            "lap.profile.detect.dmi",
            None,
            "XPS 13 9350",
            true,
            "laptop",
        ),
        (
            "lap.profile.detect.desktop",
            None,
            "XPS 8930",
            false,
            "desktop",
        ),
        (
            "lap.profile.override",
            Some("desktop"),
            "XPS 13 9350",
            true,
            "desktop",
        ),
    ] {
        s.case(id, "laptop-deploy.md selector", |s| {
            let root = s.directory.join(id);
            std::fs::create_dir_all(root.join("sys/class/dmi/id"))?;
            std::fs::write(root.join("sys/class/dmi/id/product_name"), product)?;
            if battery {
                let p = root.join("sys/class/power_supply/BAT0");
                std::fs::create_dir_all(&p)?;
                for (k, v) in [("type", "Battery"), ("scope", "System"), ("present", "1")] {
                    std::fs::write(p.join(k), v)?;
                }
            }
            let result = profile_probe(profile, &root)?;
            check(result["profile"] == want, "incorrect process profile")?;
            if profile.is_some() {
                check(
                    profile_probe(Some("laptop"), &root)?["profile"] == "laptop",
                    "reverse override",
                )?;
            }
            std::fs::remove_dir_all(root)?;
            Ok(result)
        });
    }
    s.case("lap.bar.boot.compact", "laptop-bar.md §1", |s| {
        s.reset()?;
        let v = s.rpc("snapshot", None)?;
        check(
            v["bars"]
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["reveal"] == 0.0 && b["expanded"] == false),
            "bar not initially compact",
        )?;
        check(
            v["surfaces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["namespace"] == "hyprv-quickshell" && v["exclusive_zone"] == 25),
            "compact exclusive zone",
        )?;
        check(v["stats_demand"] == "Compact", "compact stats demand")?;
        Ok(v)
    });
    s.case("lap.bar.toggle", "laptop-bar.md §1", |s| {
        s.reset()?;
        expand(s)?;
        let v = s.call("bar", "layoutStatus", vec![primary(s)?])?;
        check(
            v["expanded"] == true && v["reveal"].as_f64().is_some_and(|r| (r - 1.).abs() < 1e-6),
            "did not expand",
        )?;
        Ok(v)
    });
    s.case("lap.bar.toggle.debounce", "laptop-bar.md §1", |s| {
        s.reset()?;
        let n = primary(s)?;
        s.call("bar", "toggle", vec![n.clone()])?;
        s.settle(300);
        s.call("bar", "toggle", vec![n.clone()])?;
        s.settle(600);
        let v = s.call("bar", "layoutStatus", vec![n])?;
        check(v["expanded"] == true, "global debounce dropped")?;
        Ok(v)
    });
    s.case("lap.bar.toggle.permonitor","laptop-bar.md §1",|s|{
  s.reset()?;s.compositor("/output create headless LAPTOP-SECOND")?;s.compositor("/eval hl.monitor({output=\"LAPTOP-SECOND\",mode=\"1440x900@60\",position=\"1440x0\",scale=1})")?;s.settle(500);let n=primary(s)?;s.call("bar","toggle",vec![n.clone()])?;s.settle(850);let v=s.rpc("snapshot",None)?;let bars=v["bars"].as_array().ok_or("bars")?;check(bars.iter().any(|b|b["output"]==n&&b["expanded"]==true)&&bars.iter().any(|b|b["output"]=="LAPTOP-SECOND"&&b["expanded"]==false),"per-output expansion")?;s.compositor("/output remove LAPTOP-SECOND")?;s.settle(300);Ok(v)
 });
    s.case("lap.brightness.osd", "laptop-quickadjust.md §1", |s| {
        s.reset()?;
        s.rpc("patch", Some(json!({"root":{"brightnessPercent":71}})))?;
        s.call("quickAdjust", "showBrightnessLevel", vec!["71".into()])?;
        s.settle(300);
        let v = s.rpc("snapshot", None)?;
        check(
            v["panels"][0]["quick"] == true && v["panels"][0]["brightness"] == 71,
            "odd brightness OSD",
        )?;
        check(v["actions"] == json!([]), "preview wrote hardware")?;
        Ok(v)
    });
    for (id, percent, mode, on_battery) in [
        ("lap.battery.discharge.tick", 20, "discharging", true),
        ("lap.battery.plug", 57, "charging", false),
        ("lap.battery.full", 100, "full", false),
    ] {
        s.case(id,"laptop-bar.md §5 / typed provider event projection",|s|{s.reset()?;let mut backend=backend::Backend::new(s,id)?;backend.step(json!({"battery":{"Percentage":percent,"State":match mode {"charging"=>1,"full"=>4,_=>2}},"on_battery":on_battery}))?;let power=backend.wait(|p|p.battery_percent==Some(f64::from(percent))&&p.on_battery==on_battery)?;s.rpc("patch",Some(json!({"providers":{"power":power}})))?;s.settle(100);let v=s.rpc("snapshot",None)?;check(v["bars"][0]["battery_available"]==true&&v["bars"][0]["battery_percent"].as_f64()==Some(f64::from(percent)),"battery projection")?;check(v["bars"][0]["battery_on_battery"]==on_battery,"battery plugged state")?;check(v["actions"]==json!([]),"provider event emitted action")?;Ok(v) });
    }
    s.case("lap.island.hidden.compact", "laptop-island.md §4", |s| {
        use std::io::Write;
        s.reset_seed(Some("agent"))?;
        let before = s.rpc("snapshot", None)?;
        check(!before["islands"][0]["agents"].as_array().ok_or("agents")?.is_empty(), "missing active agent")?;
        let exe = std::env::var_os("HYPRV_AGENT_HOOK_BINARY").map(std::path::PathBuf::from).unwrap_or_else(||Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hyprv-agent-hook"));
        let dir = s.directory.join("compact-hook"); std::fs::create_dir_all(&dir)?;
        let mut child = std::process::Command::new(exe).args(["--source", "claude", "--event", "PreToolUse"]).env("HYPRV_AGENT_ISLAND_DIR", &dir).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn()?;
        child.stdin.take().ok_or("stdin")?.write_all(br#"{"session_id":"compact-agent","cwd":"/tmp","tool_name":"Read","tool_input":{"file_path":"/tmp/fresh-model.txt"}}"#)?;
        check(child.wait()?.success(), "hook failed")?;
        let mut state:Value = serde_json::from_slice(&std::fs::read(dir.join("state.json"))?)?;
        let (rows, _) = hyprv_svc_desktop::agent_island::model::public_sessions(&mut state, chrono::Local::now().timestamp_millis() as f64 / 1000.);
        let sessions:Vec<_> = rows.into_iter().map(|a|json!({"id":a.id,"source":a.source,"source_label":a.source_label,"title":a.title,"project":a.project,"status":a.status,"current_tool":a.current_tool,"detail":a.detail,"progress":a.progress})).collect();
        check(!sessions.is_empty(), "hook projected no sessions")?;
        s.rpc("patch",Some(json!({"root":{"agentSessions":sessions}})))?;
        s.settle(150);
        let hidden = s.rpc("snapshot",None)?;
        check(hidden["islands"][0]["agents"] != before["islands"][0]["agents"], "hidden inputs stale")?;
        check(hidden["islands"][0]["visual_rev"] == before["islands"][0]["visual_rev"], "hidden event repainted")?;
        check(hidden["islands"][0]["next_deadline"].is_null() && hidden["islands"][0]["animating"] == false, "hidden event scheduled animation")?;
        s.call("bar", "toggle", vec![primary(s)?])?;
        s.settle(16);
        let first = s.rpc("snapshot",None)?;
        check(first["islands"][0]["agents"] == hidden["islands"][0]["agents"], "first reveal frame stale")?;
        s.settle(850);
        let revealed = s.rpc("snapshot",None)?;
        check(revealed["islands"][0]["agents"] == hidden["islands"][0]["agents"], "reveal inputs stale")?;
        check(revealed["islands"][0]["bar_hidden"] == false, "reveal stayed hidden")?;
        Ok(json!({"before":before,"hidden":hidden,"first":first,"revealed":revealed,"projection":"native hook/model rows injected into mocked UI graph"}))
    });
    s.case("lap.media.compact.idle", "laptop-island.md §4", |s| {
        s.reset_seed(Some("music"))?;
        let v = s.rpc("snapshot", None)?;
        check(
            v["islands"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["bar_hidden"] == true && i["animating"] == false),
            "hidden media animated",
        )?;
        check(v["stats_demand"] == "Compact", "hidden detailed sampling")?;
        Ok(v)
    });
    s.case("lap.media.expand", "laptop-island.md §4", |s| {
        s.reset()?;
        expand(s)?;
        let v = s.rpc("snapshot", None)?;
        check(
            v["islands"][0]["bar_hidden"] == false,
            "island did not resume on first reveal",
        )?;
        check(v["stats_demand"] == "Detailed", "expanded metrics")?;
        Ok(v)
    });
    s.case("lap.cp.nofans", "laptop-panels.md §1", |s| {
        s.reset()?;
        s.call("controlPanel", "toggle", vec![])?;
        s.settle(600);
        let v = s.rpc("snapshot", None)?;
        check(
            v["panels"][0]["control"] == true && v["panels"][0]["fan_available"] == false,
            "fan controls exposed",
        )?;
        check(v["actions"] == json!([]), "opening CP sent hardware action")?;
        Ok(v)
    });
    s.case(
        "lap.power.profile.set",
        "native laptop power actor / private PPD",
        |s| {
            let mut b = backend::Backend::new(s, "profile-set")?;
            b.profile_set()
        },
    );
    s.case(
        "lap.power.profile.external",
        "native PropertiesChanged / private PPD",
        |s| {
            let mut b = backend::Backend::new(s, "profile-external")?;
            b.step(json!({"profile":"performance"}))?;
            let state =
                b.wait(|p| p.profile == hyprv_svc_power::power::PowerProfile::Performance)?;
            Ok(json!({"native":state}))
        },
    );
    s.case(
        "lap.lid.close_open",
        "native login1 LidClosed signals",
        |s| {
            let mut b = backend::Backend::new(s, "lid")?;
            b.step(json!({"lid_closed":true}))?;
            let closed = b.wait(|p| p.lid_closed == Some(true))?;
            b.step(json!({"lid_closed":false}))?;
            let open = b.wait(|p| p.lid_closed == Some(false))?;
            Ok(json!({"closed":closed,"open":open}))
        },
    );
    s.case(
        "lap.sleep.resume",
        "native login1 PrepareForSleep signals",
        |s| {
            let mut b = backend::Backend::new(s, "sleep")?;
            b.step(json!({"prepare_for_sleep":true}))?;
            let sleep = b.wait(|p| p.sleeping)?;
            b.step(json!({"prepare_for_sleep":false}))?;
            let wake = b.wait(|p| !p.sleeping)?;
            Ok(json!({"sleep":sleep,"wake":wake}))
        },
    );
    s.case(
        "lap.privhelper.inert",
        "real laptop fan actor / private bus and fake sysfs",
        |s| backend::Backend::new(s, "inert")?.inert_fan(),
    );
    s.case("lap.battery.popup.open","laptop-bar.md §5 native click",|s|{s.reset()?;s.rpc("patch",Some(json!({"providers":{"power":{"battery_present":true,"battery_percent":57,"battery_mode":"discharging"}}})))?;expand(s)?;let(x,y,_)=s.module("Battery")?;s.native.click(x,y,0x110)?;s.settle(550);let v=s.rpc("snapshot",None)?;check(v["popups"]["battery"]==true,"battery popup did not open")?;Ok(v)});
    s.case(
        "lap.battery.popup.close",
        "laptop-bar.md §5 native outside click",
        |s| {
            s.native.click(700., 700., 0x110)?;
            s.settle(650);
            let v = s.rpc("snapshot", None)?;
            check(
                v["popups"]["battery"] == false,
                "battery popup did not close",
            )?;
            check(
                !v["surfaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["namespace"] == "shell:hyprv-battery-info"),
                "closed battery GPU surface retained",
            )?;
            Ok(v)
        },
    );
    s.case(
        "lap.island.agent.focus",
        "laptop-island.md read-only focus",
        |s| {
            s.reset_seed(Some("agent"))?;
            expand(s)?;
            let (x, y, _, h) = s.island()?;
            s.native.motion(x + 20., y + h / 2.)?;
            s.settle(700);
            check(
                s.rpc("snapshot", None)?["islands"][0]["agent_expanded"] == true,
                "agent did not expand",
            )?;
            let (x, y, _, h) = s.island()?;
            s.rpc("clear", None)?;
            s.native.click(x + 80., y + h - 45., 0x110)?;
            s.expect(json!([action(
                "agent",
                "FocusSession(\"codex-session-0\")"
            )]))
        },
    );

    s.case(
        "lap.thermal.source",
        "native metric lease / pkg-hwmon-missing fixture",
        |s| backend::Backend::new(s, "thermal")?.thermal(),
    );
    s.case(
        "lap.brightness.slider",
        "laptop-quickadjust.md Set-only drag",
        |s| {
            s.reset()?;
            s.call("controlPanel", "toggle", vec![])?;
            s.settle(600);
            let (x, y, _, _) = s.card("shell:hyprv-control-panel")?;
            s.rpc("clear", None)?;
            s.native.motion(x + 67., y + 319.)?;
            s.native.button(0x110, true)?;
            s.settle(40);
            s.native.motion(x + 80.4, y + 319.)?;
            s.settle(20);
            s.native.motion(x + 87.1, y + 319.)?;
            s.settle(20);
            s.native.motion(x + 95.14, y + 319.)?;
            s.settle(20);
            let before = s.pixels("shell:hyprv-control-panel", (x, y + 290., 134., 62.))?;
            s.rpc("patch",Some(json!({"root":{"brightnessPercent":3}})))?;
            s.settle(20);
            let stale = s.pixels("shell:hyprv-control-panel", (x, y + 290., 134., 62.))?;
            check(before["hash"] == stale["hash"], "stale provider overwrote drag")?;
            s.native.button(0x110, false)?;
            s.settle(150);
            s.expect(json!([
                action(
                    "backlight",
                    "Set { output: \"WAYLAND-1\", percent: 50.0, sequence: 1 }"
                ),
                action(
                    "backlight",
                    "Set { output: \"WAYLAND-1\", percent: 60.0, sequence: 2 }"
                ),
                action("backlight", "Set { output: \"WAYLAND-1\", percent: 65.0, sequence: 3 }"),
                action("backlight", "Set { output: \"WAYLAND-1\", percent: 71.0, sequence: 4 }"),
                action("backlight", "Refresh"
                )
            ]))
        },
    );
    s.case("lap.brightness.plan", "real pinned laptop controls.lua in private Hyprland; inert timer", |s| {
        let source = include_str!("../../../hyprv-svc-power/tests/fixtures/laptop-controls.lua");
        let profile = include_str!("../../../hyprv-svc-power/tests/fixtures/laptop-controls-profile.lua");
        let path = s.directory.join("real-lua-plans.csv");
        let lua = format!("local cfg=assert(load({},'@profile','t',{{}}))();local env=setmetatable({{hl={{timer=function() return {{}} end}},require=function(n) assert(n=='hypr.controls-profile');return cfg end}},{{__index=_G}});local m=assert(load({},'@pinned-laptop-controls','t',env))();local f=assert(io.open({},'w'));for l=0,100 do local r,d,c=m.plan(l);f:write(l,',',r,',',d,',',c,'\\n') end;f:close();return true", serde_json::to_string(profile)?, serde_json::to_string(source)?, serde_json::to_string(&path.to_string_lossy())?);
        s.compositor(&format!("/eval {lua}"))?;
        let rows=std::fs::read_to_string(&path)?;
        let config:hyprv_svc_power::backlight::OutputConfig=serde_json::from_value(json!({"output":"eDP-1","mode":"preferred","backend":{"Sysfs":{"device":"/unused"}},"desktop":153.,"peak":400.,"sdr_min":0.,"monitor_fields":{},"curve":"Laptop","auto_brightness":true}))?;
        let mut count=0;
        for line in rows.lines() {let n:Vec<f64>=line.split(',').map(str::parse).collect::<Result<_,_>>()?;check(n.len()==4,"Lua row")?;check(hyprv_svc_power::backlight::plan_for_config(n[0],400,&config) == n[1] as u32,"native raw plan differs from real Lua")?;let white=hyprv_svc_power::backlight::luminance(n[0],&config);check(white==n[2]&&white==n[3],"native white differs from real Lua")?;count+=1;}
        check(count==101,"incomplete real Lua plan")?;
        Ok(json!({"levels":count,"oracle":"svc-power pinned origin/laptop controls.lua; M.plan only; no timer/action/hardware call"}))
    });
    s.case(
        "lap.brightness.wheel",
        "laptop-quickadjust.md one-percent wheel",
        |s| {
            s.reset()?;
            s.call("quickAdjust", "showBrightness", vec![])?;
            s.settle(600);
            let (x, y, _, _) = s.card("shell:hyprv-quick-adjust")?;
            s.rpc("clear", None)?;
            s.native.motion(x + 100., y + 31.)?;
            s.native.wheel(-1, false)?;
            let ignored=s.expect(json!([]))?;
            s.call("quickAdjust","showBrightnessIncrease",vec![])?;
            let state=s.rpc("snapshot",None)?;
            check(state["panels"][0]["brightness"]==51,"one-percent IPC preview")?;
            check(state["actions"]==json!([]),"brightness notification wrote hardware")?;
            Ok(json!({"native_wheel":ignored,"preview":state,"legacy":"ControlPanelSlider.qml:112-166 has no onWheel/handler; wheel is ignored, IPC preview changes by1"}))
        },
    );
    s.case("lap.island.readonly","laptop-island.md D1 user decision",|s|{s.reset_seed(Some("agent"))?;expand(s)?;let before=s.rpc("snapshot",None)?;s.rpc("patch",Some(json!({"root":{"agentPending":{"id":"pending-test","kind":"approval","question":"test permission"},"agentPendingCount":1}})))?;s.settle(300);let after=s.rpc("snapshot",None)?;check(before["islands"][0]["rect"]==after["islands"][0]["rect"],"pending request created a response card")?;check(after["actions"]==json!([]),"pending request emitted response action")?;Ok(after)});

    s.case(
        "lap.wifi.scan.visible",
        "laptop-services.md visibility-scoped scan",
        |s| {
            s.reset_seed(Some("wifi"))?;
            check(
                s.rpc("snapshot", None)?["wifi_demand"] == false,
                "hidden scanner active",
            )?;
            expand(s)?;
            s.open_wifi()?;
            check(
                s.rpc("snapshot", None)?["wifi_demand"] == true,
                "visible scanner inactive",
            )?;
            s.native.click(700., 700., 0x110)?;
            s.settle(80);
            let v = s.rpc("snapshot", None)?;
            check(
                v["wifi_demand"] == false,
                "scanner held during exit animation",
            )?;
            Ok(v)
        },
    );
    s.case("lap.island.hook.nonblocking","read-only native permission hook",|s|{
  use std::io::Write;
  let exe=std::env::var_os("HYPRV_AGENT_HOOK_BINARY").map(std::path::PathBuf::from).unwrap_or_else(||Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hyprv-agent-hook"));
  let dir=s.directory.join("permission-hook");std::fs::create_dir_all(&dir)?;
  let start=std::time::Instant::now();let mut child=std::process::Command::new(&exe).args(["--source","claude","--event","PermissionRequest"]).env("HYPRV_AGENT_ISLAND_DIR",&dir).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn()?;
  child.stdin.take().ok_or("hook stdin")?.write_all(br#"{"session_id":"laptop-permission-e2e","cwd":"/tmp","tool_name":"Bash","tool_input":{"command":"true"}}"#)?;
  loop {if let Some(status)=child.try_wait()?{check(status.success(),"hook failed")?;break;}
if start.elapsed()>Duration::from_secs(1){let _=child.kill();let _=child.wait();return Err("permission hook blocked".into());}s.settle(5);}
  let state:Value=serde_json::from_slice(&std::fs::read(dir.join("state.json"))?)?;check(state["pending_requests"].as_object().is_some_and(|r|r.is_empty()),"hook retained response request")?;Ok(json!({"elapsed_ms":start.elapsed().as_millis(),"binary":exe,"state":state}))
 });
    s.case("lap.island.list_inplace","laptop-island.md native scroll survives provider update",|s|{
  s.reset_seed(Some("agent"))?;let sessions:Vec<_>=(0..9).map(|i|json!({"id":format!("session-{i}"),"source":"codex","source_label":"Codex","title":format!("Session {i}"),"project":"rshell","status":"running","progress":0.5})).collect();
  s.rpc("patch",Some(json!({"root":{"agentSessions":sessions}})))?;expand(s)?;let(x,y,_,h)=s.island()?;s.native.motion(x+20.,y+h/2.)?;s.settle(700);let(x,y,w,h)=s.island()?;let before=s.pixels("hyprv-quickshell",(x,y,w,h))?;s.native.motion(x+w/2.,y+h-65.)?;s.native.wheel(6,false)?;s.settle(600);let scrolled=s.pixels("hyprv-quickshell",(x,y,w,h))?;check(before["hash"]!=scrolled["hash"],"list did not scroll")?;
  let mut sessions=sessions;sessions[0]["title"]=json!("Updated offscreen");s.rpc("patch",Some(json!({"root":{"agentSessions":sessions}})))?;s.settle(600);let after=s.pixels("hyprv-quickshell",(x,y,w,h))?;check(scrolled["hash"]==after["hash"],"provider update reset scrolled list")?;Ok(json!({"before":before,"scrolled":scrolled,"after":after}))
 });
 s.case("lap.egpu.ui","laptop no-eGPU graph capability; no active QML eGPU card",|s|{backend::Backend::new(s,"egpu")?.no_egpu()});

 for (id,target) in [("lap.strip.control","ControlPanel"),("lap.strip.stats","Cpu"),("lap.strip.clock","StripClock"),("lap.strip.workspace","Workspace(3)")] {
  s.case(id,"laptop-bar.md §2 corner-window native input",|s|{
   s.reset()?;let state=s.rpc("snapshot",None)?;let strip=state["bars"][0]["strip"].as_array().ok_or("strip")?;let r=strip.iter().find(|r|r["target"]==target).ok_or("strip target")?["rect"].as_array().ok_or("strip rect")?;let(x,y)=(r[0].as_f64().unwrap()+r[2].as_f64().unwrap()/2.,r[1].as_f64().unwrap()+r[3].as_f64().unwrap()/2.);
   s.native.click(x,y,0x110)?;s.settle(900);
   if target=="Workspace(3)" {return s.expect(json!([action("hyprland","WorkspaceActivate(3)")]))}
   let state=s.rpc("snapshot",None)?;
   match target {"ControlPanel"=>check(state["panels"][0]["control"]==true,"compact CP click")?,"Cpu"=>check(state["panels"][0]["stats"]==true,"compact stats click")?,"StripClock"=>check(state["bars"][0]["expanded"]==true&&state["bars"][0]["calendar"]==true,"compact clock expand/calendar")?,_=>{}}
   check(state["actions"]==json!([]),"duplicate service action from local strip click")?;Ok(state)
  });
 }

 s.case("lap.tooltip.cpu","laptop-bar.md §4 native hover deadline and unmap",|s|{
  s.reset()?;s.rpc("patch",Some(json!({"providers":{"sysstats":{"package_power_w":12.5,"temperature_c":45.}}})))?;expand(s)?;let(x,y,_)=s.module("CpuPower")?;s.native.motion(x,y)?;s.settle(450);let shown=s.rpc("snapshot",None)?;let surface=shown["surfaces"].as_array().unwrap().iter().find(|r|r["namespace"]=="shell:hyprv-cpu-temperature-tooltip").ok_or("CPU tooltip did not map")?;check(surface["size"][0].as_f64().is_some_and(|w|w<400.)&&surface["size"][1].as_f64().is_some_and(|h|h<200.),"tooltip keeps output-sized target")?;s.native.motion(700.,700.)?;s.settle(80);let hidden=s.rpc("snapshot",None)?;check(!hidden["surfaces"].as_array().unwrap().iter().any(|r|r["namespace"]=="shell:hyprv-cpu-temperature-tooltip"),"tooltip surface retained after hover")?;Ok(json!({"shown":shown,"hidden":hidden}))
 });
 s.case("lap.brightness.probe","laptop-quickadjust.md90ms transient preview",|s|{s.reset()?;s.call("quickAdjust","showBrightnessIncrease",vec![])?;let first=s.rpc("snapshot",None)?;check(first["panels"][0]["brightness"]==51,"preview not1percent")?;s.settle(150);let after=s.rpc("snapshot",None)?;check(after["panels"][0]["brightness"]==50,"90ms provider value did not supersede preview")?;check(after["actions"]==json!([]),"notification probe wrote hardware")?;Ok(json!({"first":first,"after":after}))});

}
