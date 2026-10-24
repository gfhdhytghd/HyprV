//! Explicitly run on a private compositor: production EGL, not a CPU surrogate.
use hyprv_ui::offscreen;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, Node, OpacityMode, Rectangle, RenderType, Scene, Text},
    skia as sk,
};
static GL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn scene(background: bool) -> Node {
    let mut root = Node::new(Rect::new(0., 0., 80., 60.), Content::Group);
    if background {
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
            align: sk::textlayout::TextAlign::Center,
            render_type: RenderType::NativeRendering,
            ..Default::default()
        }),
    ));
    for node in &mut root.children {
        if matches!(node.content, Content::Text(_)) {
            node.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
        }
    }
    root
}
fn decode(bytes: &[u8]) -> Vec<u8> {
    let image = sk::Image::from_encoded(sk::Data::new_copy(bytes)).unwrap();
    let info = sk::ImageInfo::new(
        (image.width(), image.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut pixels,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Disallow
    ));
    pixels
}
fn render(scene: &Scene, scale: f64, disabled: bool) -> Vec<u8> {
    if disabled {
        std::env::set_var("HYPRV_NATIVE_GL_DISABLE", "1");
    } else {
        std::env::remove_var("HYPRV_NATIVE_GL_DISABLE");
    }
    let mut renderer = Renderer::new();
    let bytes = offscreen::render_png_if_enabled(&mut renderer, scene, 80., 60., scale)
        .unwrap()
        .expect("set HYPRV_PARITY_BACKEND=gl");
    renderer.clear_caches();
    decode(&bytes)
}
#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn native_gl_goldens_clip_layer_opacity_and_interleaving() {
    let _guard=GL_LOCK.lock().unwrap();
    for scale in [1., 2.] {
        for background in [false, true] {
            let name = format!(
                "{}/tests/fixtures/native-gl/{}@{scale}.png",
                env!("CARGO_MANIFEST_DIR"),
                if background { "background" } else { "plain" }
            );
            assert_eq!(
                render(&Scene::new(scene(background)), scale, false),
                decode(&std::fs::read(name).unwrap()),
                "independent Qt golden, background={background} scale={scale}"
            );
        }
    }
    for mode in 0..7 {
        for scale in [1., 1.25, 2.] {
            let mut node = scene(true);
            match mode {
                0 => {
                    node.clip = Some([0.; 4]);
                    node.frame.width = 30.;
                }
                1 => {
                    node.clip = Some([11.; 4]);
                    node.frame.width = 30.;
                }
                2 => {
                    node.opacity_mode = OpacityMode::Group;
                    node.opacity = 0.45;
                }
                3 => {
                    node.effects.blur_sigma = 1.3;
                }
                6 => {
                    if let Content::Text(t) = &mut node.children.last_mut().unwrap().content {
                        t.color.a = 0.72;
                    }
                }
                5 => {
                    node.clip = Some([0.; 4]);
                    node.frame = Rect::new(0.3, 0., 29.4, 60.);
                    node.rect_mode = Some(hyprv_ui::qt_rect::RectMode::Skia);
                }
                _ => {
                    node.opacity = 0.45;
                }
            }
            let scene = Scene::new(node);
            let old = render(&scene, scale, true);
            let new = render(&scene, scale, false);
            let max = old
                .iter()
                .zip(&new)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            if matches!(mode, 1..=3 | 5) {
                assert_eq!(old, new, "unsupported clip/layer must use original path");
            } else {
                assert!(max <= 2, "clip/opacity max {max}");
            }
            if mode == 0 {
                let width = (80. * scale) as usize;
                let right = (30. * scale).round() as usize;
                for row in new.chunks(width * 4) {
                    assert!(row[right * 4..].iter().all(|b| *b == 0));
                }
            }
        }
    }
    std::env::remove_var("HYPRV_NATIVE_GL_DISABLE");
}

#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn native_color_and_inherited_alpha_match_qt_qrgb8_goldens() {
    let _guard=GL_LOCK.lock().unwrap();
    for scale in [1., 2.] {
        for background in [false, true] {
            let mut root = Node::new(Rect::new(0., 0., 320., 240.), Content::Group);
            if background {
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
                        align: sk::textlayout::TextAlign::Center,
                        render_type: RenderType::NativeRendering,
                        ..Default::default()
                    }),
                );
                text.opacity = [1., 0.82, 0.45, 0.13][index / 4];
                text.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
                root.add(text);
            }
            let actual = offscreen::render_png_if_enabled(
                &mut Renderer::new(),
                &Scene::new(root),
                320.,
                240.,
                scale,
            )
            .unwrap()
            .expect("GL backend");
            let expected = std::fs::read(format!(
                "{}/tests/fixtures/native-gl-alpha/effect-{background}--dark@{scale}.png",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            assert_eq!(
                decode(&actual),
                decode(&expected),
                "Qt QRgb8 alpha golden background={background} scale={scale}"
            );
        }
    }
}


#[path="common/batch_material.rs"]
mod batch_material;
#[test]
#[ignore="requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn native_batch_vertices_texture_opacity_and_layer_origin_match_qt() {
    let _lock=GL_LOCK.lock().unwrap();
    std::env::remove_var("HYPRV_NATIVE_GL_DISABLE");
    for scale in [1.,2.] {for faded in [false,true] {
        let scene=batch_material::scene(faded,scale as f32);
        let png=offscreen::render_png_if_enabled(&mut Renderer::new(),&scene,3072.,1728.,scale).unwrap().expect("GL backend");
        let expected=std::fs::read(format!("{}/tests/fixtures/batch-materials/effect-{faded}--dark@{scale}.png",env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(decode(&png),decode(&expected),"Qt batch/image/effect golden faded={faded} scale={scale}");
    }}
}

#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn native_fractional_mask_uses_qt_nearest_sampling() {
    let _guard=GL_LOCK.lock().unwrap();
    let mut root=Node::new(Rect::new(0.,0.,80.,60.),Content::Group);
    root.native_materials=Some(true);
    let mut masked=Node::new(Rect::new(17.375,10.25,40.,40.),Content::Group);
    masked.opacity=0.91;
    masked.effects.multi_effect=Some(hyprv_ui::effects::MultiEffect {
        mask:Some(hyprv_ui::effects::Mask {
            source:hyprv_ui::effects::MaskSource::Rect {radius:11.},
            threshold_min:0.5,spread_at_min:1.,..Default::default()
        }),..Default::default()
    });
    masked.add(Node::new(Rect::new(0.,0.,40.,40.),Content::Rect(Rectangle {
        fill:Fill::Solid(Color::rgba(137./255.,180./255.,250./255.,1.)),radii:[0.;4],border:None
    })));
    root.add(masked);
    let scene=Scene::new(root);
    for scale in [1.,2.] {
        let path=format!("{}/tests/fixtures/native-gl/mask@{scale}.png",env!("CARGO_MANIFEST_DIR"));
        assert_eq!(render(&scene,scale,false),decode(&std::fs::read(path).unwrap()),"Qt fractional mask DPR{scale}");
    }
}

#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn native_rectangular_clip_rounds_qt_projected_scissor() {
    let _guard=GL_LOCK.lock().unwrap();
    let mut root=Node::new(Rect::new(0.,0.,80.,60.),Content::Group);
    root.native_materials=Some(true);
    let mut clipped=Node::new(Rect::new(5.,10.,70.,38.75),Content::Rect(Rectangle {
        fill:Fill::Solid(Color::rgba(137./255.,180./255.,250./255.,1.)),radii:[11.;4],border:None
    }));
    clipped.clip=Some([0.;4]);root.add(clipped);
    let scene=Scene::new(root);
    for scale in [1.,2.] {
        let path=format!("{}/tests/fixtures/native-gl/clip@{scale}.png",env!("CARGO_MANIFEST_DIR"));
        assert_eq!(render(&scene,scale,false),decode(&std::fs::read(path).unwrap()),"Qt clipped rect DPR{scale}");
    }
}

#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn scaled_sibling_prevents_smooth_rectangle_batch_merging() {
    let _guard=GL_LOCK.lock().unwrap();
    let mut root=Node::new(Rect::new(0.,0.,640.,200.),Content::Group);
    root.native_materials=Some(true);
    let mut group=Node::new(Rect::new(124.,10.,392.,180.),Content::Group);
    group.clip=Some([0.;4]);
    group.add(Node::new(Rect::new(174.,119.,44.,44.),Content::Rect(Rectangle {
        fill:Fill::Solid(Color::rgba(137./255.,180./255.,250./255.,184./255.)),radii:[22.;4],border:None
    })));
    let mut sibling=Node::new(Rect::new(236.,121.,40.,40.),Content::Group);
    sibling.transform=sk::Matrix::translate((-1.2,-1.2));
    sibling.transform.pre_scale((1.06,1.06),None);
    sibling.add(Node::new(Rect::new(0.,0.,40.,40.),Content::Rect(Rectangle {
        fill:Fill::Solid(Color::rgba(30./255.,30./255.,46./255.,33./255.)),radii:[20.;4],
        border:Some(hyprv_ui::scene::Border {width:1.,color:Color::rgba(205./255.,214./255.,244./255.,29./255.)})
    })));
    group.add(sibling);root.add(group);
    let scene=Scene::new(root);
    for scale in [1.,2.] {
        let actual=offscreen::render_png_if_enabled(&mut Renderer::new(),&scene,640.,200.,scale).unwrap().expect("GL backend");
        let expected=std::fs::read(format!("{}/tests/fixtures/native-gl/batch@{scale}.png",env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(decode(&actual),decode(&expected),"Qt unmerged sibling batch DPR{scale}");
    }
}

#[test]
#[ignore = "requires private Weston and HYPRV_PARITY_BACKEND=gl"]
fn explicit_layer_size_keeps_qt_logical_projection() {
    let _guard=GL_LOCK.lock().unwrap();
    for scale in [1.,2.] {
        let mut root=Node::new(Rect::new(0.,0.,3072.,1728.),Content::Group);
        root.native_materials=Some(true);
        let mut card=Node::new(Rect::new(10.,58.,548.,572.),Content::Group);
        card.clip=Some([0.;4]);
        let mut layer=Node::new(Rect::new(0.,0.,548.,572.),Content::Group);
        layer.effects.multi_effect=Some(hyprv_ui::effects::MultiEffect {
            shadow_enabled:true,shadow_blur:10./64.,shadow_color:Color::rgba(0.,0.,0.,0.45),
            source_texture_size:Some(((548.*scale)as u32,(572.*scale)as u32)),
            ..Default::default()
        });
        let mut fill=Node::new(Rect::new(0.,0.,548.,572.),Content::Rect(Rectangle {
            fill:Fill::Solid(Color::rgba(16./255.,18./255.,20./255.,0.42)),radii:[19.;4],
            border:Some(hyprv_ui::scene::Border {width:1.,color:Color::rgba(205./255.,214./255.,244./255.,0.14)})
        }));
        fill.opacity=0.82;layer.add(fill);card.add(layer);root.add(card);
        let actual=offscreen::render_png_if_enabled(&mut Renderer::new(),&Scene::new(root),3072.,1728.,scale).unwrap().expect("GL backend");
        let expected=std::fs::read(format!("{}/tests/fixtures/native-gl/layer-projection@{scale}.png",env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(decode(&actual),decode(&expected),"Qt logical layer projection DPR{scale}");
    }
}
