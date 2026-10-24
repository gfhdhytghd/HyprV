//! Qt 6.12 glyph cache numerical port (LGPL-3.0-only).
//! FreeType outlines enter qSimplifyPath's exact intersection/subdivision sweep,
//! then qdistancefield's signed 8.8 edge/corner kernels. No Qt runtime is linked.
use skia_safe as sk;
unsafe extern "C" {
    fn hyprv_distance_field_path(
        coords: *const f64,
        types: *const i32,
        count: i32,
        x: f64,
        y: f64,
        out: *mut u8,
        width: i32,
        height: i32,
    ) -> bool;
}
#[cfg(test)]
pub(crate) fn raster(path: &sk::Path, width: usize, height: usize, origin: (f32, f32)) -> Vec<u8> {
    let mut coords = Vec::new();
    let mut types = Vec::new();
    let mut add = |kind, x: f64, y: f64| {
        types.push(kind);
        coords.extend([x, y]);
    };
    for (verb, p) in sk::path::Iter::new(path, true) {
        use sk::path::Verb;
        match verb {
            Verb::Move => add(0, p[0].x as f64, p[0].y as f64),
            Verb::Line => add(1, p[1].x as f64, p[1].y as f64),
            Verb::Quad | Verb::Conic => {
                // QPainterPath represents FreeType conics as equivalent cubics.
                add(
                    2,
                    (p[0].x as f64 + 2. * p[1].x as f64) / 3.,
                    (p[0].y as f64 + 2. * p[1].y as f64) / 3.,
                );
                add(
                    3,
                    (p[2].x as f64 + 2. * p[1].x as f64) / 3.,
                    (p[2].y as f64 + 2. * p[1].y as f64) / 3.,
                );
                add(3, p[2].x as f64, p[2].y as f64);
            }
            Verb::Cubic => {
                for (i, p) in p.iter().skip(1).enumerate() {
                    add(if i == 0 { 2 } else { 3 }, p.x as f64, p.y as f64);
                }
            }
            _ => {}
        }
    }
    let mut data = vec![0; width * height];
    if !types.is_empty() {
        // SAFETY: generated element types match coordinates; buffers remain live
        // through this synchronous, non-retaining call. The C++ sweep owns all
        // temporary storage and writes exactly width*height bytes.
        unsafe {
            hyprv_distance_field_path(
                coords.as_ptr(),
                types.as_ptr(),
                types.len() as i32,
                5. - origin.0 as f64,
                5. - origin.1 as f64,
                data.as_mut_ptr(),
                width as i32,
                height as i32,
            );
        }
    }
    data
}
/// Qt's outline walker preserves half-unit implied conic points; FreeType's
/// integer decompose API truncates them and produces different DF cache bytes.
pub(crate) fn outline(face: &freetype::Face) -> Option<(sk::Path, Vec<f64>, Vec<i32>)> {
    let slot = face.glyph();
    if slot.raw().outline.n_contours <= 0 {
        return None;
    }
    let outline = slot.outline()?;
    let points = outline.points();
    let tags = outline.tags();
    let mut coords = Vec::new();
    let mut kinds = Vec::new();
    let pt = |i: usize| [points[i].x as f64 / 64., -points[i].y as f64 / 64.];
    let add = |a: [f64; 2], b: [f64; 2]| [a[0] + b[0], a[1] + b[1]];
    let scale = |a: [f64; 2], n: f64| [a[0] / n, a[1] / n];
    fn emit(kind: i32, p: [f64; 2], coords: &mut Vec<f64>, kinds: &mut Vec<i32>) {
        kinds.push(kind);
        coords.extend([p[0] / 16., p[1] / 16.]);
    }
    fn curve(c: &[[f64; 2]; 4], coords: &mut Vec<f64>, kinds: &mut Vec<i32>) {
        emit(2, c[1], coords, kinds);
        emit(3, c[2], coords, kinds);
        emit(3, c[3], coords, kinds);
    }
    let mut i = 0isize;
    for last in outline.contours() {
        let last = *last as isize;
        let mut start = pt(i as usize);
        if tags[i as usize] & 1 == 0 {
            start = if tags[last as usize] & 1 == 0 {
                scale(add(pt(last as usize), start), 2.)
            } else {
                pt(last as usize)
            };
            i -= 1;
        }
        emit(0, start, &mut coords, &mut kinds);
        let mut c = [[0.; 2]; 4];
        c[0] = start;
        let mut n = 1;
        while i < last {
            i += 1;
            c[n] = pt(i as usize);
            n += 1;
            match tags[i as usize] & 3 {
                2 => {
                    if n < 4 {
                        continue;
                    }
                    c[3] = scale(add(c[3], c[2]), 2.);
                    i -= 1;
                }
                0 => {
                    if n < 3 {
                        continue;
                    }
                    c[3] = scale(add(c[1], c[2]), 2.);
                    c[2] = scale(add(scale(c[1], 0.5), c[3]), 3.);
                    c[1] = scale(add(scale(c[1], 0.5), c[0]), 3.);
                    i -= 1;
                }
                _ => {
                    if n == 2 {
                        emit(1, c[1], &mut coords, &mut kinds);
                        c[0] = c[1];
                        n = 1;
                        continue;
                    } else if n == 3 {
                        c[3] = c[2];
                        c[2] = scale(add(scale(c[1], 0.5), c[3]), 3.);
                        c[1] = scale(add(scale(c[1], 0.5), c[0]), 3.);
                    }
                }
            }
            curve(&c, &mut coords, &mut kinds);
            c[0] = c[3];
            n = 1;
        }
        if n > 1 {
            c[3] = start;
            if n == 2 {
                c[2] = scale(add(scale(c[1], 0.5), c[3]), 3.);
                c[1] = scale(add(scale(c[1], 0.5), c[0]), 3.);
            }
            curve(&c, &mut coords, &mut kinds);
        } else {
            emit(1, start, &mut coords, &mut kinds);
        }
        i += 1;
    }
    let mut path = sk::Path::new();
    let mut i = 0;
    let point = |i: usize| (coords[i * 2] as f32, coords[i * 2 + 1] as f32);
    while i < kinds.len() {
        match kinds[i] {
            0 => {
                path.close();
                path.move_to(point(i));
            }
            1 => {
                path.line_to(point(i));
            }
            2 => {
                path.cubic_to(point(i), point(i + 1), point(i + 2));
                i += 2;
            }
            _ => {}
        }
        i += 1;
    }
    path.close();
    Some((path, coords, kinds))
}
pub(crate) fn raster_elements(
    coords: &[f64],
    types: &[i32],
    width: usize,
    height: usize,
    origin: (f32, f32),
) -> Vec<u8> {
    let mut data = vec![0; width * height];
    if !types.is_empty() {
        unsafe {
            hyprv_distance_field_path(
                coords.as_ptr(),
                types.as_ptr(),
                types.len() as i32,
                5. - origin.0 as f64,
                5. - origin.1 as f64,
                data.as_mut_ptr(),
                width as i32,
                height as i32,
            );
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_point_thin_rectangle() {
        let path = sk::Path::rect(sk::Rect::from_xywh(0., 0., 48., 3.), None);
        let data = raster(&path, 58, 13, (0., 0.));
        assert_eq!(data[6 * 58 + 24], 165);
        assert_eq!(data[6 * 58], 12);
    }
}
