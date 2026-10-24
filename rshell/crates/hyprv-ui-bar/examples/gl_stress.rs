//! Opt-in private-Weston stress of the real shared EGL context and window targets.
use hyprv_ui::{
    animation::Color,
    egl::{EglWindow, SharedEgl},
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, Node, Rectangle, RenderType, Scene, Text},
    skia,
};
use std::path::PathBuf;
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_compositor::WlCompositor, wl_registry::WlRegistry, wl_surface::WlSurface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{
    xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
};
struct State;
impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: wayland_client::protocol::wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: wayland_client::protocol::wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        proxy: &XdgWmBase,
        event: wayland_protocols::xdg::shell::client::xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wayland_protocols::xdg::shell::client::xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}
impl Dispatch<XdgSurface, ()> for State {
    fn event(
        _: &mut Self,
        proxy: &XdgSurface,
        event: wayland_protocols::xdg::shell::client::xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wayland_protocols::xdg::shell::client::xdg_surface::Event::Configure { serial } =
            event
        {
            proxy.ack_configure(serial);
        }
    }
}
impl Dispatch<XdgToplevel, ()> for State {
    fn event(
        _: &mut Self,
        _: &XdgToplevel,
        _: wayland_protocols::xdg::shell::client::xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
fn hash(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
fn case(seed: usize) -> (Scene, (f32, f32), f64) {
    let i = if seed >= 24 { 0 } else { seed };
    let size = match i % 3 {
        0 => (320., 180.),
        1 => (3072., 70.),
        _ => (512., 520.),
    };
    let scale = if i.is_multiple_of(2) { 1. } else { 2. };
    let mut root = Node::new(Rect::new(0., 0., size.0, size.1), Content::Group);
    for j in 0..12 {
        let x = ((j * 97 + i * 13) % size.0 as usize) as f32;
        let y = ((j * 17 + i * 3) % size.1 as usize) as f32;
        root.add(Node::new(
            Rect::new(x, y, 120., 38.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba((i % 5) as f32 / 5., 0.2, 0.6, 0.8)),
                radii: [11.; 4],
                border: None,
            }),
        ));
        root.add(Node::new(
            Rect::new(x + 4., y + 2., 115., 28.),
            Content::Text(Text {
                text: format!("{i:02}/{j} Aa中"),
                families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
                size: 12. + (i % 5) as f32,
                weight: if i.is_multiple_of(3) { 700 } else { 400 },
                color: Color::rgba(137. / 255., 180. / 255., 250. / 255., 1.),
                render_type: if j % 2 == 0 {
                    RenderType::NativeRendering
                } else {
                    RenderType::QtRendering
                },
                ..Default::default()
            }),
        ));
    }
    let pixels: Vec<u8> = (0..32 * 32)
        .flat_map(|p| {
            [
                (p * 17 + i * 31) as u8,
                (p / 32 * 13) as u8,
                (p % 32 * 7) as u8,
                255,
            ]
        })
        .collect();
    let info = skia::ImageInfo::new(
        (32, 32),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        None,
    );
    let image = skia::images::raster_from_data(&info, skia::Data::new_copy(&pixels), 128).unwrap();
    for j in 0..12 {
        root.add(Node::new(
            Rect::new((j * 37) as f32, 24., 27., 27.),
            Content::Image(image.clone()),
        ));
    }
    if seed >= 24 {
        root.add(Node::new(
            Rect::new(10., 5., 80., 40.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(if seed == 24 {
                    Color::rgba(1., 0., 0., 1.)
                } else {
                    Color::rgba(0., 1., 0., 1.)
                }),
                radii: [0.; 4],
                border: None,
            }),
        ));
    }
    for node in &mut root.children {
        if matches!(node.content, Content::Text(_)) {
            node.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
        }
    }
    root.native_materials=Some(true);
    (Scene::new(root), size, scale)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(std::env::args().nth(1).ok_or("output dir")?);
    std::fs::create_dir_all(&out)?;
    let frames: usize = std::env::var("HYPRV_STRESS_FRAMES")
        .unwrap_or("6000".into())
        .parse()?;
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let qh = queue.handle();
    let compositor: WlCompositor = globals.bind(&qh, 1..=4, ())?;
    let shared = SharedEgl::new(&connection)?;
    let driver = shared.renderer_name();
    let surfaces: Vec<_> = (0..4).map(|_| compositor.create_surface(&qh, ())).collect();
    queue.roundtrip(&mut State)?;
    let mapped = std::env::var_os("HYPRV_STRESS_MAPPED").is_some();
    let mut roles = vec![];
    if mapped {
        let wm: XdgWmBase = globals.bind(&qh, 1..=1, ())?;
        for surface in &surfaces {
            let xdg = wm.get_xdg_surface(surface, &qh, ());
            let top = xdg.get_toplevel(&qh, ());
            top.set_title("Private GPU stress".into());
            surface.commit();
            roles.push((top, xdg));
        }
        queue.roundtrip(&mut State)?;
    }
    let cases: Vec<_> = (0..26).map(case).collect();
    let scratch = out.join("checkpoint.png");
    let mut expected = vec![];
    for (scene, size, scale) in &cases {
        let mut renderer = Renderer::new();
        let mut window = EglWindow::new(shared.clone(), &surfaces[0], (16, 16))?;
        window.draw(
            &mut renderer,
            scene,
            *size,
            *scale,
            Rect::new(0., 0., size.0, size.1),
        )?;
        window.png(&scratch)?;
        expected.push(hash(&std::fs::read(&scratch)?));
        renderer.clear_caches();
    }
    let mut windows: Vec<Option<EglWindow>> = (0..4).map(|_| None).collect();
    let mut renderers: Vec<_> = (0..5).map(|_| Renderer::new()).collect();
    let mut latest = [None; 4];
    let mut checks = 0;
    let mut mismatches = vec![];
    let mut resizes = 0;
    let mut recreates = 0;
    let mut sizes = [(0, 0); 4];
    let mut partial = 0;
    let interval: usize = std::env::var("HYPRV_STRESS_INTERVAL")
        .unwrap_or("31".into())
        .parse()?;
    for frame in 0..frames {
        let slot = (frame * 13 + frame / 7) % 4;
        let index = match (frame / (frames / 3).max(1)).min(2) {
            0 => (frame * 7 + frame / 23) % cases.len(),
            1 => slot,
            _ => 24 + (frame / 4) % 2,
        };
        let (scene, size, scale) = &cases[index];
        if windows[slot].is_none() || frame % 101 == 0 {
            windows[slot] = None;
            windows[slot] = Some(EglWindow::new(shared.clone(), &surfaces[slot], (16, 16))?);
            recreates += 1;
        }
        let physical = (
            (size.0 * *scale as f32) as i32,
            (size.1 * *scale as f32) as i32,
        );
        if sizes[slot] != physical {
            resizes += 1;
            sizes[slot] = physical;
        }
        let renderer = if frame % 2 == 0 { 4 } else { slot };
        let damage = if (latest[slot] == Some(index) && frame % 3 == 0)
            || (index >= 24 && latest[slot].is_some_and(|i| i >= 24))
        {
            partial += 1;
            Rect::new(10., 5., 80., 40.)
        } else {
            Rect::new(0., 0., size.0, size.1)
        };
        windows[slot].as_mut().unwrap().draw(
            &mut renderers[renderer],
            scene,
            *size,
            *scale,
            damage,
        )?;
        latest[slot] = Some(index);
        // Bursts have no readback/fence: check all outstanding surfaces after uploads,
        // swaps, size changes and a switch of the current EGL drawable.
        if frame % interval == interval - 1 || frame + 1 == frames {
            if mapped {
                queue.roundtrip(&mut State)?;
            }
            for s in 0..4 {
                if let (Some(window), Some(k)) = (&mut windows[s], latest[s]) {
                    window.png(&scratch)?;
                    checks += 1;
                    let actual = hash(&std::fs::read(&scratch)?);
                    if actual != expected[k] {
                        let name = format!("bad-{frame}-{s}-{k}.png");
                        std::fs::copy(&scratch, out.join(&name))?;
                        mismatches.push(serde_json::json!({"frame":frame,"surface":s,"case":k,"file":name,"actual":actual,"expected":expected[k]}));
                    }
                }
            }
        }
        if frame % 500 == 499 {
            eprintln!(
                "stress {}/{} checks={} mismatches={}",
                frame + 1,
                frames,
                checks,
                mismatches.len()
            );
        }
    }
    for r in &mut renderers {
        r.clear_caches();
    }
    drop(windows);
    for (top, xdg) in roles {
        top.destroy();
        xdg.destroy();
    }
    for s in surfaces {
        s.destroy();
    }
    let report = serde_json::json!({"frames":frames,"surfaces":4,"resizes":resizes,"recreates":recreates,"checkpoints":checks,"mismatches":mismatches,"driver":driver,"oracle_cases":cases.len(),"readback_every":interval,"partial_damage_frames":partial,"mapped_private_surfaces":mapped});
    std::fs::write(
        out.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    if !mismatches.is_empty() {
        return Err("GPU stress mismatch".into());
    }
    Ok(())
}
