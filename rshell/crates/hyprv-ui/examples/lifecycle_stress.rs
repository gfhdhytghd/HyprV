#[cfg(debug_assertions)]
mod diagnostic {
    // Explicit private-session lifecycle search, with an independent CPU render for every frame.
    // Debug build only: optional EGL self-checks and simulated reset are not release runtime APIs.
    use hyprv_ui::{
        animation::Color,
        egl::{EglWindow, SharedEgl},
        geometry::Rect,
        render::Renderer,
        scene::{Content, Fill, NativeTextMode, Node, Rectangle, RenderType, Scene, Text},
        skia as sk,
    };
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    use wayland_client::{
        globals::{registry_queue_init, GlobalListContents},
        protocol::{
            wl_callback::{self, WlCallback},
            wl_compositor::WlCompositor,
            wl_output::{self, WlOutput},
            wl_registry::WlRegistry,
            wl_surface::WlSurface,
        },
        Connection, Dispatch, Proxy, QueueHandle,
    };
    use wayland_protocols::xdg::shell::client::{
        xdg_popup::XdgPopup, xdg_positioner::XdgPositioner, xdg_surface::XdgSurface,
        xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
    };
    #[derive(Default)]
    struct State {
        callbacks: [usize; 4],
        configures: usize,
        output_events: usize,
        modes: Vec<serde_json::Value>,
        callback_times: Vec<(usize, u32)>,
    }
    macro_rules! noop {
        ($ty:ty,$data:ty) => {
            impl Dispatch<$ty, $data> for State {
                fn event(
                    _: &mut Self,
                    _: &$ty,
                    _: <$ty as Proxy>::Event,
                    _: &$data,
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }
        };
    }
    noop!(WlRegistry, GlobalListContents);
    noop!(WlCompositor, ());
    noop!(WlSurface, ());
    noop!(XdgToplevel, ());
    noop!(XdgPopup, ());
    noop!(XdgPositioner, ());
    impl Dispatch<WlOutput, ()> for State {
        fn event(
            s: &mut Self,
            output: &WlOutput,
            event: wl_output::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            s.output_events += 1;
            if let wl_output::Event::Mode {
                width,
                height,
                refresh,
                ..
            } = event
            {
                s.modes.push(serde_json::json!({"object":output.id().protocol_id(),"width":width,"height":height,"refresh_mHz":refresh}));
            }
        }
    }
    impl Dispatch<WlCallback, usize> for State {
        fn event(
            s: &mut Self,
            _: &WlCallback,
            e: wl_callback::Event,
            i: &usize,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_callback::Event::Done { callback_data } = e {
                s.callback_times.push((*i, callback_data));
                s.callbacks[*i] += 1;
            }
        }
    }
    impl Dispatch<XdgWmBase, ()> for State {
        fn event(
            _: &mut Self,
            p: &XdgWmBase,
            e: <XdgWmBase as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wayland_protocols::xdg::shell::client::xdg_wm_base::Event::Ping { serial } = e {
                p.pong(serial);
            }
        }
    }
    impl Dispatch<XdgSurface, ()> for State {
        fn event(
            s: &mut Self,
            p: &XdgSurface,
            e: <XdgSurface as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wayland_protocols::xdg::shell::client::xdg_surface::Event::Configure { serial } =
                e
            {
                p.ack_configure(serial);
                s.configures += 1;
            }
        }
    }
    struct Window {
        egl: Option<EglWindow>,
        surface: WlSurface,
        xdg: XdgSurface,
        top: Option<XdgToplevel>,
        popup: Option<XdgPopup>,
    }
    impl Drop for Window {
        fn drop(&mut self) {
            drop(self.egl.take());
            if let Some(p) = &self.popup {
                p.destroy();
            }
            if let Some(t) = &self.top {
                t.destroy();
            }
            self.xdg.destroy();
            self.surface.destroy();
        }
    }
    fn make_window(
        compositor: &WlCompositor,
        wm: &XdgWmBase,
        qh: &QueueHandle<State>,
        parent: Option<&XdgSurface>,
    ) -> Window {
        let surface = compositor.create_surface(qh, ());
        let xdg = wm.get_xdg_surface(&surface, qh, ());
        let (top, popup) = if let Some(parent) = parent {
            let p = wm.create_positioner(qh, ());
            p.set_size(192, 128);
            p.set_anchor_rect(10, 10, 32, 32);
            let popup = xdg.get_popup(Some(parent), &p, qh, ());
            p.destroy();
            (None, Some(popup))
        } else {
            let top = xdg.get_toplevel(qh, ());
            top.set_title("PRIVATE lifecycle audit".into());
            (Some(top), None)
        };
        surface.commit();
        Window {
            egl: None,
            surface,
            xdg,
            top,
            popup,
        }
    }
    fn hash(b: &[u8]) -> u64 {
        b.iter().fold(0xcbf29ce484222325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
    }
    fn pixels(image: &sk::Image) -> Vec<u8> {
        let mut b = vec![0; image.width() as usize * image.height() as usize * 4];
        assert!(image.read_pixels(
            &sk::ImageInfo::new(
                image.dimensions(),
                sk::ColorType::RGBA8888,
                sk::AlphaType::Premul,
                None
            ),
            &mut b,
            image.width() as usize * 4,
            (0, 0),
            sk::image::CachingHint::Disallow
        ));
        b
    }
    fn scene(k: usize) -> (Scene, (f32, f32), f64) {
        let patch = (k >= 48).then_some(k % 2);
        let k = if k >= 48 { (k - 48) / 2 } else { k };
        let size = match k % 3 {
            0 => (256., 160.),
            1 => (640., 64.),
            _ => (320., 240.),
        };
        let scale = [1., 1.25, 1.5, 2.][(k / 3) % 4];
        let mut root = Node::new(Rect::new(0., 0., size.0, size.1), Content::Group);
        root.native_materials = Some(true);
        for j in 0..8 {
            let exact = std::env::var_os("HYPRV_LIFECYCLE_RICH").is_none();
            let x = (((j * 31 + k * 7) % 180) / 4 * 4) as f32;
            let y = (((j * 13 + k * 3) % (size.1 as usize - 38)) / 4 * 4) as f32;
            root.add(Node::new(
                Rect::new(x, y, 64., 28.),
                Content::Rect(Rectangle {
                    fill: Fill::Solid(Color::rgba((j % 3) as f32 / 2., 0.5, 0.25, 1.)),
                    radii: [if exact { 0. } else { 7. }; 4],
                    border: None,
                }),
            ));
            let mut n = Node::new(
                Rect::new(x + 2., y + 2., 100., 20.),
                Content::Text(Text {
                    text: format!("{k:02}/{j} Aa中"),
                    families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
                    size: 12.,
                    color: Color::rgba(1., 1., 1., 1.),
                    render_type: RenderType::NativeRendering,
                    ..Default::default()
                }),
            );
            n.native_text_mode = NativeTextMode::QtGl;
            if !exact {
                root.add(n);
            }
            let mut rec = sk::PictureRecorder::new();
            let c = rec.begin_recording(sk::Rect::from_xywh(0., 0., 20., 12.), false);
            let mut p = sk::Paint::default();
            p.set_color(sk::Color::from_rgb(32, 64, 128));
            c.draw_rect(sk::Rect::from_xywh(0., 0., 20., 12.), &p);
            let picture = rec.finish_recording_as_picture(None).unwrap();
            root.add(Node::new(
                Rect::new(x + 32., y + 4., 20., 12.),
                Content::Picture(picture),
            ));
        }
        if let Some(patch) = patch {
            root.add(Node::new(
                Rect::new(12., 8., 32., 16.),
                Content::Rect(Rectangle {
                    fill: Fill::Solid(if patch == 0 {
                        Color::rgba(1., 0., 0., 1.)
                    } else {
                        Color::rgba(0., 1., 0., 1.)
                    }),
                    radii: [0.; 4],
                    border: None,
                }),
            ));
        }
        (Scene::new(root), size, scale)
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() != Ok("1") {
            return Err("requires explicitly private launcher".into());
        }
        let out = PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);
        std::fs::create_dir_all(&out)?;
        let frames: usize = std::env::var("HYPRV_LIFECYCLE_FRAMES")
            .unwrap_or("2400".into())
            .parse()?;
        let seed: usize = std::env::var("HYPRV_LIFECYCLE_SEED")
            .unwrap_or("0".into())
            .parse()?;
        let checked = std::env::var_os("HYPRV_LIFECYCLE_NO_READBACK").is_none();
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
        let qh = queue.handle();
        let mut state = State::default();
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ())?;
        let wm: XdgWmBase = globals.bind(&qh, 1..=1, ())?;
        let mut outputs: Vec<(u32, WlOutput)> = globals
            .contents()
            .clone_list()
            .iter()
            .filter(|g| g.interface == "wl_output")
            .map(|g| {
                (
                    g.name,
                    globals.registry().bind(g.name, g.version.min(4), &qh, ()),
                )
            })
            .collect();
        let mut shared = SharedEgl::new(&connection)?;
        let driver = shared.renderer_name();
        let extensions = shared.debug_gl_capabilities()?;
        let cases: Vec<_> = (0..56).map(scene).collect();
        let mut cpu = Renderer::new();
        cpu.set_offscreen_backend(hyprv_ui::offscreen::Backend::Cpu);
        let expected: Vec<_> = cases
            .iter()
            .map(|(s, size, scale)| {
                let mut raster = cpu.raster(s, size.0, size.1, *scale).unwrap();
                pixels(&raster.image_snapshot())
            })
            .collect();
        let mut windows: Vec<_> = (0..4)
            .map(|_| make_window(&compositor, &wm, &qh, None))
            .collect();
        queue.roundtrip(&mut state)?;
        let rich = std::env::var_os("HYPRV_LIFECYCLE_RICH").is_some();
        let mut controls = Vec::new();
        let mut calibration = Vec::new();
        for (k, (s, size, scale)) in cases.iter().enumerate() {
            if rich {
                let mut fresh = EglWindow::new(shared.clone(), &windows[0].surface, (16, 16))?;
                fresh.draw(
                    &mut Renderer::new(),
                    s,
                    *size,
                    *scale,
                    Rect::new(0., 0., size.0, size.1),
                )?;
                let b = pixels(&fresh.snapshot()?);
                let diff = b
                    .iter()
                    .zip(&expected[k])
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                calibration.push(serde_json::json!({"case":k,"cpu_max_delta":diff,"cpu_checksum":format!("{:016x}",hash(&expected[k])),"fresh_gpu_checksum":format!("{:016x}",hash(&b))}));
                controls.push(b);
            } else {
                controls.push(expected[k].clone());
            }
        }
        std::fs::write(
            out.join("oracle.json"),
            serde_json::to_vec_pretty(&calibration)?,
        )?;
        let mut renderers: Vec<_> = (0..5).map(|_| Renderer::new()).collect();
        let mut counts = serde_json::json!({"submitted":0,"checked":0,"cpu_exact":0,"surface_recreates":0,"popup_cycles":0,"context_recreates":0,"simulated_context_losses":0,"state_poisons":0,"cache_purges":0,"memory_pressure_cycles":0,"callback_pauses":0,"partial_damage_frames":0});
        let mut records = Vec::new();
        let mut mismatches = Vec::new();
        let mut max = 0;
        let start = Instant::now();
        let mut last = [None; 4];
        let mut max_outputs = outputs.len();
        let mut output_history = vec![];
        for frame in 0..frames {
            // Distinct presentation schedules: slot0 every tick, slot1 half-rate, slot2 third-rate, slot3 fifth-rate.
            let slot = if frame % 5 == 0 {
                3
            } else if frame % 3 == 0 {
                2
            } else if frame % 2 == 0 {
                1
            } else {
                0
            };
            let phase = (frame / (frames / 3).max(1)).min(2);
            let k = match phase {
                0 => (frame * 7 + seed * 11) % 48,
                1 => slot + 12 * ((frame / 4) % 4),
                _ => 48 + slot * 2 + (frame / 4) % 2,
            };
            let (s, size, scale) = &cases[k];
            if frame > 0 && frame % 97 == 0 {
                shared.debug_simulate_context_loss()?;
                let err = windows[slot]
                    .egl
                    .as_mut()
                    .unwrap()
                    .draw(
                        &mut renderers[4],
                        s,
                        *size,
                        *scale,
                        Rect::new(0., 0., size.0, size.1),
                    )
                    .unwrap_err();
                if !matches!(
                    err,
                    hyprv_ui::egl::EglError::Egl(khronos_egl::Error::ContextLost)
                ) {
                    return Err(format!("wrong reset error: {err}").into());
                }
                counts["simulated_context_losses"] =
                    serde_json::json!(counts["simulated_context_losses"].as_u64().unwrap() + 1);
                for w in &mut windows {
                    drop(w.egl.take());
                }
                drop(shared);
                shared = SharedEgl::new(&connection)?;
                renderers = (0..5).map(|_| Renderer::new()).collect();
                last = [None; 4];
                counts["context_recreates"] =
                    serde_json::json!(counts["context_recreates"].as_u64().unwrap() + 1);
            }
            if frame % 11 == 0 {
                windows[slot] = make_window(&compositor, &wm, &qh, None);
                queue.roundtrip(&mut state)?;
                last[slot] = None;
                counts["surface_recreates"] =
                    serde_json::json!(counts["surface_recreates"].as_u64().unwrap() + 1);
            }
            if frame % 13 == 0 {
                shared.debug_poison_gl_state()?;
                counts["state_poisons"] =
                    serde_json::json!(counts["state_poisons"].as_u64().unwrap() + 1);
            }
            if frame % 17 == 0 {
                for r in &mut renderers {
                    shared.clear_caches(r)?;
                }
                counts["cache_purges"] =
                    serde_json::json!(counts["cache_purges"].as_u64().unwrap() + 1);
            }
            if frame % 37 == 0 {
                assert_eq!(shared.debug_memory_pressure()?, 32 * 1024 * 1024);
                counts["memory_pressure_cycles"] =
                    serde_json::json!(counts["memory_pressure_cycles"].as_u64().unwrap() + 1);
            }
            if frame % 101 == 0 {
                std::thread::sleep(Duration::from_millis(35));
                counts["callback_pauses"] =
                    serde_json::json!(counts["callback_pauses"].as_u64().unwrap() + 1);
            }
            if windows[slot].egl.is_none() {
                windows[slot].egl = Some(EglWindow::new(
                    shared.clone(),
                    &windows[slot].surface,
                    (16, 16),
                )?);
            }
            let current_outputs: Vec<_> = globals
                .contents()
                .clone_list()
                .into_iter()
                .filter(|g| g.interface == "wl_output")
                .collect();
            outputs.retain(|(name, output)| {
                if current_outputs.iter().any(|g| g.name == *name) {
                    true
                } else {
                    output.release();
                    false
                }
            });
            for g in current_outputs {
                if !outputs.iter().any(|(name, _)| *name == g.name) {
                    outputs.push((
                        g.name,
                        globals.registry().bind(g.name, g.version.min(4), &qh, ()),
                    ));
                    output_history.push(serde_json::json!({"frame":frame,"added_global":g.name}));
                }
            }
            max_outputs = max_outputs.max(outputs.len());
            let w = &mut windows[slot];
            if let Some(top) = &w.top {
                if let Some((_, output)) = outputs.get(slot % outputs.len().max(1)) {
                    top.set_fullscreen(Some(output));
                }
            }

            w.surface.set_buffer_transform(match (frame / 23) % 4 {
                0 => wl_output::Transform::Normal,
                1 => wl_output::Transform::_90,
                2 => wl_output::Transform::_180,
                _ => wl_output::Transform::_270,
            });
            // Small declared content geometry fits every configured fullscreen mode;
            // full test buffers remain shadows/outsets, including rotated cases.
            w.xdg.set_window_geometry(0, 0, 32, 32);
            w.surface.frame(&qh, slot);
            let damage = if k >= 48 && last[slot].is_some_and(|old| old >= 48) {
                counts["partial_damage_frames"] =
                    serde_json::json!(counts["partial_damage_frames"].as_u64().unwrap() + 1);
                Rect::new(12., 8., 32., 16.)
            } else {
                Rect::new(0., 0., size.0, size.1)
            };
            let window = w.egl.as_mut().unwrap();
            window.draw(&mut renderers[slot], s, *size, *scale, damage)?;
            last[slot] = Some(k);
            counts["submitted"] = serde_json::json!(counts["submitted"].as_u64().unwrap() + 1);
            if checked {
                let actual = pixels(&window.snapshot()?);
                let oracle = &expected[k];
                let diff = actual
                    .iter()
                    .zip(oracle)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                max = max.max(diff);
                let record = serde_json::json!({"frame":frame,"slot":slot,"case":k,"scale":scale,"transform":(frame/23)%4,"source_checksum":format!("{:016x}",hash(&actual)),"cpu_checksum":format!("{:016x}",hash(oracle)),"max_delta":diff,"lifecycle_control_exact":actual==controls[k]});
                records.push(record.clone());
                counts["checked"] = serde_json::json!(counts["checked"].as_u64().unwrap() + 1);
                if diff == 0 {
                    counts["cpu_exact"] =
                        serde_json::json!(counts["cpu_exact"].as_u64().unwrap() + 1);
                }
                if actual != controls[k] {
                    if mismatches.len() < 12 {
                        window.png(&out.join(format!("mismatch-{frame}-{k}.png")))?;
                    }
                    mismatches.push(record);
                }
            }
            if frame % 7 == 0 {
                let mut popup = make_window(&compositor, &wm, &qh, Some(&windows[slot].xdg));
                queue.roundtrip(&mut state)?;
                let mut egl = EglWindow::new(shared.clone(), &popup.surface, (16, 16))?;
                egl.draw(
                    &mut renderers[4],
                    s,
                    *size,
                    *scale,
                    Rect::new(0., 0., size.0, size.1),
                )?;
                counts["submitted"] = serde_json::json!(counts["submitted"].as_u64().unwrap() + 1);
                if checked {
                    let actual = pixels(&egl.snapshot()?);
                    let diff = actual
                        .iter()
                        .zip(&expected[k])
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    let r = serde_json::json!({"frame":frame,"popup":true,"case":k,"max_delta":diff,"source_checksum":format!("{:016x}",hash(&actual)),"cpu_checksum":format!("{:016x}",hash(&expected[k])),"lifecycle_control_exact":actual==controls[k]});
                    records.push(r.clone());
                    counts["checked"] = serde_json::json!(counts["checked"].as_u64().unwrap() + 1);
                    if diff == 0 {
                        counts["cpu_exact"] =
                            serde_json::json!(counts["cpu_exact"].as_u64().unwrap() + 1);
                    }
                    if actual != controls[k] {
                        max = max.max(diff);
                        mismatches.push(r);
                    }
                }
                popup.egl = Some(egl);
                drop(popup);
                counts["popup_cycles"] =
                    serde_json::json!(counts["popup_cycles"].as_u64().unwrap() + 1);
            }
            if !checked && (frame % 127 == 126 || frame + 1 == frames) {
                for (slot, w) in windows.iter_mut().enumerate() {
                    if let (Some(window), Some(k)) = (&mut w.egl, last[slot]) {
                        let actual = pixels(&window.snapshot()?);
                        let diff = actual
                            .iter()
                            .zip(&expected[k])
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        let r = serde_json::json!({"frame":frame,"slot":slot,"delayed_readback":true,"case":k,"max_delta":diff,"lifecycle_control_exact":actual==controls[k],"source_checksum":format!("{:016x}",hash(&actual)),"cpu_checksum":format!("{:016x}",hash(&expected[k]))});
                        records.push(r.clone());
                        counts["checked"] =
                            serde_json::json!(counts["checked"].as_u64().unwrap() + 1);
                        if diff == 0 {
                            counts["cpu_exact"] =
                                serde_json::json!(counts["cpu_exact"].as_u64().unwrap() + 1);
                        }
                        if actual != controls[k] {
                            mismatches.push(r);
                        }
                    }
                }
            }
            // Deliberately withhold dispatch in short intervals, then drain all callbacks.
            if frame % 29 == 0 {
                queue.roundtrip(&mut state)?;
            }
            if let Ok(us) = std::env::var("HYPRV_LIFECYCLE_TICK_US") {
                std::thread::sleep(Duration::from_micros(us.parse()?));
            }
            if frame % 100 == 99 {
                std::fs::write(
                    out.join("frames-checkpoint.json"),
                    serde_json::to_vec(&records)?,
                )?;
                std::fs::write(
                    out.join("progress.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"frame":frame,"elapsed_ms":start.elapsed().as_millis()}),
                    )?,
                )?;
                eprintln!(
                    "lifecycle {}/{} checks={} mismatches={} max={}",
                    frame + 1,
                    frames,
                    counts["checked"],
                    mismatches.len(),
                    max
                );
            }
        }
        queue.roundtrip(&mut state)?;
        drop(windows);
        for r in &mut renderers {
            shared.clear_caches(r)?;
        }
        drop(shared);
        for (_, o) in outputs {
            o.release();
        }
        let result = serde_json::json!({"frames":frames,"counts":counts,"max_delta":max,"mismatches":mismatches,"driver":driver,"extensions":extensions,"callback_counts":state.callbacks,"configure_events":state.configures,"output_events":state.output_events,"output_mode_events":state.modes,"callback_times":state.callback_times,"max_simultaneous_outputs":max_outputs,"output_additions":output_history,"elapsed_ms":start.elapsed().as_millis(),"checked":checked,"profile":if rich{"rich: per-frame CPU comparison plus byte-exact fresh GPU lifecycle control; known static raster differences in oracle.json"}else{"exact: independent deterministic CPU oracle"},"oracle":"independent deterministic CPU raster per case; premultiplied RGBA8","context_reset":"simulated fail-closed notification plus destruction/recreation; no hardware reset"});
        std::fs::write(out.join("frames.json"), serde_json::to_vec(&records)?)?;
        std::fs::write(
            out.join("results.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        println!("{result}");
        if !mismatches.is_empty() {
            return Err("CPU oracle mismatches".into());
        }
        Ok(())
    }
}
#[cfg(debug_assertions)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    diagnostic::run()
}
#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("lifecycle_stress requires a debug build; no fault injection in release");
    std::process::exit(1);
}
