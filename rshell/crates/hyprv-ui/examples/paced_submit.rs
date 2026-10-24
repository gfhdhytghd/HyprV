//! Explicit private-compositor lower-bound probe: one changed pixel at 1 Hz.
//! Same production EGL/Skia/Wayland path and 3072×58 logical bar at output DPR.
use calloop::timer::{TimeoutAction, Timer};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Fill, Node, Rectangle, Scene},
    wayland::{Application, LayerConfig, SurfaceId, UiHandler},
};
use std::time::Duration;
struct Handler;
impl UiHandler for Handler {
    fn update_after_frame(&self) -> bool {
        false
    }
    fn presentation_feedback(&self, _: SurfaceId) -> bool {
        false
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() != Ok("1") {
        return Err("private compositor required".into());
    }
    let mut root = Node::new(
        Rect::new(0., 0., 3072., 58.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(0.1, 0.2, 0.3, 0.6)),
            ..Default::default()
        }),
    );
    root.native_materials = Some(true);
    let pixel = root.add(Node::new(
        Rect::new(20., 20., 1., 1.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
            ..Default::default()
        }),
    ));
    let mut app = Application::connect(Handler)?;
    let surface = app.engine.add_surface(
        LayerConfig {
            namespace: "hyprv-quickshell".into(),
            size: (3072, 58),
            ..Default::default()
        },
        Scene::new(root),
    );
    let mut tick = 0;
    app.event_loop_handle().insert_source(
        Timer::from_duration(Duration::from_secs(1)),
        move |_, _, engine| {
            tick += 1;
            if tick == 2 {
                hyprv_ui::diagnostics::clear();
            }
            if tick == 34 {
                engine.quit();
                return TimeoutAction::Drop;
            }
            hyprv_ui::diagnostics::mark("probe.tick", "");
            if let Some(scene) = engine.scene(surface) {
                scene.edit(pixel, |n| {
                    if let Content::Rect(r) = &mut n.content {
                        r.fill = Fill::Solid(Color::rgba((tick % 2) as f32, 0., 1., 1.));
                    }
                });
            }
            engine.request_redraw();
            TimeoutAction::ToDuration(Duration::from_secs(1))
        },
    )?;
    app.run()?;
    Ok(())
}
