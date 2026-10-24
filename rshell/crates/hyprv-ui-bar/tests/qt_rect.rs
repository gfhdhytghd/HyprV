//! Independent Qt-emitted mesh golden data, including circles and fractional pens.
use hyprv_ui::{animation, geometry, qt_rect, scene, skia};
fn rect(radius: f32, pen: f32) -> scene::Rectangle {
    scene::Rectangle {
        fill: scene::Fill::Solid(animation::Color::rgba(
            30. / 255.,
            30. / 255.,
            46. / 255.,
            1.,
        )),
        radii: [radius; 4],
        border: if pen > 0. {
            Some(scene::Border {
                width: pen,
                color: animation::Color::rgba(205. / 255., 214. / 255., 244. / 255., 29. / 255.),
            })
        } else {
            None
        },
    }
}
#[test]
fn geometry_and_topology_match_qt_private_node() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("data/qt_rect_mesh.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let f = |name: &str| case[name].as_f64().unwrap() as f32;
        let mesh = qt_rect::tessellate(
            geometry::Rect::new(0., 0., f("width"), f("height")),
            &rect(f("radius"), f("pen")),
        )
        .unwrap();
        let expected = case["vertices"].as_array().unwrap();
        assert_eq!(mesh.vertices.len(), expected.len());
        for (actual, expected) in mesh.vertices.iter().zip(expected) {
            let v = [
                actual.position[0],
                actual.position[1],
                actual.offset[0],
                actual.offset[1],
                actual.color[0] as f32,
                actual.color[1] as f32,
                actual.color[2] as f32,
                actual.color[3] as f32,
            ];
            let expected: Vec<f32> = expected
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            assert_eq!(v.as_slice(), expected, "{case}");
        }
        let indices: Vec<u16> = case["indices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        let mut expected: Vec<[u16; 3]> = indices
            .windows(3)
            .filter(|v| v[0] != v[1] && v[0] != v[2] && v[1] != v[2])
            .map(|v| {
                let mut t = [v[0], v[1], v[2]];
                t.sort();
                t
            })
            .collect();
        let mut actual = mesh.triangles;
        for t in &mut actual {
            t.sort();
        }
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
}
#[test]
fn snapped_scan_bounds_do_not_crack_circle_fill() {
    let raster = qt_rect::rasterize(
        geometry::Rect::new(0., 0., 32., 32.),
        &rect(16., 0.),
        &skia::Matrix::translate((69.375, 13.)),
        1.,
    )
    .unwrap();
    let info = skia::ImageInfo::new(
        (raster.bounds.width(), raster.bounds.height()),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Premul,
        None,
    );
    let mut data = vec![0u8; info.width() as usize * info.height() as usize * 4];
    assert!(raster.image.read_pixels(
        &info,
        &mut data,
        info.width() as usize * 4,
        (0, 0),
        skia::image::CachingHint::Allow
    ));
    // This Qt-covered row was omitted when scan bounds used unsnapped vertices.
    let x = 72 - raster.bounds.left;
    let y = 36 - raster.bounds.top;
    let i = ((y * info.width() + x) * 4) as usize;
    assert_eq!(&data[i..i + 4], &[30, 30, 46, 255]);
}
#[test]
fn unsupported_geometry_is_explicit() {
    let mut r = rect(19., 0.);
    r.radii[1] = 8.;
    assert!(qt_rect::tessellate(geometry::Rect::new(0., 0., 100., 38.), &r).is_none());
    assert!(qt_rect::rasterize(
        geometry::Rect::new(0., 0., 32., 32.),
        &rect(16., 0.),
        &skia::Matrix::new_identity(),
        f32::NAN
    )
    .is_none());
}
