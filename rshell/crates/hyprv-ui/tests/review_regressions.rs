//! Retained raster and affine-text regressions for review UI-1/UI-2/UI-4.
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Border, Content, Fill, Node, RenderType, Scene, StrokeStyle, Text},
    skia as sk,
};
fn pixels(surface: &mut sk::Surface) -> Vec<u8> {
    let image = surface.image_snapshot();
    let info = sk::ImageInfo::new_n32_premul((image.width(), image.height()), None);
    let mut bytes = vec![0; image.width() as usize * image.height() as usize * 4];
    assert!(image.read_pixels(
        &info,
        &mut bytes,
        image.width() as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow
    ));
    bytes
}
#[test]
fn path_edits_clear_complete_stroke_and_replacement_keeps_identity() {
    for detailed in [false, true] {
        for operation in ["hide", "move", "remove", "replace"] {
            let mut path = sk::Path::new();
            path.move_to((-8., 10.))
                .line_to((5., -8.))
                .line_to((18., 10.));
            let white = Color::rgba(1., 1., 1., 1.);
            let content = if detailed {
                Content::StrokedPath {
                    path,
                    fill: Fill::default(),
                    stroke: StrokeStyle {
                        width: 20.,
                        color: white,
                        cap: sk::paint::Cap::Square,
                        join: sk::paint::Join::Miter,
                    },
                }
            } else {
                Content::Path {
                    path,
                    fill: Fill::default(),
                    stroke: Some(Border {
                        width: 20.,
                        color: white,
                    }),
                }
            };
            let mut root = Node::new(Rect::new(0., 0., 128., 128.), Content::Group);
            let root_id = root.id();
            let id = root.add(Node::new(Rect::new(40., 40., 10., 10.), content));
            let mut scene = Scene::new(root);
            let mut renderer = Renderer::new();
            let mut retained = renderer.raster(&scene, 128., 128., 1.).unwrap();
            let extent = Rect::new(0., 0., 128., 128.);
            scene.take_damage(extent);
            match operation {
                "hide" => {
                    scene.edit(id, |n| n.visible = false);
                }
                "move" => {
                    scene.set_position(id, 78., 78.);
                }
                "remove" => {
                    scene.edit(root_id, |n| n.children.clear());
                }
                "replace" => {
                    scene.edit(id, |n| {
                        *n = Node::new(Rect::new(75., 70., 8., 8.), Content::Group)
                    });
                    assert_eq!(scene.root().find(id).unwrap().id(), id);
                }
                _ => unreachable!(),
            }
            let damage = scene.take_damage(extent).unwrap();
            renderer
                .paint(retained.canvas(), &scene, 1., damage)
                .unwrap();
            let mut fresh = Renderer::new().raster(&scene, 128., 128., 1.).unwrap();
            assert_eq!(
                pixels(&mut retained),
                pixels(&mut fresh),
                "{operation}, detailed={detailed}"
            );
        }
    }
}
#[test]
fn text_masks_follow_complete_affine_transform() {
    for render_type in [RenderType::NativeRendering, RenderType::QtRendering] {
        for (name, matrix) in [
            ("rotation", sk::Matrix::rotate_deg(90.)),
            ("nonuniform", sk::Matrix::scale((2., 0.75))),
            ("reflection", sk::Matrix::scale((-1., 1.))),
            (
                "shear",
                sk::Matrix::new_all(1., 0.5, 0., 0., 1., 0., 0., 0., 1.),
            ),
        ] {
            let dpr = matrix
                .scale_x()
                .hypot(matrix.skew_y())
                .max(matrix.skew_x().hypot(matrix.scale_y()));
            let text = Text {
                text: "ROTATE".into(),
                size: 18.,
                families: vec!["JetBrainsMono Nerd Font".into()],
                render_type,
                elide: false,
                ..Text::default()
            };
            let mut node = Node::new(Rect::new(100., 70., 90., 30.), Content::Text(text.clone()));
            node.transform = matrix;
            let scene = Scene::new(node);
            let mut actual = Renderer::new().raster(&scene, 300., 220., 1.).unwrap();
            // Independently transform a complete local raster through the same affine matrix.
            let mut base = sk::surfaces::raster_n32_premul((
                (120. * dpr).ceil() as i32,
                (60. * dpr).ceil() as i32,
            ))
            .unwrap();
            base.canvas().scale((dpr, dpr));
            hyprv_ui::text::TextEngine::new()
                .unwrap()
                .paint(base.canvas(), &text, 90., 1.)
                .unwrap();
            let image = base.image_snapshot();
            let mut expected = sk::surfaces::raster_n32_premul((300, 220)).unwrap();
            expected
                .canvas()
                .translate((100., 70.))
                .concat(&matrix)
                .scale((1. / dpr, 1. / dpr));
            expected.canvas().draw_image(image, (0., 0.), None);
            let actual = pixels(&mut actual);
            let expected = pixels(&mut expected);
            let ink: usize = actual.chunks_exact(4).filter(|p| p[3] > 0).count();
            assert!(ink > 50, "{name} {render_type:?}");
            let differences = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
            assert!(
                differences < 60,
                "{name} {render_type:?}: {differences} differing channels"
            );
        }
    }
}

#[test]
fn whole_node_replacement_invalidates_existing_effect_cache() {
    fn colored(color: Color) -> Node {
        let mut node = Node::new(
            Rect::new(20., 20., 30., 30.),
            Content::Rect(hyprv_ui::scene::Rectangle {
                fill: Fill::Solid(color),
                ..Default::default()
            }),
        );
        node.effects.multi_effect = Some(hyprv_ui::effects::MultiEffect::default());
        node
    }
    let mut scene = Scene::new(colored(Color::rgba(1., 0., 0., 1.)));
    let id = scene.root().id();
    scene.edit(id, |_| {}); // Cache revision two, which a fresh node reaches after touch.
    let mut renderer = Renderer::new();
    renderer.raster(&scene, 80., 80., 1.).unwrap();
    scene.edit(id, |n| *n = colored(Color::rgba(0., 0., 1., 1.)));
    let mut cached = renderer.raster(&scene, 80., 80., 1.).unwrap();
    let mut fresh = Renderer::new().raster(&scene, 80., 80., 1.).unwrap();
    assert_eq!(pixels(&mut cached), pixels(&mut fresh));
}
