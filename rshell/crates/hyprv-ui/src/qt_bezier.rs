// SPDX-License-Identifier: LGPL-3.0-only
// Numerical port of Qt's BezierEase solver, qtbase/src/corelib/tools/qeasingcurve.cpp.
// Copyright (C) 2016 The Qt Company Ltd. See ../licenses/LGPL-3.0.txt.
// Keep Qt's approximations: replacing these with exact cbrt/acos changes animation values.
fn qt_cbrt(value: f64) -> f64 {
    let sign = if value < 0. { -1. } else { 1. };
    let d = value * sign;
    let high = (d.to_bits() >> 32) as u32;
    let mut t = f64::from_bits(((high / 3 + 715094163) as u64) << 32);
    let cube = t * t * t;
    let denominator = cube + cube + d;
    if denominator != 0. {
        t = t * (cube + d + d) / denominator;
    }
    t * sign
}
fn in_range(t: f64) -> bool {
    (-0.01..=1.01).contains(&t)
}
fn solve_cubic(a: f64, b: f64, c: f64) -> f64 {
    if c.abs() < 0.000001 {
        return 0.;
    }
    let third = a / 3.;
    let p = b - a * third;
    let q = 2. * a * a * a / 27. - a * b / 3. + c;
    let d = 0.25 * q * q + p * p * p / 27.;
    if d >= 0. {
        let root = d.sqrt();
        let u = qt_cbrt(-q * 0.5 + root);
        let v = qt_cbrt(-q * 0.5 - root);
        let t = u + v - third;
        if in_range(t) {
            return t;
        }
        return -u - third;
    }
    let sqrt_p = (-p).sqrt();
    let f = (4f64 / 3.).sqrt() * sqrt_p;
    let g = -q * 0.5 * (-3. * 3f64.sqrt() / (sqrt_p * p));
    let square = g * g;
    let plus = (1. + g).sqrt();
    let minus = (1. - g).sqrt();
    let s1 = 0.463614 - 0.0347815 * g + 0.00218245 * square + 0.402421 * plus;
    let s3 = 0.463614 + 0.402421 * minus + 0.0347815 * g + 0.00218245 * square;
    let s2 = -0.401644 * minus - 0.0686804 * g + 0.401644 * plus;
    let t1 = -f * s2 - third;
    if in_range(t1) {
        return t1;
    }
    let t2 = f * s1 - third;
    if in_range(t2) {
        return t2;
    }
    -f * s3 - third
}
pub(crate) fn parameter(p0: f64, p1: f64, p2: f64, p3: f64, x: f64) -> f64 {
    let a = p3 - p0 + 3. * p1 - 3. * p2;
    let b = 3. * p0 - 6. * p1 + 3. * p2;
    let c = -3. * p0 + 3. * p1;
    let d = p0 - x;
    if a.abs() < 1e-3 {
        if b.abs() < 1e-3 {
            return if c.abs() < 1e-3 { 0. } else { -d / c };
        }
        let discriminant = c * c - 4. * b * d;
        if discriminant < 0. {
            return 0.;
        }
        if discriminant == 0. {
            return -c / (2. * b);
        }
        let first = (-c + discriminant.sqrt()) / (2. * b);
        if (0.0..=1.0).contains(&first) {
            return first;
        }
        let second = (-c - discriminant.sqrt()) / (2. * b);
        if (0.0..=1.0).contains(&second) {
            return second;
        }
        return 0.;
    }
    solve_cubic(b / a, c / a, d / a)
}
