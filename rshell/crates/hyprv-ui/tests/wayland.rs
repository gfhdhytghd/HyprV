//! Explicitly opted-in smoke test: two small non-exclusive, non-focusable surfaces, <3s.
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Fill, Node, Rectangle, Scene},
    wayland::{Application, InputEvent, LayerConfig, Response, SurfaceId, UiHandler},
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
struct Handler(Rc<RefCell<Vec<f64>>>);
impl UiHandler for Handler {
    fn input(&mut self, _: SurfaceId, _: &mut Scene, event: InputEvent) -> Response {
        if let InputEvent::Resize {
            scale,
            width,
            height,
        } = event
        {
            assert!(
                width > 0 && height > 0,
                "resize must wait for a valid configure"
            );
            self.0.borrow_mut().push(scale);
        }
        Response::default()
    }
}
#[test]
#[ignore = "brief live Wayland test; run explicitly with WAYLAND_DISPLAY set"]
fn shared_egl_service_wakeup_and_idle() {
    let scales = Rc::new(RefCell::new(Vec::new()));
    let mut app = Application::connect(Handler(scales.clone())).unwrap();
    let outputs = app.engine.outputs();
    let mut ids = Vec::new();
    for i in 0..2 {
        let mut root = Node::new(
            Rect::new(0., 0., 100., 30.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(0.3, 0.5, 0.8, 0.5)),
                radii: [8.; 4],
                border: None,
            }),
        );
        root.effects.multi_effect = Some(hyprv_ui::effects::MultiEffect {
            shadow_enabled: true,
            shadow_blur: 10. / 64.,
            ..Default::default()
        });
        let node = root.id();
        let config = LayerConfig {
            size: (100, 30),
            output: outputs
                .get(i % outputs.len().max(1))
                .map(|(o, _)| o.clone()),
            ..Default::default()
        };
        ids.push((app.engine.add_surface(config, Scene::new(root)), node));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.engine.frames < 2 && Instant::now() < deadline {
        app.dispatch(Some(Duration::from_millis(100))).unwrap();
    }
    assert!(
        app.engine.frames >= 2,
        "both configured surfaces must paint"
    );
    // Drain feedback and verify idle no longer requests frame callbacks.
    app.dispatch(Some(Duration::from_millis(60))).unwrap();
    println!(
        "output globals={}, observed surface scales={:?}",
        outputs.len(),
        scales.borrow()
    );
    let frames = app.engine.frames;
    app.dispatch(Some(Duration::from_millis(80))).unwrap();
    assert_eq!(app.engine.frames, frames);
    let handle = app.handle();
    let (surface, node) = ids[0];
    let sender = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        handle
            .send(move |engine| {
                engine
                    .scene(surface)
                    .unwrap()
                    .edit(node, |node| node.opacity = 0.25);
            })
            .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    while app.engine.frames == frames && Instant::now() < deadline {
        app.dispatch(Some(Duration::from_millis(100))).unwrap();
    }
    sender.join().unwrap();
    assert_eq!(app.engine.frames, frames + 1);
    for (surface, _) in ids {
        app.engine.close(surface);
    }
    assert_eq!(app.engine.surface_count(), 0);
    app.dispatch(Some(Duration::from_millis(20))).unwrap();
}
