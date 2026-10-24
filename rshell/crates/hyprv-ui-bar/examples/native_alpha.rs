use hyprv_ui::offscreen;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, NativeTextMode, Node, Rectangle, RenderType, Scene, Text},
    skia::textlayout::TextAlign,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&out)?;
    for scale in [1., 2.] {
        for effect in [false, true] {
            let mut root = Node::new(Rect::new(0., 0., 320., 240.), Content::Group);
            if effect {
                root.add(Node::new(
                    Rect::new(0., 0., 320., 240.),
                    Content::Rect(Rectangle {
                        fill: Fill::Solid(Color::rgba(30. / 255., 30. / 255., 46. / 255., 0.8)),
                        radii: [0.; 4],
                        border: None,
                    }),
                ));
            }
            for index in 0..16 {
                let mut text = Node::new(
                    Rect::new(
                        (index % 4) as f32 * 80. + 17.,
                        (index / 4) as f32 * 60. + 17.,
                        45.,
                        23.,
                    ),
                    Content::Text(Text {
                        text: "\u{f303}AB".into(),
                        families: vec!["JetBrainsMono Nerd Font".into()],
                        size: 17.,
                        weight: 700,
                        color: Color::rgba(
                            137. / 255.,
                            180. / 255.,
                            250. / 255.,
                            [1., 0.72, 0.14, 0.98][index % 4],
                        ),
                        align: TextAlign::Center,
                        render_type: RenderType::NativeRendering,
                        ..Default::default()
                    }),
                );
                text.opacity = [1., 0.82, 0.45, 0.13][index / 4];
                text.native_text_mode = NativeTextMode::QtGl;
                root.add(text);
            }
            let mut renderer = Renderer::new();
            root.native_materials = Some(true);
            let scene = Scene::new(root);
            let path = out.join(format!("effect-{effect}--dark@{scale}.png"));
            if let Some(png) =
                offscreen::render_png_if_enabled(&mut renderer, &scene, 320., 240., scale)?
            {
                std::fs::write(path, png)?
            } else {
                renderer.png(&scene, 320., 240., scale, path)?
            }
        }
    }
    Ok(())
}
