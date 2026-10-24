// Verbatim copy of the previous hyprv-ui qt_rect rasteriser (float coverage tests), kept
// only to measure the rewrite against it.
use hyprv_ui::{geometry::Rect, qt_rect::{device_vertex, tessellate}, scene::Rectangle, skia as sk};
fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn top_left(a: [f32; 2], b: [f32; 2]) -> bool {
    b[1] < a[1] || (b[1] == a[1] && b[0] > a[0])
}
pub fn draw(canvas: &sk::Canvas, bounds: Rect, rect: &Rectangle, opacity: f32) {
    let matrix = canvas.local_to_device_as_3x3();
    let antialias = rect.radii[0] > 0.;
    let Some(mesh) = tessellate(bounds, rect) else { return };
    if mesh.vertices.is_empty() {
        return;
    }
    let vertices: Vec<_> = mesh.vertices.iter().map(|v| {
        let mut v = *v;
        if !antialias {
            v.offset = [0., 0.];
        }
        device_vertex(v, &matrix)
    }).collect();
    let left = vertices.iter().map(|v| v.0[0]).fold(f32::INFINITY, f32::min).floor() as i32;
    let top = vertices.iter().map(|v| v.0[1]).fold(f32::INFINITY, f32::min).floor() as i32;
    let right = vertices.iter().map(|v| v.0[0]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let bottom = vertices.iter().map(|v| v.0[1]).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let (w, h) = ((right - left) as usize, (bottom - top) as usize);
    if w == 0 || h == 0 {
        return;
    }
    let mut data = vec![0u8; w * h * 4];
    for triangle in mesh.triangles {
        let mut t = triangle.map(|i| vertices[i as usize]);
        let mut area = edge(t[0].0, t[1].0, t[2].0);
        if area == 0. {
            continue;
        }
        if area < 0. {
            t.swap(1, 2);
            area = -area;
        }
        let coverage = t.map(|v| v.0.map(|c| (c * 256.).round_ties_even() / 256.));
        let x0 = (coverage.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min) - 0.5).ceil().max(left as f32) as i32;
        let x1 = (coverage.iter().map(|v| v[0]).fold(f32::NEG_INFINITY, f32::max) - 0.5).floor().min((right - 1) as f32) as i32;
        let y0 = (coverage.iter().map(|v| v[1]).fold(f32::INFINITY, f32::min) - 0.5).ceil().max(top as f32) as i32;
        let y1 = (coverage.iter().map(|v| v[1]).fold(f32::NEG_INFINITY, f32::max) - 0.5).floor().min((bottom - 1) as f32) as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let ce = [edge(coverage[1], coverage[2], p), edge(coverage[2], coverage[0], p), edge(coverage[0], coverage[1], p)];
                if ce.iter().enumerate().any(|(i, e)| *e < 0. || (*e == 0. && !top_left(coverage[(i + 1) % 3], coverage[(i + 2) % 3]))) {
                    continue;
                }
                let e = [edge(t[1].0, t[2].0, p), edge(t[2].0, t[0].0, p), edge(t[0].0, t[1].0, p)];
                let color: [f32; 4] = std::array::from_fn(|c| {
                    let weights = e.map(|v| v / area);
                    (weights[0] * (t[0].1[c] as f32 / 255.) + weights[1] * (t[1].1[c] as f32 / 255.) + weights[2] * (t[2].1[c] as f32 / 255.)) * opacity.clamp(0., 1.)
                });
                let at = (((y - top) as usize) * w + (x - left) as usize) * 4;
                let inverse = 1. - color[3];
                for c in 0..4 {
                    data[at + c] = ((color[c] + data[at + c] as f32 / 255. * inverse) * 255.).round_ties_even().clamp(0., 255.) as u8;
                }
            }
        }
    }
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let Some(image) = sk::images::raster_from_data(&info, sk::Data::new_copy(&data), w * 4) else { return };
    let _save = sk::AutoCanvasRestore::guard(canvas, true);
    canvas.reset_matrix();
    canvas.draw_image(image, (left as f32, top as f32), None);
}
