//! Private protocol probe, including compositor-sized (zero) axes.
use super::*;
thread_local! { static SURFACE: RefCell<Option<SurfaceId>> = const { RefCell::new(None) }; }

pub(super) fn dispatch(
    engine: &mut Engine,
    args: &[String],
) -> Result<Value, Box<dyn std::error::Error>> {
    let size: (u32, u32) = serde_json::from_str(args.first().ok_or("size")?)?;
    SURFACE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(id) = *slot {
            engine.configure_surface(id, size, Layer::Overlay, 0, KeyboardInteractivity::None);
        } else {
            *slot = Some(engine.add_surface(
                LayerConfig {
                    namespace: "hyprv-test-atomic-size".into(),
                    anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                    size,
                    layer: Layer::Overlay,
                    input_passthrough: true,
                    ..Default::default()
                },
                Scene::new(hyprv_ui::scene::Node::new(
                    Rect::new(0., 0., 300., 820.),
                    hyprv_ui::scene::Content::Group,
                )),
            ));
        }
    });
    Ok(json!({"requested":size}))
}

pub(super) fn run(s: &mut suite::Suite) {
    s.case("engine.layer-resize", "Wayland atomic layer size / buffer commits", |s| {
        for size in [(300, 820), (300, 58), (0, 820), (0, 58), (300, 820), (300, 58)] {
            s.rpc("layer-probe", Some(json!(size)))?;
            s.settle(300);
        }
        Ok(json!({"sizes":[[300,820],[300,58],[0,820],[0,58],[300,820],[300,58]],"assertions":"WAYLAND_DEBUG checked by hyprv-ui/tests/layer-commits.py"}))
    });
}
