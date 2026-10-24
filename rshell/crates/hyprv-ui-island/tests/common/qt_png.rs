//! Qt QImage ARGB32PM→RGBA8888 capture conversion (qdrawhelper_sse4.cpp).
//! Test transport only: the compositor still receives original premultiplied pixels.
use super::{sk,Img};
fn straight(pixel:&mut [u8]) {
    let alpha=pixel[3];
    if alpha==255 {return;}
    if alpha==0 {pixel.fill(0);return;}
    #[cfg(target_arch="x86_64")]
    unsafe {
        use std::arch::x86_64::*;
        let a=_mm_set1_ps(alpha as f32);
        let mut inv=_mm_rcp_ps(a);
        inv=_mm_sub_ps(_mm_add_ps(inv,inv),_mm_mul_ps(inv,_mm_mul_ps(inv,a)));
        inv=_mm_mul_ps(inv,_mm_set1_ps(255.));
        let rgb=_mm_setr_epi32(pixel[0]as i32,pixel[1]as i32,pixel[2]as i32,0);
        let out=_mm_cvtps_epi32(_mm_mul_ps(_mm_cvtepi32_ps(rgb),inv));
        let mut channels=[0i32;4];_mm_storeu_si128(channels.as_mut_ptr().cast(),out);
        for c in 0..3 {pixel[c]=channels[c].clamp(0,255)as u8;}
    }
    #[cfg(not(target_arch="x86_64"))]
    for channel in &mut pixel[..3] {*channel=(*channel as f64*255./alpha as f64).round_ties_even().clamp(0.,255.)as u8;}
}
pub fn capture(image:&sk::Image)->Img {
    let (w,h)=(image.width()as usize,image.height()as usize);
    let info=sk::ImageInfo::new((w as i32,h as i32),sk::ColorType::RGBA8888,sk::AlphaType::Premul,None);
    let mut px=vec![0;w*h*4];assert!(image.read_pixels(&info,&mut px,w*4,(0,0),sk::image::CachingHint::Allow));
    for pixel in px.chunks_exact_mut(4) {straight(pixel);}
    Img{w,h,px}
}
#[cfg(all(test,target_arch="x86_64"))]
#[test]
fn conversion_matches_independent_qimage_simd_oracle() {
    let oracle=include_bytes!("../fixtures/qt-unpremultiply.bin");
    for alpha in 0..=255u32 {for value in 0..=alpha {
        let mut pixel=[value as u8;4];pixel[3]=alpha as u8;straight(&mut pixel);
        let expected=oracle[(alpha*256+value)as usize];
        assert_eq!(pixel,[expected,expected,expected,alpha as u8],"value={value},alpha={alpha}");
    }}
}
