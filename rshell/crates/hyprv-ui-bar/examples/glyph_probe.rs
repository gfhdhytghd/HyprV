//! Independent native glyph isolation; optional parity QA production GL path.
use hyprv_ui::offscreen;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, Node, Rectangle, RenderType, Scene, Text},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);
    std::fs::create_dir_all(&out)?;
    for scale in [1., 2.] {
        for effect in [false, true] {
            let mut root = Node::new(Rect::new(0., 0., 80., 60.), Content::Group);
            if effect {
                root.add(Node::new(
                    Rect::new(10., 10., 223.375, 38.),
                    Content::Rect(Rectangle {
                        fill: Fill::Solid(Color::rgba(30. / 255., 30. / 255., 46. / 255., 0.8)),
                        radii: [19.; 4],
                        border: None,
                    }),
                ));
            }
            root.add(Node::new(
                Rect::new(17., 17., 17.1875, 23.),
                Content::Text(Text {
                    text: "\u{f303}".into(),
                    families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
                    size: 17.,
                    weight: 700,
                    color: Color::rgba(137. / 255., 180. / 255., 250. / 255., 1.),
                    align: hyprv_ui::skia::textlayout::TextAlign::Center,
                    render_type: RenderType::NativeRendering,
                    ..Default::default()
                }),
            ));
            if std::env::var_os("HYPRV_GLYPH_COLOR_FILTER").is_some() {
                // Diagnostic: isolate A8 mask color math from Skia's half paint.
                let mut glyph = root.children.pop().unwrap();
                if let Content::Text(t) = &mut glyph.content {
                    t.color = Color::rgba(1., 1., 1., 1.);
                }
                let mut mask = Renderer::new().raster(&Scene::new(glyph), 80., 60., scale)?;
                let image = mask.image_snapshot();
                let shader_mode =
                    std::env::var("HYPRV_GLYPH_COLOR_FILTER").as_deref() == Ok("shader");
                let source = match std::env::var("HYPRV_GLYPH_COLOR_FILTER").as_deref() {
                    Ok("shader") => "uniform shader mask; uniform float4 color; float4 main(float2 pos) { return color * float(mask.eval(pos).a); }",
                    Ok("simple") => "uniform float4 color; half4 main(half4 mask) { return color * float(mask.a); }",
                    Ok("float") => "uniform float4 color; float4 main(float4 mask) { return color * mask.a; }",
                    _ => "uniform float4 color; half4 main(half4 mask) { float a=floor(float(mask.a)*255.0+0.5)/255.0; return floor(color*a*255.0+0.5)/255.0; }",
                };
                let effect = if shader_mode {
                    hyprv_ui::skia::RuntimeEffect::make_for_shader(source, None)
                } else {
                    hyprv_ui::skia::RuntimeEffect::make_for_color_filter(source, None)
                }
                .unwrap();
                let data: Vec<u8> = [137f32 / 255., 180. / 255., 250. / 255., 1.]
                    .into_iter()
                    .flat_map(f32::to_ne_bytes)
                    .collect();
                let mut recorder = hyprv_ui::skia::PictureRecorder::new();
                let canvas =
                    recorder.begin_recording(hyprv_ui::skia::Rect::from_wh(80., 60.), false);
                let mut paint = hyprv_ui::skia::Paint::default();
                let data = hyprv_ui::skia::Data::new_copy(&data);
                if shader_mode {
                    let child = image
                        .to_shader(None, hyprv_ui::skia::SamplingOptions::default(), None)
                        .unwrap();
                    let shader = effect.make_shader(data, &[child.into()], None).unwrap();
                    paint.set_shader(shader);
                    canvas.scale((1. / scale as f32, 1. / scale as f32));
                    canvas.draw_rect(
                        hyprv_ui::skia::Rect::from_wh(80. * scale as f32, 60. * scale as f32),
                        &paint,
                    );
                } else {
                    paint.set_color_filter(effect.make_color_filter(data, None).unwrap());
                    canvas.draw_image_rect(
                        &image,
                        None,
                        hyprv_ui::skia::Rect::from_wh(80., 60.),
                        &paint,
                    );
                }
                root.add(Node::new(
                    Rect::new(0., 0., 80., 60.),
                    Content::Picture(recorder.finish_recording_as_picture(None).unwrap()),
                ));
            }
            for node in &mut root.children {
                if matches!(node.content, Content::Text(_)) {
                    node.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
                }
            }
            root.native_materials=Some(true);
            let scene = Scene::new(root);
            let mut renderer = Renderer::new();
            let path = out.join(format!("effect-{effect}--dark@{scale}.png"));
            if let Some(png) =
                offscreen::render_png_if_enabled(&mut renderer, &scene, 80., 60., scale)?
            {
                std::fs::write(path, png)?;
            } else {
                renderer.png(&scene, 80., 60., scale, path)?;
            }
        }
    }
    Ok(())
}
