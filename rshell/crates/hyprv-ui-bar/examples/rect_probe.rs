use hyprv_ui::{animation, geometry, qt_rect, scene};
fn main() {
    let width = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(223.375);
    let pen = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.);
    let height = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(38.);
    let radius = std::env::args()
        .nth(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(19.);
    let rect = scene::Rectangle {
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
    };
    let m = qt_rect::tessellate(geometry::Rect::new(0., 0., width, height), &rect).unwrap();
    println!(
        "{}",
        serde_json::json!({"vertices":m.vertices.iter().map(|v|vec![v.position[0]as f64,v.position[1]as f64,v.offset[0]as f64,v.offset[1]as f64,v.color[0]as f64,v.color[1]as f64,v.color[2]as f64,v.color[3]as f64]).collect::<Vec<_>>(),"triangles":m.triangles})
    );
}
