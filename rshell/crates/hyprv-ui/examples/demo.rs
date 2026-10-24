//! Safe defaults: no reserved space, no keyboard focus, stops animation after 800ms.
//! --offscreen PATH writes a PNG without connecting to Wayland.
//! --idle-test SECONDS exits automatically and reports engine frames and /proc CPU ticks.
use hyprv_ui::{
    animation::{Behavior, Color, Direction, Easing, Family},
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, Node, NodeId, Rectangle, Scene, Text},
    wayland::{Application, FrameInfo, InputEvent, LayerConfig, Response, SurfaceId, UiHandler},
};
use std::time::{Duration, Instant};
fn scene() -> (Scene, NodeId) {
    let mut root = Node::new(
        Rect::new(0., 0., 400., 100.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.08, 0.09, 0.12, 0.82)),
            radii: [24.; 4],
            border: None,
        }),
    );
    let mut animated = Node::new(
        Rect::new(18., 18., 64., 64.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.48, 0.69, 1., 0.95)),
            radii: [18.; 4],
            border: None,
        }),
    );
    animated.effects.multi_effect = Some(hyprv_ui::effects::MultiEffect {
        shadow_enabled: true,
        shadow_blur: 10. / 64.,
        shadow_color: Color::rgba(0., 0., 0., 0.4),
        ..Default::default()
    });
    let rect = root.add(animated);
    root.add(Node::new(
        Rect::new(105., 30., 280., 50.),
        Content::Text(Text {
            text: "HyprV · Rust + Skia".into(),
            size: 20.,
            ..Default::default()
        }),
    ));
    (Scene::new(root), rect)
}
struct Demo {
    rect: NodeId,
    x: Behavior<f64>,
    started: Option<Duration>,
}
impl UiHandler for Demo {
    fn frame(&mut self, _: SurfaceId, scene: &mut Scene, time: FrameInfo) -> bool {
        if self.started.is_none() {
            self.started = Some(time.time);
            self.x.retarget(34., time.time);
        }
        let x = self.x.advance(time.time) as f32;
        if scene.root().find(self.rect).is_some_and(|n| n.frame.x != x) {
            scene.set_position(self.rect, x, 18.);
        }
        self.x.is_running()
    }
    fn input(&mut self, _: SurfaceId, _: &mut Scene, _: InputEvent) -> Response {
        Response::default()
    }
}
fn cpu_ticks() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .map(|(_, s)| s.split_whitespace().collect())
        .unwrap_or_default();
    [11, 12]
        .into_iter()
        .filter_map(|i| fields.get(i)?.parse::<u64>().ok())
        .sum()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let (scene, rect) = scene();
    if args.get(1).is_some_and(|a| a == "--offscreen") {
        let path = args.get(2).ok_or("--offscreen requires output path")?;
        let start = Instant::now();
        let mut renderer = Renderer::new();
        renderer.png(&scene, 400., 100., 1.25, path)?;
        let warmup = start.elapsed();
        let start = Instant::now();
        for _ in 0..100 {
            let _ = renderer.raster(&scene, 400., 100., 1.25)?;
        }
        println!(
            "PNG {path}; cold {:?}; 100 CPU frames (text/effects cached): {:?}",
            warmup,
            start.elapsed()
        );
        return Ok(());
    }
    let handler = Demo {
        rect,
        x: Behavior::new(
            18.,
            Duration::from_millis(800),
            Easing::new(Family::Back, Direction::Out),
        ),
        started: None,
    };
    let mut app = Application::connect(handler)?;
    app.engine.add_surface(
        LayerConfig {
            size: (400, 100),
            ..Default::default()
        },
        scene,
    );
    if args.get(1).is_some_and(|a| a == "--idle-test") {
        let seconds: f64 = args.get(2).ok_or("--idle-test requires seconds")?.parse()?;
        if !(2.0..=30.0).contains(&seconds) {
            return Err("idle test must last 2..30 seconds".into());
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(1200) {
            app.dispatch(Some(Duration::from_millis(50)))?;
        }
        let ticks = cpu_ticks();
        let frames = app.engine.frames;
        let until = start + Duration::from_secs_f64(seconds);
        let mut dispatches = 0;
        while Instant::now() < until {
            app.dispatch(Some(until.saturating_duration_since(Instant::now())))?;
            dispatches += 1;
        }
        println!("idle: CPU ticks delta={}, frame delta={}, dispatches={} (includes final test timeout); total frames={}, render+submit={:?}",cpu_ticks()-ticks,app.engine.frames-frames,dispatches,app.engine.frames,app.engine.render_time);
        if app.engine.frames > 1 {
            println!(
                "first frame={:?}; warm render+submit mean={:?}; last={:?}",
                app.engine.first_render_time,
                (app.engine.render_time - app.engine.first_render_time.unwrap())
                    / (app.engine.frames as u32 - 1),
                app.engine.last_render_time
            );
        }
        return Ok(());
    }
    app.run()?;
    Ok(())
}
