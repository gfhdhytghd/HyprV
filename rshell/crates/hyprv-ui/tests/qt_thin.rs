//! Installed Qt6.12 geometry oracle, tools/rect_mesh.cpp in the bar crate.
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    qt_rect,
    scene::{Fill, Rectangle},
};
#[test]
fn clamped_radius_keeps_qt_inner_radius_and_paired_rows() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/qt-thin/mesh.json")).unwrap();
    let mesh = qt_rect::tessellate(
        Rect::new(0., 0., 300., 1.),
        &Rectangle {
            fill: Fill::Solid(Color::rgba(30. / 255., 30. / 255., 46. / 255., 1.)),
            radii: [1.; 4],
            border: None,
        },
    )
    .unwrap();
    let rows = expected["vertices"].as_array().unwrap();
    assert_eq!(mesh.vertices.len(), rows.len());
    for (v, row) in mesh.vertices.iter().zip(rows) {
        let actual = [v.position[0], v.position[1], v.offset[0], v.offset[1]];
        for (i, component) in actual.iter().enumerate() {
            assert_eq!(*component, row[i].as_f64().unwrap() as f32);
        }
        assert_eq!(
            v.color,
            std::array::from_fn(|i| row[i + 4].as_u64().unwrap() as u8)
        );
    }
    let indices = expected["indices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect::<Vec<_>>();
    assert_eq!(mesh.indices,indices,"canonical GPU strip preserves Qt degenerates");
    let triangles = indices
        .windows(3)
        .enumerate()
        .filter(|(_, v)| v[0] != v[1] && v[0] != v[2] && v[1] != v[2])
        .map(|(i, t)| {
            if i % 2 == 0 {
                [t[0], t[1], t[2]]
            } else {
                [t[1], t[0], t[2]]
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(mesh.triangles, triangles);
}
