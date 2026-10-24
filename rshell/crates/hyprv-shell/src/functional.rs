//! Private-compositor functional tests. Guarded RPC sets fixtures and observes
//! state; all tested gestures enter via virtual input protocols, never replay_input.
use super::*;
use hyprv_core::{store::Topic, tokio};
use serde_json::{json, Value};
#[path = "functional/native.rs"]
mod native;
#[path = "functional/suite.rs"]
mod suite;
thread_local! {static INPUTS:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};}
static IDLE_ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn main_voluntary_switches() -> Result<u64, Box<dyn std::error::Error>> {
    let status = std::fs::read_to_string("/proc/thread-self/status")?;
    Ok(status.lines().find_map(|line| line.strip_prefix("voluntary_ctxt_switches:")
        .and_then(|value| value.trim().parse().ok())).ok_or("main thread scheduling counters unavailable")?)
}
pub(super) fn observe(event: &InputEvent) {
    let name = match event {
        InputEvent::Pointer { event, .. } => format!("Pointer({:?})", event.kind),
        InputEvent::Scroll { event, .. } => format!("Scroll({event:?})"),
        InputEvent::Key { pressed, .. } => format!("Key({pressed})"),
        _ => return,
    };
    INPUTS.with(|v| v.borrow_mut().push(name));
}
fn snapshot(a: &Assembly, engine: &mut Engine) -> Value {
    let popup_rects=a.primary.as_ref().and_then(|name|a.owners.bars.get(name)).map(|c|{
        let rect=|kind|{let r=a.popups.card_rect(&c.model,kind);json!([r.x,r.y,r.width,r.height])};
        json!({"shell:hyprv-tray-menu":rect(bar::popups::Kind::Menu),"shell:hyprv-tray-overflow":rect(bar::popups::Kind::Overflow),"shell:hyprv-battery-info":a.popups.battery.surface.mapped.then(||rect(bar::popups::Kind::Battery))})
    });
    let modules:Vec<_>=a.owners.bars.iter().map(|(name,c)|{
        let l=bar::layout::compute(&c.model,&c.model.theme);
        let modules:Vec<_>=l.modules.iter().filter_map(|m| l.target(&m.target).map(|r|json!({"target":format!("{:?}",m.target),"rect":[r.x,r.y,r.width,r.height],"interactive":m.interactive}))).collect();
        let strip:Vec<_>=if c.model.laptop() {bar::surfaces::compact_strip(&c.model).items.into_iter().map(|(t,r)|json!({"target":format!("{t:?}"),"rect":[r.x,r.y,r.width,r.height]})).collect()}else{Vec::new()};
        json!({"strip":strip,"output":name,"expanded":c.model.expanded,"reveal":c.model.reveal,"profile":format!("{:?}",c.model.profile),"top_corner_offset":c.model.top_corner_offset(),"battery_available":c.model.battery_available(),"battery_percent":c.model.battery_percent(),"battery_on_battery":c.model.battery_on_battery(),"modules":modules,"calendar":c.model.calendar_presented,"hovered":format!("{:?}",c.model.hovered),"island":[l.center.x,l.center.y+l.content_y,l.center.width,l.center.height]})
    }).collect();
    let surfaces:Vec<_>=engine.surfaces().into_iter().filter_map(|(id,config)|{
        engine.surface_geometry(id).map(|(size,scale)|{let card=engine.scene(id).and_then(|scene|surface_card(scene)).map(|r|json!([r.x,r.y,r.width,r.height]));json!({"namespace":config.namespace,"size":[size.0,size.1],"scale":scale,"origin":[config.margins.3,config.margins.0],"input_only":config.input_only,"exclusive_zone":config.exclusive_zone,"card":card})})
    }).collect();
    let outputs: Vec<_> = a
        .outputs
        .iter()
        .map(
            |(name, (_, o))| json!({"name":name,"width":o.width,"height":o.height,"scale":o.scale,"x":o.global_x,"y":o.global_y}),
        )
        .collect();
    let pages:Vec<_>=a.owners.panels.iter().map(|(name,p)| {let p=p.borrow();json!({"output":name,"page":format!("{:?}",p.control.page),"control":p.control.is_mapped(),"wifi":p.wifi.is_mapped(),"stats":p.stats.is_mapped(),"quick":p.quick.mapped,"calendar_hover":format!("{:?}",p.calendar.inter.hovered),"control_hover":format!("{:?}",p.control.inter.hovered),"wifi_hover":format!("{:?}",p.wifi.inter.hovered),"wifi_expanded":p.wifi.expanded,"bluetooth_show_unnamed":a.bluetooth_show_unnamed,"bluetooth_visible_devices":panels::control_panel::visible_devices(&a.panel_model(name)).len(),"brightness":a.panel_model(name).controls.brightness_percent,"fan_available":a.panel_model(name).controls.fan_available,"calendar_size":[p.calendar.width,p.calendar.height]})}).collect();
    let islands: Vec<_> = a
        .owners
        .islands
        .iter()
        .map(|(name, i)| {let i=i.borrow();let g=i.geometry();let d=i.derived();json!({"output":name,"rect":[g.rect.x,g.rect.y,g.rect.width,g.rect.height],"expanded":d.expanded,"music_expanded":d.music_expanded,"agent_expanded":d.agent_expanded,"bar_hidden":i.bar_hidden(),"visual_rev":i.visual_rev(),"agents":i.inputs().agents.iter().map(|a|json!({"id":a.id,"title":a.title,"detail":a.detail,"status":a.status})).collect::<Vec<_>>(),"animating":i.animating(a.now),"next_deadline":i.next_deadline().map(|d|d.as_millis() as u64),"derived":format!("{d:?}")})})
        .collect();
    json!({"profile":format!("{:?}",hyprv_core::profile::profile()),"stats_demand":format!("{:?}",*a.hub.visibility.bar_stats().borrow()),"wifi_demand":*a.hub.visibility.gate(Consumer::Wifi).borrow(),"media_demand":*a.hub.visibility.gate(Consumer::Media).borrow(),"media_presentation":*a.hub.visibility.gate(Consumer::Presentation).borrow(),"popup_rects":popup_rects,"outputs":outputs,"primary":a.primary,"bars":modules,"surfaces":surfaces,"panels":pages,"islands":islands,"actions":a.hub.recorded(false),"popups":{"menu":a.popups.menu.surface.mapped,"menu_hover":a.popups.menu.hovered,"menu_stack":a.popups.menu.stack.len(),"menu_scroll":a.popups.menu.scroll,"overflow":a.popups.overflow.surface.mapped,"battery":a.popups.battery.surface.mapped},"inputs":INPUTS.with(|v|v.borrow().clone()),"owners":{"bars":a.owners.bars.len(),"panels":a.owners.panels.len(),"islands":a.owners.islands.len()},"pending_deadline":a.owners.next_deadline(a.now).map(|t|t.as_millis() as u64)})
}
pub(super) fn dispatch(
    state: &Rc<RefCell<Assembly>>,
    engine: &mut Engine,
    request: Request,
) -> IpcResponse {
    let result = (|| -> Result<Value, Box<dyn std::error::Error>> {
        let mut a = state.borrow_mut();
        if !a.hub.is_mock() {
            return Err("test RPC disabled".into());
        }
        match request.function.as_str() {
            "idle-arm" if std::env::var("HYPRV_IDLE_TEST").as_deref() == Ok("1") => {
                IDLE_ARMED.store(true, std::sync::atomic::Ordering::Release);
                Ok(json!({}))
            }
            "snapshot" => Ok(snapshot(&a, engine)),
            "profile" => Ok(json!({"csv":hyprv_ui::diagnostics::csv(),"main_voluntary":main_voluntary_switches()?})),
            "profile-clear" => {hyprv_ui::diagnostics::clear(); Ok(json!({"main_voluntary":main_voluntary_switches()?}))},
            "clear" => {
                a.hub.recorded(true);
                INPUTS.with(|v| v.borrow_mut().clear());
                Ok(json!({}))
            }
            "reset" => {
                let value: Value =
                    serde_json::from_str(request.args.first().ok_or("reset needs JSON")?)?;
                let path =
                    std::path::PathBuf::from(value["fixture"].as_str().ok_or("fixture path")?);
                let mut f = crate::fixture::Fixture::load(&path, true, 1., a.assets.clone())?;
                f.advance_to(a.now.as_secs_f64() * 1000.);
                a.fixture = Some(f);
                a.owners = OutputOwners::default();
                a.bar_expansion = Default::default();
                a.popups = Default::default();
                a.position = Default::default();
                a.hub.reset_mock_sequence();
                a.input_values.clear();
                a.input_epoch = false;
                a.bluetooth_show_unnamed = true;
                a.dark = true;
                a.dirty = true;
                Ok(json!({}))
            }
            "pixels" => {
                drop(a);
                let request: Value =
                    serde_json::from_str(request.args.first().ok_or("pixel request")?)?;
                let directory = std::path::PathBuf::from(
                    std::env::var_os("XDG_RUNTIME_DIR").ok_or("private runtime")?,
                )
                .join("pixel-snapshot");
                engine.render_now();
                let captures = engine.dump_surfaces(&directory)?;
                let c = captures
                    .iter()
                    .find(|c| request["namespace"] == c.config.namespace && !c.config.input_only)
                    .ok_or("capture surface")?;
                let image = crate::fullscreen::Image::load(&directory.join(&c.file))?;
                let r = request["rect"].as_array().ok_or("pixel rect")?;
                let left = ((r[0].as_f64().ok_or("x")? - f64::from(c.config.margins.3)) * c.scale)
                    .round()
                    .max(0.) as usize;
                let top = ((r[1].as_f64().ok_or("y")? - f64::from(c.config.margins.0)) * c.scale)
                    .round()
                    .max(0.) as usize;
                let right = (left + (r[2].as_f64().ok_or("w")? * c.scale).round() as usize)
                    .min(image.width);
                let bottom = (top + (r[3].as_f64().ok_or("h")? * c.scale).round() as usize)
                    .min(image.height);
                if right <= left || bottom <= top {
                    return Err("empty capture region".into());
                }
                let mut hash = 0xcbf29ce484222325u64;
                for y in top..bottom {
                    for byte in &image.pixels[((y * image.width + left) * 4) as usize
                        ..((y * image.width + right) * 4) as usize]
                    {
                        hash ^= u64::from(*byte);
                        hash = hash.wrapping_mul(0x100000001b3);
                    }
                }
                Ok(
                    json!({"hash":format!("{hash:016x}"),"pixels":(right-left)*(bottom-top),"source":"retained EGL RGBA8"}),
                )
            }
            "page" => {
                let name = a.primary.clone().ok_or("no primary")?;
                let value: Value = serde_json::from_str(request.args.first().ok_or("page")?)?;
                let page = match value.as_str() {
                    Some("mihomo") => panels::control_panel::Page::Mihomo,
                    _ => return Err("unknown setup page".into()),
                };
                let now = a.now;
                a.owners.panels[&name]
                    .borrow_mut()
                    .control
                    .show_page(page, now);
                a.dirty = true;
                Ok(json!({}))
            }
            "patch" => {
                let patch: Value = serde_json::from_str(request.args.first().ok_or("patch JSON")?)?;
                a.fixture.as_mut().unwrap().apply(&patch);
                a.dirty = true;
                Ok(json!({}))
            }
            "quit" => {
                engine.quit();
                Ok(json!({}))
            }
            _ => Err("unknown test RPC".into()),
        }
    })();
    match result {
        Ok(v) => IpcResponse::success(v.to_string()),
        Err(e) => IpcResponse::error(e.to_string()),
    }
}
pub(super) fn install(
    app: &Application,
    state: Rc<RefCell<Assembly>>,
    directory: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let (send, receive) = ServiceRuntime::bridge::<ipc::UiRequest>();
    let rpc_state = state.clone();
    app.event_loop_handle()
        .insert_source(receive, move |event, _, engine| {
            if let Event::Msg(message) = event {
                let redraw = !matches!(
                    message.request.function.as_str(),
                    "snapshot" | "clear" | "pixels" | "profile" | "profile-clear"
                );
                let response = dispatch(&rpc_state, engine, message.request);
                let _ = message.reply.send(response);
                if redraw {
                    engine.request_redraw();
                }
            }
        })?;
    let socket =
        std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("private runtime")?)
            .join("functional.sock");
    let listener = std::os::unix::net::UnixListener::bind(socket)?;
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let mut data = Vec::new();
            if stream.read_to_end(&mut data).is_err() {
                continue;
            }
            let Ok(request) = serde_json::from_slice::<Request>(&data) else {
                continue;
            };
            let quit = request.function == "quit";
            let (reply, response) = hyprv_core::tokio::sync::oneshot::channel();
            if send.send(ipc::UiRequest { request, reply }).is_err() {
                break;
            }
            if let Ok(response) = response.blocking_recv() {
                let _ = stream.write_all(&serde_json::to_vec(&response).unwrap());
            }
            if quit {
                break;
            }
        }
    });
    {
        let mut a = state.borrow_mut();
        a.fixture.as_mut().unwrap().advance_to(2500.);
        a.now = Duration::from_millis(2500);
        a.dirty = true;
    }
    let start = std::time::Instant::now();
    if std::env::var("HYPRV_IDLE_TEST").as_deref() == Ok("1") {
        let runtime = state.borrow().hub.runtime.clone();
        let topic = Topic::new(hyprv_svc_power::sysstats::State::default());
        let (visible_owner, visible) = tokio::sync::watch::channel(true);
        let (ping, receive) = hyprv_core::calloop::ping::make_ping()?;
        app.event_loop_handle().insert_source(receive, |_, _, engine| {
            hyprv_ui::diagnostics::count("idle.stats_ui_wake");
            engine.request_redraw();
        })?;
        if std::env::var_os("HYPRV_IDLE_RAW_STATS_CONTROL").is_some() {
            // Negative control: the previous unprojected production bridge must
            // fail the gate even though its redundant updates paint no pixels.
            runtime.spawn(crate::services::forward(topic.clone(), visible,
                runtime.shutdown_signal(), ping));
        } else {
            runtime.spawn(crate::services::forward_projected(topic.clone(), visible,
                runtime.shutdown_signal(), ping, crate::services::bar_stats_key, None));
        }
        let mut stop = runtime.shutdown_signal();
        runtime.spawn(async move {
            let _visible = visible_owner;
            let mut value = hyprv_svc_power::sysstats::State::default();
            loop {
                tokio::select! { _ = tokio::time::sleep(Duration::from_secs(1)) => {}, _ = stop.changed() => return }
                if IDLE_ARMED.load(std::sync::atomic::Ordering::Acquire) {
                    hyprv_ui::diagnostics::count("idle.stats_tick");
                    value.generation += 1;
                    value.elapsed += Duration::from_secs(1);
                    value.cpu_history.push_back(Some(0.));
                    topic.publish(value.clone());
                }
            }
        });
    }
    app.event_loop_handle().insert_source(
        Timer::from_duration(Duration::from_millis(16)),
        move |_, _, engine| {
            if IDLE_ARMED.load(std::sync::atomic::Ordering::Acquire) {
                return TimeoutAction::Drop;
            }
            if start.elapsed() > Duration::from_secs(1200) {
                engine.quit();
                return TimeoutAction::Drop;
            }
            let mut a = state.borrow_mut();
            let ms = 2500. + start.elapsed().as_secs_f64() * 1000.;
            a.fixture.as_mut().unwrap().advance_to(ms);
            a.now = Duration::from_millis(ms.floor() as u64);
            // This private clock wakes controllers only while they animate or
            // reach a deadline. Read-only RPCs and idle fixtures do not repaint.
            let active = a.owners.next_deadline(a.now).is_some_and(|t| t <= a.now)
                || a.popups.next_deadline().is_some_and(|t| t <= a.now)
                || a.popups.animating()
                || a.owners.bars.values().any(|b| b.animating())
                || a.owners
                    .islands
                    .values()
                    .any(|i| i.borrow().animating(a.now))
                || a.owners
                    .panels
                    .iter()
                    .any(|(name, p)| p.borrow().animating(&a.panel_model(name)));
            drop(a);
            if active {
                engine.request_redraw();
            }
            TimeoutAction::ToDuration(Duration::from_millis(16))
        },
    )?;
    if std::env::var("HYPRV_FUNCTIONAL_PROBE").as_deref() == Ok("1") {
        return Ok(());
    }
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| suite::run(&directory));
        if let Err(error) = result {
            eprintln!("functional test worker panicked: {error:?}");
        }
    });
    Ok(())
}
