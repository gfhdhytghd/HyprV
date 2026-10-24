use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    render::Renderer,
    scene::{Content, Fill, Node, Rectangle, Scene},
};
use std::rc::Rc;
#[test]
fn shared_subtrees_keep_rendering_hits_damage_and_immutable_identity() {
    let mut leaf = Node::new(
        Rect::new(3., 4., 10., 12.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
            ..Default::default()
        }),
    );
    leaf.interactive = true;
    let leaf_id = leaf.id();
    let tree = Rc::new(leaf);
    let wrapper = Node::new(Rect::new(7., 8., 20., 24.), Content::Shared(tree.clone()));
    let wrapper_id = wrapper.id();
    let mut scene = Scene::new(wrapper);
    assert_eq!(scene.hit_test(11., 13.), Some(leaf_id));
    assert!(scene.root().find(leaf_id).is_some());
    assert!(!scene.edit(leaf_id, |_| panic!("immutable subtree edited")));
    assert!(!scene.set_position(leaf_id, 4., 5.));
    scene.take_damage(Rect::new(0., 0., 50., 50.));
    assert!(scene.set_position(wrapper_id, 17., 18.));
    let damage = scene.take_damage(Rect::new(0., 0., 50., 50.)).unwrap();
    assert!(damage.contains(10., 12.) && damage.contains(29., 32.));
    assert_eq!(scene.hit_test(21., 23.), Some(leaf_id));
    let mut renderer = Renderer::new();
    let mut a = renderer.raster(&scene, 50., 50., 1.).unwrap();
    let mut direct = Node::new(Rect::new(17., 18., 20., 24.), Content::Group);
    let mut leaf = Node::new(
        Rect::new(3., 4., 10., 12.),
        Content::Rect(Rectangle {
            fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
            ..Default::default()
        }),
    );
    leaf.interactive = true;
    direct.add(leaf);
    let mut b = renderer.raster(&Scene::new(direct), 50., 50., 1.).unwrap();
    let info = hyprv_ui::skia::ImageInfo::new(
        (50, 50),
        hyprv_ui::skia::ColorType::RGBA8888,
        hyprv_ui::skia::AlphaType::Premul,
        None,
    );
    let mut aa = vec![0u8; 10000];
    let mut bb = aa.clone();
    assert!(a.image_snapshot().read_pixels(
        &info,
        &mut aa,
        200,
        (0, 0),
        hyprv_ui::skia::image::CachingHint::Allow
    ));
    assert!(b.image_snapshot().read_pixels(
        &info,
        &mut bb,
        200,
        (0, 0),
        hyprv_ui::skia::image::CachingHint::Allow
    ));
    assert_eq!(aa, bb);
}
