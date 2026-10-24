//! Virtual fixture clock and host-local input, using the production Assembly.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    error::Error,
    path::{Path, PathBuf},
};
#[derive(Debug)]
enum Op {
    Tick(f64),
    Set(Value),
    Input(String, Value, usize),
    Capture(String, Option<f64>),
    Invoke(Value),
}
fn advance(ops: &mut VecDeque<Op>, time: &mut f64, target: f64, tick: f64) {
    while *time + tick < target - 1e-8 {
        *time += tick;
        ops.push_back(Op::Tick(*time));
    }
    *time = target;
    ops.push_back(Op::Tick(target));
}
fn operations(f: &Value) -> Result<VecDeque<Op>, Box<dyn Error>> {
    let tick = f["tick_ms"].as_f64().unwrap_or(1000. / 60.);
    if !tick.is_finite() || tick <= 0. {
        return Err("invalid fixture tick_ms".into());
    }
    let mut ops = VecDeque::new();
    let mut time = 0.;
    ops.push_back(Op::Set(f["props"].clone()));
    advance(
        &mut ops,
        &mut time,
        f["settle_ms"].as_f64().unwrap_or(1500.) + tick,
        tick,
    );
    for (index, step) in f["steps"].as_array().into_iter().flatten().enumerate() {
        if let Some(set) = step.get("set") {
            ops.push_back(Op::Set(set.clone()));
        }
        if let Some(invoke) = step.get("invoke") {
            ops.push_back(Op::Invoke(invoke.clone()));
        }
        for key in [
            "hover",
            "press",
            "click",
            "rightclick",
            "drag",
            "wheel",
            "release",
        ] {
            if let Some(value) = step.get(key) {
                if key == "drag" {
                    ops.push_back(Op::Input("drag_begin".into(), value.clone(), index));
                    for i in 1..=12 {
                        ops.push_back(Op::Input("drag_move".into(), json!(i as f64 / 12.), index));
                        time += tick;
                        ops.push_back(Op::Tick(time));
                    }
                } else {
                    ops.push_back(Op::Input(key.into(), value.clone(), index));
                }
            }
        }
        if let Some(ms) = step["advance"].as_f64() {
            if !ms.is_finite() || ms < 0. {
                return Err("invalid fixture advance".into());
            }
            let target = time + ms;
            advance(&mut ops, &mut time, target, tick);
        }
        if let Some(name) = step["capture"].as_str() {
            ops.push_back(Op::Capture(name.into(), None));
        }
        if let Some(frames) = step.get("frames") {
            let base = time;
            let name = frames["name"].as_str().unwrap_or("frame");
            for at in frames["at"].as_array().into_iter().flatten() {
                let offset = at.as_f64().ok_or("invalid frame offset")?;
                if base + offset < time {
                    return Err("frame offsets must increase".into());
                }
                advance(&mut ops, &mut time, base + offset, tick);
                ops.push_back(Op::Capture(format!("{name}-t{:04}", offset as u64), Some(offset)));
            }
        }
        for key in step.as_object().into_iter().flat_map(|o| o.keys()) {
            if ![
                "set",
                "invoke",
                "hover",
                "press",
                "click",
                "rightclick",
                "drag",
                "wheel",
                "release",
                "advance",
                "capture",
                "frames",
                "dump",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unsupported fixture step: {key}").into());
            }
        }
    }
    if !ops.iter().any(|o| matches!(o, Op::Capture(_, _))) {
        ops.push_back(Op::Capture(String::new(), None));
    }
    Ok(ops)
}
fn ipc(a: &mut Assembly, call: &Value) -> Result<(), Box<dyn Error>> {
    let args = call.as_array().ok_or("ipc must be an array")?;
    if args.len() < 2 {
        return Err("ipc needs target and function".into());
    }
    let response = a.dispatch(Request {
        target: args[0].as_str().ok_or("ipc target")?.into(),
        function: args[1].as_str().ok_or("ipc function")?.into(),
        args: args[2..]
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .collect(),
    });
    if !response.ok {
        return Err(response.result.into());
    }
    Ok(())
}
fn apply(a: &Rc<RefCell<Assembly>>, patch: &Value) -> Result<(), Box<dyn Error>> {
    if patch.is_null() {
        return Ok(());
    }
    let mut a = a.borrow_mut();
    a.fixture.as_mut().unwrap().apply(patch);
    if let Some(call) = patch.get("ipc") {
        ipc(&mut a, call)?;
    }
    a.dirty = true;
    Ok(())
}
fn text_targets(
    node: &hyprv_ui::scene::Node,
    parent: &hyprv_ui::skia::Matrix,
    target: &Value,
    out: &mut Vec<(f32, f32)>,
) {
    use hyprv_ui::scene::Content;
    if !node.visible || node.opacity <= 0. {
        return;
    }
    let mut matrix = *parent;
    matrix.pre_translate((node.frame.x, node.frame.y));
    matrix.pre_concat(&node.transform);
    if let Content::Text(t) = &node.content {
        if target["text"].as_str().is_some_and(|v| t.text == v)
            || target["contains"]
                .as_str()
                .is_some_and(|v| t.text.contains(v))
        {
            let p = matrix.map_point((node.frame.width / 2., node.frame.height / 2.));
            out.push((p.x, p.y));
        }
    }
    if let Content::Shared(n) = &node.content {
        text_targets(n, &matrix, target, out);
    }
    for child in &node.children {
        text_targets(child, &matrix, target, out);
    }
}
fn target(
    a: &Rc<RefCell<Assembly>>,
    engine: &mut Engine,
    target: &Value,
    index: usize,
    action: &str,
) -> Result<(f32, f32), Box<dyn Error>> {
    if let Some(p) = target.as_array().filter(|p| p.len() == 2) {
        return Ok((
            p[0].as_f64().ok_or("target x")? as f32,
            p[1].as_f64().ok_or("target y")? as f32,
        ));
    }
    let explicit =
        a.borrow().fixture.as_ref().unwrap().json["targets"][format!("{index}/{action}")].clone();
    if !explicit.is_null() {
        return self::target(a, engine, &explicit, index, action);
    }
    let path = PathBuf::from(std::env::var_os("HYPRV_NESTED_FIXTURE").unwrap());
    let stem = path.file_stem().unwrap().to_string_lossy();
    let legacy_targets = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hyprv-ui-panels/tests/shared_common/targets.json");
    if let Ok(bytes) = std::fs::read(legacy_targets) {
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(coords) = v[stem.as_ref()][format!("{index}/{action}")].as_array() {
                if coords.len() == 2 {
                    return Ok((
                        coords[0].as_f64().unwrap() as f32,
                        coords[1].as_f64().unwrap() as f32,
                    ));
                }
            }
        }
    }
    {
        let a = a.borrow();
        let name = a.primary.as_ref().ok_or("no output")?;
        let model = &a.owners.bars[name].model;
        let layout = bar::layout::compute(model, &model.theme);
        if target["item"] == "island" {
            let g = a.owners.islands[name].borrow().geometry();
            let half = hyprv_widgets::foundations::hcenter;
            let x = layout.center.x + half(layout.center.width) - half(g.width);
            return Ok((
                (x + target["x"].as_f64().unwrap_or(0.) as f32 + 0.5).floor(),
                (layout.center.y
                    + layout.content_y
                    + target["y"].as_f64().unwrap_or(0.) as f32
                    + 0.5)
                    .floor(),
            ));
        }
        if let Some(image) = target["image"].as_str() {
            let target_kind = if image.contains("control-panel-") {
                Some(bar::model::Target::ControlPanel)
            } else if image.contains("nm-signal-") {
                Some(bar::model::Target::Wifi)
            } else {
                model
                    .tray
                    .items
                    .iter()
                    .find(|i| i.icon.contains(image))
                    .map(|i| bar::model::Target::Tray(i.key.clone()))
            };
            if let Some(rect) = target_kind.and_then(|t| layout.target(&t)) {
                return Ok((
                    (rect.x + rect.width / 2. + target["x"].as_f64().unwrap_or(0.) as f32 + 0.5)
                        .floor(),
                    (rect.y + rect.height / 2. + target["y"].as_f64().unwrap_or(0.) as f32 + 0.5)
                        .floor(),
                ));
            }
        }
    }
    let mut matches = Vec::new();
    let output = {
        let a = a.borrow();
        let name = a.primary.as_ref().ok_or("no output")?;
        a.outputs[name].1.clone()
    };
    for (id, config) in engine.surfaces() {
        if config.input_only {
            continue;
        }
        let Some((size, _)) = engine.surface_geometry(id) else {
            continue;
        };
        let origin = crate::fullscreen::origin(
            &config,
            (size.0 as f32, size.1 as f32),
            (output.width, output.height),
        );
        if let Some(scene) = engine.scene(id) {
            let before = matches.len();
            text_targets(
                scene.root(),
                &hyprv_ui::skia::Matrix::new_identity(),
                target,
                &mut matches,
            );
            for p in &mut matches[before..] {
                p.0 += origin.0;
                p.1 += origin.1;
            }
        }
    }
    matches.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    let p=matches.get(target["index"].as_u64().unwrap_or(0) as usize).copied().ok_or_else(||format!("unresolved fixture target {index}/{action}: {target}; supply Qt-resolved targets coordinates for baked labels"))?;
    Ok((
        (p.0 + target["x"].as_f64().unwrap_or(0.) as f32 + 0.5).floor(),
        (p.1 + target["y"].as_f64().unwrap_or(0.) as f32 + 0.5).floor(),
    ))
}
fn pointer(
    a: &Rc<RefCell<Assembly>>,
    engine: &mut Engine,
    position: (f32, f32),
    kind: PointerKind,
    grab: &mut Option<SurfaceId>,
    focus: &mut Option<SurfaceId>,
    motion: bool,
) -> Result<(), Box<dyn Error>> {
    let (output, masks) = {
        let a = a.borrow();
        let name = a.primary.as_ref().ok_or("no output")?;
        (
            a.outputs[name].1.clone(),
            a.mounts
                .iter()
                .map(|(id, m)| (*id, m.mask.clone()))
                .collect::<HashMap<_, _>>(),
        )
    };
    let mut surfaces = engine.surfaces();
    surfaces.sort_by_key(|(_, c)| crate::fullscreen::layer(c.layer));
    for (id, config) in surfaces.into_iter().rev() {
        if grab.is_some_and(|g| g != id) {
            continue;
        }
        let Some((size, _)) = engine.surface_geometry(id) else {
            continue;
        };
        let origin = crate::fullscreen::origin(
            &config,
            (size.0 as f32, size.1 as f32),
            (output.width, output.height),
        );
        let p = (position.0 - origin.0, position.1 - origin.1);
        if grab.is_none()
            && masks
                .get(&id)
                .is_none_or(|m| !m.iter().any(|r| r.contains(p.0, p.1)))
        {
            continue;
        }
        if !motion && *focus == Some(id) { return Ok(()); }
        let surface = engine.surface_proxy(id).ok_or("missing native surface")?;
        if *focus != Some(id) {
            if let Some(previous) = focus.take() {
                if let Some(old) = engine.surface_proxy(previous) {
                    engine.replay_input(
                        previous,
                        InputEvent::Pointer {
                            event: hyprv_ui::wayland::NativePointerEvent {
                                surface: old,
                                position: (0., 0.),
                                kind: PointerKind::Leave { serial: 0 },
                            },
                            target: None,
                        },
                    );
                }
            }
            engine.replay_input(
                id,
                InputEvent::Pointer {
                    event: hyprv_ui::wayland::NativePointerEvent {
                        surface: surface.clone(),
                        position: (p.0 as f64, p.1 as f64),
                        kind: PointerKind::Enter { serial: 0 },
                    },
                    target: None,
                },
            );
            *focus = Some(id);
        }
        if !motion { return Ok(()); }
        if matches!(kind, PointerKind::Press { .. }) {
            *grab = Some(id);
        } else if matches!(kind, PointerKind::Release { .. }) {
            *grab = None;
        }
        engine.replay_input(
            id,
            InputEvent::Pointer {
                event: hyprv_ui::wayland::NativePointerEvent {
                    surface,
                    position: (p.0 as f64, p.1 as f64),
                    kind,
                },
                target: None,
            },
        );
        return Ok(());
    }
    Err(format!("no input surface at {position:?}").into())
}
fn capture(
    a: &Rc<RefCell<Assembly>>,
    engine: &mut Engine,
    directory: &Path,
    frame: &str,
    frame_ms: Option<f64>,
) -> Result<Value, Box<dyn Error>> {
    engine.request_redraw();
    engine.render_now();
    engine.render_now();
    let (output, time, theme, scale, stem) = {
        let a = a.borrow();
        let f = a.fixture.as_ref().unwrap();
        let output = &a.outputs[a.primary.as_ref().unwrap()].1;
        if std::env::var_os("HYPRV_REPLAY_TRACE").is_some() {
            let panel = a.owners.panels[a.primary.as_ref().unwrap()].borrow();
            eprintln!("replay-capture {frame}: raw={} clock={} host={:?} control_full={} control={:?}",
                f.time, f.clock, a.now, panel.control.m.reveal.full_height, panel.control.m.reveal.values());
        }
        (
            output.clone(),
            f.time,
            if f.dark { "dark" } else { "light" },
            f.scale as f64,
            PathBuf::from(std::env::var_os("HYPRV_NESTED_FIXTURE").unwrap())
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        )
    };
    let expected_width = std::env::var("HYPRV_NESTED_WIDTH")?.parse::<f32>()?;
    let expected_height = std::env::var("HYPRV_NESTED_HEIGHT")?.parse::<f32>()?;
    if output.width != expected_width || output.height != expected_height {
        return Err(format!(
            "actual output {}x{} differs from requested {}x{}",
            output.width, output.height, expected_width, expected_height
        )
        .into());
    }
    let name = if frame.is_empty() {
        stem.clone()
    } else {
        format!("{stem}--{frame}")
    };
    let part = directory.join(format!("{name}--{theme}@{scale}"));
    std::fs::create_dir_all(&part)?;
    let captures = engine.dump_surfaces(&part)?;
    if captures.len() < 2 || captures.len() != engine.surface_count() {
        return Err("not all assembled surfaces rendered".into());
    }
    // References are compositor black/white mattes, not raw buffer SourceOver.
    // Include Hyprland's real texture sampling and blend arithmetic identically.
    let composite=crate::compositor_capture::capture(&output.name,
        ((output.width as f64*scale).round()as usize,(output.height as f64*scale).round()as usize))?;
    let file = format!("{name}--{theme}@{scale}.png");
    composite.save(&directory.join(&file))?;
    let outname = std::env::var("HYPRV_NESTED_OUTPUT").unwrap_or(output.name);
    let reference_dir = PathBuf::from(std::env::var_os("HYPRV_NESTED_REFERENCES").unwrap_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../parity/reference/fullscreen")
                .into_os_string()
        },
    ));
    let entry =
        crate::fullscreen::reference_entry(&reference_dir, &stem, frame, theme, scale, &outname)?;
    let reference_time = frame_ms.unwrap_or(time);
    let comparison = if let Some(entry) = entry {
        let path = reference_dir.join(entry["file"].as_str().ok_or("reference has no file")?);
        if entry["virtual_ms"]
            .as_f64()
            .is_none_or(|t| (t - reference_time).abs() > 0.01)
        {
            json!({"status":"reference_time_mismatch","pass":false,"actual_virtual_ms":reference_time,"reference_virtual_ms":entry["virtual_ms"]})
        } else if !path.exists() {
            json!({"status":"missing_reference","pass":false,"reference":path})
        } else {
            let reference = crate::fullscreen::Image::load(&path)?;
            let mut result = crate::fullscreen::compare(
                &composite,
                &reference,
                entry["distance_field_text"]
                    .as_bool()
                    .ok_or("reference lacks distance_field_text")?,
                scale,
            );
            result["reference"] = json!(path);
            result
        }
    } else {
        json!({"status":"missing_reference","pass":false,"reference_manifest":reference_dir.join("manifest.json")})
    };
    let surfaces:Vec<_>=captures.into_iter().map(|c|json!({"file":c.file,"output":c.output,"namespace":c.config.namespace,"logical_size":[c.size.0,c.size.1],"scale":c.scale,"margins":[c.config.margins.0,c.config.margins.1,c.config.margins.2,c.config.margins.3],"anchors":c.config.anchor.bits(),"layer":crate::fullscreen::layer(c.config.layer),"input_only":c.config.input_only,"source":"native EGL/GL RGBA8 target","direct_window":c.config.full_redraw})).collect();
    std::fs::write(
        part.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({"surfaces":surfaces}))?,
    )?;
    Ok(
        json!({"file":file,"fixture":stem,"frame":frame,"theme":theme,"scale":scale,"output":outname,"logical_size":[output.width,output.height],"pixel_size":[composite.width,composite.height],"renderer":engine.renderer_name(),"format":"RGBA8","virtual_ms":time,"reference_clock_ms":reference_time,"comparison":comparison}),
    )
}
pub(super) fn install(
    app: &Application,
    state: Rc<RefCell<Assembly>>,
    directory: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let fixture = state.borrow().fixture.as_ref().unwrap().json.clone();
    let mut ops = operations(&fixture)?;
    let mut results = Vec::new();
    let mut mouse = (0., 0.);
    let mut capture_wait = None;
    let mut grab = None;
    let mut focus = None;
    let mut drag = None;
    // Bounds (real time): no op may wait longer than STALL for unconfigured surfaces, and
    // the whole replay gets a budget proportional to its length (long settles, frame
    // sequences and three compositor mattes per capture need a larger wall budget).
    const STALL: Duration = Duration::from_secs(10);
    let budget = Duration::from_secs(120) + Duration::from_millis(40) * ops.len() as u32;
    let trace = std::env::var_os("HYPRV_REPLAY_TRACE").is_some();
    let mut progress = Duration::ZERO;
    let mut done_ops = 0usize;
    app.event_loop_handle().insert_source(Timer::from_duration(Duration::from_millis(100)),move|_,_,engine|{
  let result=(||->Result<bool,Box<dyn Error>>{
   let now=engine.now();
   if now>budget{return Err(format!("fixture replay exceeded its {:.0}-second budget after {done_ops} ops; next op {:?}",budget.as_secs_f64(),ops.front()).into());}
   let unconfigured:Vec<String>=engine.surfaces().iter().filter(|(id,_)|engine.surface_geometry(*id).is_none_or(|(size,_)|size.0==0||size.1==0)).map(|(id,_)|format!("{id:?}")).collect();
   if engine.surface_count()<2||!unconfigured.is_empty(){
     if now.saturating_sub(progress)>STALL{return Err(format!("fixture replay stalled {:.0} s before op {done_ops} {:?}: {} surface(s), unconfigured {:?}",STALL.as_secs_f64(),ops.front(),engine.surface_count(),unconfigured).into());}
     return Ok(false);
   }
   let Some(op)=ops.pop_front()else{return Ok(true);};
   progress=now;
   done_ops+=1;
   if trace{eprintln!("replay: {:>8.1} ms op {done_ops} {op:?} surfaces {}",now.as_secs_f64()*1000.,engine.surface_count());}
   match op {
    Op::Tick(ms)=>{let mut a=state.borrow_mut();a.fixture.as_mut().unwrap().advance_to(ms);a.dirty=true;drop(a);engine.request_redraw();}
    Op::Set(patch)=>{apply(&state,&patch)?;engine.request_redraw();}
    Op::Invoke(value)=>{
      if value["type"]=="ControlPanelPopup"&&value["method"]=="toggleMihomoExpanded" {let a=state.borrow();let name=a.primary.as_ref().unwrap();a.owners.panels[name].borrow_mut().control.show_page(panels::control_panel::Page::Mihomo,a.now);}else{return Err(format!("unsupported invoke: {value}").into());}
      engine.request_redraw();
    },
    Op::Input(action,value,index)=>{
      let time=state.borrow().now.as_millis() as u32;
      if action=="drag_begin" {if grab.is_none(){return Err("drag without held press".into());}drag=Some((mouse,target(&state,engine,&value,index,"drag")?));return Ok(false);}
      if action=="drag_move" {let (from,to)=drag.ok_or("drag has no start")?;let k=value.as_f64().unwrap() as f32;mouse=(from.0+(to.0-from.0)*k,from.1+(to.1-from.1)*k);pointer(&state,engine,mouse,PointerKind::Motion{time},&mut grab,&mut focus,true)?;return Ok(false);}
      if action!="release"||!value.is_boolean() {mouse=target(&state,engine,if action=="wheel"&&!value["at"].is_null(){&value["at"]}else{&value},index,&action)?;pointer(&state,engine,mouse,PointerKind::Motion{time},&mut grab,&mut focus,true)?;}
      match action.as_str(){
       "hover"=>(),
       "press"=>pointer(&state,engine,mouse,PointerKind::Press{time,button:0x110,serial:0},&mut grab,&mut focus,true)?,
       "release"=>pointer(&state,engine,mouse,PointerKind::Release{time,button:0x110,serial:0},&mut grab,&mut focus,true)?,
       "click"|"rightclick"=>{let button=if action=="rightclick"{0x111}else{0x110};pointer(&state,engine,mouse,PointerKind::Press{time,button,serial:0},&mut grab,&mut focus,true)?;pointer(&state,engine,mouse,PointerKind::Release{time,button,serial:0},&mut grab,&mut focus,true)?;},
       "wheel"=>pointer(&state,engine,mouse,PointerKind::Axis{time,horizontal:Default::default(),vertical:hyprv_ui::wayland::AxisScroll{absolute:0.,discrete:(value["dy"].as_i64().unwrap_or(-120)/120) as i32,stop:false},source:None},&mut grab,&mut focus,true)?,
       _=>return Err(format!("unsupported input: {action}; drag requires a Qt-resolved trace").into()),
      }
    }
    Op::Capture(name,frame_ms)=>{let now=engine.now();
      if capture_wait.is_none(){capture_wait=Some(now);ops.push_front(Op::Capture(name,frame_ms));return Ok(false);}

      if now-capture_wait.unwrap()<Duration::from_millis(40){ops.push_front(Op::Capture(name,frame_ms));return Ok(false);}
      // Native compositors recompute pointer focus when a surface maps beneath
      // a stationary pointer. Replay has no compositor pointer, so do it here too.
      // Keep explicit grabs until release; an untouched pointer has no focus.
      if focus.is_some() && grab.is_none() {
        let time=state.borrow().now.as_millis() as u32;
        pointer(&state,engine,mouse,PointerKind::Motion{time},&mut grab,&mut focus,false)?;
      }
      results.push(capture(&state,engine,&directory,&name,frame_ms)?);capture_wait=None;
    }
   }Ok(false)
  })();
  match result {
   Ok(false)=>TimeoutAction::ToDuration(Duration::from_millis(1)),
   done=>{let error=done.err().map(|e|e.to_string());let pass=error.is_none()&&results.iter().all(|r|r["comparison"]["pass"]==true);let manifest=json!({"pass":pass,"error":error,"captures":results,"source":"production Assembly, native EGL/GL surfaces; private Hyprland black/white compositor matte","renderer":engine.renderer_name(),"virtual_clock":true});let _=std::fs::write(directory.join("manifest.json"),serde_json::to_vec_pretty(&manifest).unwrap());engine.quit();TimeoutAction::Drop}
  }
 })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_timestamps_preserve_frame_offsets() {
        let f = json!({"settle_ms":100,"tick_ms":16.667,"steps":[{"advance":10,"frames":{"name":"open","at":[0,25,100]}}]});
        let o = operations(&f).unwrap();
        let samples: Vec<_> = o
            .iter()
            .filter_map(|o| {
                if let Op::Capture(n, _) = o {
                    Some(n.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(samples, ["open-t0000", "open-t0025", "open-t0100"]);
        let offsets:Vec<_>=o.iter().filter_map(|op| match op { Op::Capture(_, t)=>*t, _=>None }).collect();
        assert_eq!(offsets, [0.,25.,100.]);
        let ordinary=operations(&json!({"settle_ms":100,"steps":[{"capture":"still"}]})).unwrap();
        assert!(matches!(ordinary.back(),Some(Op::Capture(n,None)) if n=="still"));
        assert!(matches!(o.iter().rev().nth(1),Some(Op::Tick(t))if (*t-(210.+16.667)).abs()<1e-8));
    }
}
