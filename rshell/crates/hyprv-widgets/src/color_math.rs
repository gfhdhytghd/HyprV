// SPDX-License-Identifier: LGPL-3.0-only
// Numerical port of QColor HSV/lighter/darker, qtbase/src/gui/painting/qcolor.cpp v6.12.0.
// Copyright (C) 2016 The Qt Company Ltd. License text: ../../hyprv-ui/licenses/.
use hyprv_ui::animation::Color;
pub fn lighter(c: Color, factor: f64) -> Color {
    adjust(c, (factor * 100.) as i32, true)
}
pub fn darker(c: Color, factor: f64) -> Color {
    adjust(c, (factor * 100.) as i32, false)
}
fn adjust(c: Color, factor: i32, light: bool) -> Color {
    if factor <= 0 {
        return c;
    }
    if factor < 100 {
        return adjust(c, 10000 / factor, !light);
    }
    let r = (c.r * 65535.).round() as i32;
    let g = (c.g * 65535.).round() as i32;
    let b = (c.b * 65535.).round() as i32;
    let min = r.min(g).min(b);
    let mut v = r.max(g).max(b);
    let delta = (v - min) as f32;
    let mut s = if delta == 0. {
        0
    } else {
        (delta / v as f32 * 65535.).round() as i32
    };
    let hue = if delta == 0. {
        0
    } else {
        let h = if v == r {
            let h = (g - b) as f32 / delta;
            if h < 0. {
                h + 6.
            } else {
                h
            }
        } else if v == g {
            2. + (b - r) as f32 / delta
        } else {
            4. + (r - g) as f32 / delta
        };
        (h * 6000.).round() as i32
    };
    if light {
        v = factor * v / 100;
        if v > 65535 {
            s = (s - (v - 65535)).max(0);
            v = 65535;
        }
    } else {
        v = v * 100 / factor;
    }
    if s == 0 {
        let f = v as f32 / 65535.;
        return Color::rgba(f, f, f, c.a);
    }
    let h = if hue == 36000 { 0. } else { hue as f32 / 6000. };
    let i = h as i32;
    let f = h - i as f32;
    let s = s as f32 / 65535.;
    let v = v as f32 / 65535.;
    let p = v * (1. - s);
    let (r, g, b) = if i & 1 == 1 {
        let q = v * (1. - s * f);
        match i {
            1 => (q, v, p),
            3 => (p, q, v),
            _ => (v, p, q),
        }
    } else {
        let t = v * (1. - s * (1. - f));
        match i {
            0 => (v, t, p),
            2 => (p, v, t),
            _ => (t, p, v),
        }
    };
    let quant = |x: f32| (x * 65535.).round() / 65535.;
    Color::rgba(quant(r), quant(g), quant(b), c.a)
}
