use hyprv_ui::offscreen;
// Independent engine probe matching tools/rect_effect.qml, without bar content.
use hyprv_ui::{
    animation::Color,
    effects::MultiEffect,
    geometry::Rect,
    render::Renderer,
    scene::{Border, Content, Fill, Node, Rectangle, Scene},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);
    std::fs::create_dir_all(&out)?;
    for scale in [1., 2.] {
        for effect in [false, true] {
            let mut root = Node::new(Rect::new(0., 0., 428., 358.), Content::Group);
            let mut layer = Node::new(Rect::new(64., 64., 300., 230.), Content::Group);
            let mut rect = Node::new(
                Rect::new(0., 0., 300., 230.),
                Content::Rect(Rectangle {
                    fill: Fill::Solid(Color::rgba(16. / 255., 18. / 255., 20. / 255., 0.42)),
                    radii: [19.; 4],
                    border: Some(Border {
                        width: 1.,
                        color: Color::rgba(205. / 255., 214. / 255., 244. / 255., 0.14),
                    }),
                }),
            );
            rect.opacity = 0.82;
            layer.add(rect);
            if effect {
                layer.effects.multi_effect = Some(MultiEffect {
                    source_texture_size: Some(((300. * scale) as u32, (230. * scale) as u32)),
                    shadow_enabled: true,
                    shadow_blur: 10. / 64.,
                    shadow_color: Color::rgba(0., 0., 0., 0.45),
                    ..Default::default()
                });
            }
            root.add(layer);
            root.native_materials = Some(true);
            let scene = Scene::new(root);
            let mut renderer = Renderer::new();
            let path = out.join(format!("effect-{effect}--dark@{scale}.png"));
            if let Some(png) =
                offscreen::render_png_if_enabled(&mut renderer, &scene, 428., 358., scale)?
            {
                std::fs::write(path, png)?;
            } else {
                renderer.png(&scene, 428., 358., scale, path)?;
            }
        }
    }
    Ok(())
}
