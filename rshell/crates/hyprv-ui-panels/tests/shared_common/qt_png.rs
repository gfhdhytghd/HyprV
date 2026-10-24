//! Reference PNG encoding shared by the panels and calendar-morph harnesses.
/// parity-host's `QImage::convertToFormat(Format_RGBA8888)` of the premultiplied grab
/// (the references' encoding): Qt's SSE4.1 `convertARGBFromARGB32PM_sse4` per scanline
/// chunk, four pixels at a time with an `rcpps` reciprocal refined by one Newton-Raphson
/// step and round-to-nearest-even, the remainder through `qUnpremultiply_sse4`
/// (qt_inv_premul_factor, +0x8000). `rcpps` is CPU-specific; this matches the capture host.
pub fn qt_unpremultiply_rows(px: &mut [u8], w: usize) {
    const CHUNK: usize = 1024; // QImage generic converter buffer
    for row in px.chunks_mut(w * 4) {
        for chunk in row.chunks_mut(CHUNK * 4) {
            let n = chunk.len() / 4;
            let mut i = 0;
            while i + 4 <= n {
                unpremultiply4(&mut chunk[i * 4..i * 4 + 16]);
                i += 4;
            }
            for p in chunk[i * 4..].chunks_mut(4) {
                unpremultiply_scalar(p);
            }
        }
    }
}
fn unpremultiply_scalar(p: &mut [u8]) {
    let a = p[3] as u32;
    if a == 255 {
        return;
    }
    if a == 0 {
        p.copy_from_slice(&[0; 4]);
        return;
    }
    let inv = (255 * 65535 + a / 2) / a;
    for c in &mut p[..3] {
        *c = ((*c as u32 * inv + 0x8000) >> 16).min(255) as u8;
    }
}
#[cfg(target_arch = "x86_64")]
fn unpremultiply4(px: &mut [u8]) {
    use std::arch::x86_64::*;
    let alphas = [px[3], px[7], px[11], px[15]];
    if alphas.iter().all(|&a| a == 0) {
        px.fill(0);
        return;
    }
    if alphas.iter().all(|&a| a == 255) {
        return;
    }
    // SAFETY: SSE/SSE2 are baseline on x86_64.
    let ia: [f32; 4] = unsafe {
        let a = _mm_setr_ps(alphas[0] as f32, alphas[1] as f32, alphas[2] as f32, alphas[3] as f32);
        let r = _mm_rcp_ps(a);
        let r = _mm_sub_ps(_mm_add_ps(r, r), _mm_mul_ps(r, _mm_mul_ps(r, a)));
        let mut o = [0f32; 4];
        _mm_storeu_ps(o.as_mut_ptr(), _mm_mul_ps(r, _mm_set1_ps(255.)));
        o
    };
    for k in 0..4 {
        if alphas[k] == 0 {
            px[k * 4..k * 4 + 4].fill(0);
            continue;
        }
        for c in 0..3 {
            let v = px[k * 4 + c] as f32 * ia[k];
            // cvtps2dq (round to nearest even), then unsigned saturation
            px[k * 4 + c] = (v.round_ties_even() as i32).clamp(0, 255) as u8;
        }
    }
}
#[cfg(not(target_arch = "x86_64"))]
fn unpremultiply4(px: &mut [u8]) {
    for p in px.chunks_mut(4) {
        unpremultiply_scalar(p);
    }
}
