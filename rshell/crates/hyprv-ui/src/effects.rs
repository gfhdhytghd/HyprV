// SPDX-License-Identifier: LGPL-3.0-only
// Qt MultiEffect blur/shadow numerical and shader port.
// Copyright (C) 2023 The Qt Company Ltd.
// Reference: qtdeclarative/src/effects/{qquickmultieffect.cpp,data/shaders/*} v6.12.0.
//! Normalized Qt MultiEffect blur/shadow, with Qt's downsample pyramid and mixing.
use crate::{animation::Color, render::RenderError};
use skia_safe::{self as sk, Canvas, Data, Image, Paint, RuntimeEffect, Shader};
#[derive(Clone, Debug, PartialEq)]
pub struct MultiEffect {
    pub blur_enabled: bool,
    pub blur: f32,
    pub blur_max: u32,
    pub blur_multiplier: f32,
    pub auto_padding: bool,
    /// Explicit QML layer.textureSize, in physical pixels. Padding changes the
    /// captured logical region, but does not enlarge this source texture.
    /// None captures at the padded logical extent times the output scale.
    pub source_texture_size: Option<(u32, u32)>,
    pub shadow_enabled: bool,
    pub shadow_blur: f32,
    pub shadow_scale: f32,
    pub shadow_offset: (f32, f32),
    pub shadow_color: Color,
    pub shadow_opacity: f32,
    /// MultiEffect.colorization (0..1) and colorizationColor (Qt default #ff0000).
    /// multieffect.frag: rgb = gray(rgb)·c.rgb·a + rgb·(1−a), a = c.a·colorization,
    /// gray = dot(rgb, (0.299, 0.587, 0.114)) on premultiplied rgb.
    pub colorization: f32,
    pub colorization_color: Color,
    /// MultiEffect.maskEnabled + maskSource and thresholds; None = maskEnabled false.
    pub mask: Option<Mask>,
}
/// MultiEffect mask (multieffect.frag MASK branch). The source texture is sampled with
/// the effect's own texture coordinates, as in Qt.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub source: MaskSource,
    pub threshold_min: f32,
    pub spread_at_min: f32,
    pub threshold_max: f32,
    pub spread_at_max: f32,
    pub inverted: bool,
}
impl Default for Mask {
    fn default() -> Self {
        Self { source: MaskSource::Rect { radius: 0. }, threshold_min: 0., spread_at_min: 0., threshold_max: 1., spread_at_max: 0., inverted: false }
    }
}
impl Mask {
    /// QQuickMultiEffectPrivate::updateMaskThresholdSpread.
    pub fn uniform(&self) -> [f32; 4] {
        let c0 = 0.0001;
        let c1 = 1. - c0;
        let mt1 = self.threshold_min + c0;
        let ms1 = self.spread_at_min + 1.;
        let mt2 = c1 - self.threshold_max;
        let ms2 = self.spread_at_max + 1.;
        [mt1 * ms1 - (ms1 - c1), mt1 * ms1, mt2 * ms2 - (ms2 - c1), mt2 * ms2]
    }
}
/// What the mask texture shows.
#[derive(Clone, Debug)]
pub enum MaskSource {
    /// A hidden `Rectangle { anchors.fill: <effect item>; radius; layer.enabled: true }`
    /// (opaque white, Qt geometry/AA via `qt_rect`), its layer at item size × scale.
    Rect { radius: f32 },
    /// A prepared mask texture (premultiplied; only alpha is used).
    Image(Image),
}
impl PartialEq for MaskSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Rect { radius: a }, Self::Rect { radius: b }) => a == b,
            (Self::Image(a), Self::Image(b)) => a.unique_id() == b.unique_id(),
            _ => false,
        }
    }
}
impl Default for MultiEffect {
    fn default() -> Self {
        Self {
            blur_enabled: false,
            blur: 0.,
            blur_max: 32,
            blur_multiplier: 0.,
            auto_padding: true,
            source_texture_size: None,
            shadow_enabled: false,
            shadow_blur: 1.,
            shadow_scale: 1.,
            shadow_offset: (0., 0.),
            shadow_color: Color::rgba(0., 0., 0., 1.),
            shadow_opacity: 1.,
            colorization: 0.,
            colorization_color: Color::rgba(1., 0., 0., 1.),
            mask: None,
        }
    }
}
impl MultiEffect {
    pub fn padding(&self) -> f32 {
        if self.auto_padding && (self.blur_enabled || self.shadow_enabled) {
            (self.blur_max as f32 * (1. + self.blur_multiplier.max(0.))).trunc()
        } else {
            0.
        }
    }
    pub(crate) fn levels(&self) -> usize {
        if !(self.blur_enabled || self.shadow_enabled) || self.blur_max == 0 {
            0
        } else if self.blur_max > 32 {
            5
        } else if self.blur_max > 16 {
            4
        } else {
            3
        }
    }
    /// Qt's normalized weights (source plus five downsample levels).
    pub fn weights(&self, amount: f32) -> [f32; 6] {
        if self.blur_max == 0 {
            return [1., 0., 0., 0., 0., 0.];
        }
        let lod = (amount.clamp(0., 1.) * self.blur_max as f32 / 64.).sqrt() * 1.2 - 0.2;
        let mut w = [0.; 6];
        for (i, v) in w.iter_mut().enumerate() {
            *v = (1. - (lod - (0.1 + i as f32 * 0.2)).abs() * 2.).clamp(0., 1.);
        }
        let sum: f32 = w.iter().sum();
        for v in &mut w {
            *v /= sum;
        }
        w
    }
}
const DOWNSAMPLE: &str = r#"
uniform shader source;
uniform float2 ratio;
uniform float2 offset;
half4 main(float2 p) {
    float2 q=p*ratio;
    return (source.eval(q+float2(offset.x,offset.y*0.33))
           +source.eval(q+float2(offset.x*0.33,-offset.y))
           +source.eval(q+float2(-offset.x*0.33,offset.y))
           +source.eval(q+float2(-offset.x,-offset.y*0.33)))*0.25;
}"#;
const COMPOSE: &str = r#"
uniform shader src0; uniform shader src1; uniform shader src2;
uniform shader src3; uniform shader src4; uniform shader src5;
uniform float2 extent;
uniform float4 blurWeights1; uniform float2 blurWeights2;
uniform float4 shadowWeights1; uniform float2 shadowWeights2;
uniform float4 shadowColor;
uniform float2 shadowOffset;
uniform float shadowScale;
uniform float shadowEnabled;
uniform shader maskSrc;
uniform float4 colorizationColor;
uniform float4 maskSpread;
uniform float maskInverted;
uniform float maskEnabled;
half4 main(float2 p) {
    float2 q=p/extent;
    half4 color=src0.eval(q)*blurWeights1.x+src1.eval(q)*blurWeights1.y
        +src2.eval(q)*blurWeights1.z+src3.eval(q)*blurWeights1.w
        +src4.eval(q)*blurWeights2.x+src5.eval(q)*blurWeights2.y;
    if(colorizationColor.a>0.0) {
        float gray=dot(float3(color.rgb),float3(0.299,0.587,0.114));
        color.rgb=half3(gray*colorizationColor.rgb*colorizationColor.a
            +float3(color.rgb)*(1.0-colorizationColor.a));
    }
    if(shadowEnabled>0.5) {
        float2 s=(q-shadowOffset)*shadowScale+(1.0-shadowScale)*0.5;
        half shadow=src0.eval(s).a*shadowWeights1.x+src1.eval(s).a*shadowWeights1.y
            +src2.eval(s).a*shadowWeights1.z+src3.eval(s).a*shadowWeights1.w
            +src4.eval(s).a*shadowWeights2.x+src5.eval(s).a*shadowWeights2.y;
        half aa=min(1.0,color.a+(1.0-color.a)*(1.0-shadow));
        color=mix(half4(shadowColor),color,aa);
    }
    if(maskEnabled>0.5) {
        float alphaMask=maskSrc.eval(q).a;
        float m1=smoothstep(maskSpread.x,maskSpread.y,alphaMask);
        float m2=smoothstep(maskSpread.z,maskSpread.w,1.0-alphaMask);
        float mm=m1*m2;
        color*=half((1.0-maskInverted)*mm+maskInverted*(1.0-mm));
    }
    return color;
}"#;
#[derive(Clone)]
pub(crate) struct NativeEffect { pub images: Vec<Image>, pub uniforms: Vec<f32> }
pub(crate) struct Programs {
    downsample: RuntimeEffect,
    compose: RuntimeEffect,
}
fn bytes(values: &[f32]) -> Data {
    let v: Vec<u8> = values.iter().flat_map(|n| n.to_ne_bytes()).collect();
    Data::new_copy(&v)
}
fn image_shader(image: &Image, normalized: bool) -> Result<Shader, RenderError> {
    let matrix = normalized
        .then(|| sk::Matrix::scale((1. / image.width() as f32, 1. / image.height() as f32)));
    image
        .to_shader(
            (sk::TileMode::Clamp, sk::TileMode::Clamp),
            sk::FilterMode::Linear,
            matrix.as_ref(),
        )
        .ok_or(RenderError::Skia)
}
pub(crate) fn surface(c: &Canvas, w: i32, h: i32) -> Result<sk::Surface, RenderError> {
    if crate::native_gl::materials_active() {
        if let Some(mut context)=c.direct_context() {
            return sk::gpu::surfaces::render_target(&mut context,sk::gpu::Budgeted::Yes,
                &sk::ImageInfo::new_n32_premul((w.max(1),h.max(1)),None),None,
                sk::gpu::SurfaceOrigin::TopLeft,None,None,None).ok_or(RenderError::Skia);
        }
    }
    c.new_surface(
        &sk::ImageInfo::new_n32_premul((w.max(1), h.max(1)), None),
        None,
    )
    .ok_or(RenderError::Skia)
}
impl Programs {
    pub fn new() -> Self {
        let _timing = crate::diagnostics::scope("effect.sksl_compile");
        Self {
            downsample: RuntimeEffect::make_for_shader(DOWNSAMPLE, None)
                .expect("validated Qt downsample shader"),
            compose: RuntimeEffect::make_for_shader(COMPOSE, None)
                .expect("validated Qt compose shader"),
        }
    }
    #[cfg(test)]
    fn apply(&self,c:&Canvas,source:Image,logical:(f32,f32),scale:f32,effect:&MultiEffect)->Result<Image,RenderError> {
        self.apply_prepared(c,source,logical,scale,effect).map(|(image,_)|image)
    }
    pub fn apply_prepared(
        &self,
        c: &Canvas,
        source: Image,
        logical: (f32, f32),
        scale: f32,
        effect: &MultiEffect,
    ) -> Result<(Image,NativeEffect), RenderError> {
        let _timing = crate::diagnostics::scope("effect.apply");
        let mut levels = vec![source];
        let first = ((logical.0 / 16.).ceil() * 8., (logical.1 / 16.).ceil() * 8.);
        for i in 0..effect.levels() {
            let factor = 2f32.powi(i as i32);
            let size = (first.0 / factor, first.1 / factor);
            let previous = levels.last().unwrap();
            let (w, h) = (
                (size.0 * scale).ceil().max(1.) as i32,
                (size.1 * scale).ceil().max(1.) as i32,
            );
            let uniforms = [
                previous.width() as f32 / w as f32,
                previous.height() as f32 / h as f32,
                (1. + effect.blur_multiplier.max(0.)) * previous.width() as f32 / size.0,
                (1. + effect.blur_multiplier.max(0.)) * previous.height() as f32 / size.1,
            ];
            let shader = self
                .downsample
                .make_shader(
                    bytes(&uniforms),
                    &[image_shader(previous, false)?.into()],
                    None,
                )
                .ok_or(RenderError::Skia)?;
            let mut target = surface(c, w, h)?;
            let mut p = Paint::default();
            p.set_shader(shader).set_blend_mode(sk::BlendMode::Src);
            let offsets=[uniforms[2]/previous.width() as f32,uniforms[3]/previous.height() as f32];
            if !crate::native_gl::effect(&mut target,std::slice::from_ref(previous),&offsets,true)? {
                target.canvas().draw_paint(&p);
            }
            levels.push(target.image_snapshot());
        }
        // The source layer may have an explicit, smaller textureSize. Shader
        // coordinates and output storage follow the padded effect extent.
        let output_size = crate::render::physical_size(logical.0, logical.1, scale as f64)?;
        let mut uniforms = vec![output_size.0 as f32, output_size.1 as f32];
        let blur = if effect.blur_enabled {
            effect.weights(effect.blur)
        } else {
            [1., 0., 0., 0., 0., 0.]
        };
        uniforms.extend(blur);
        uniforms.extend(effect.weights(effect.shadow_blur));
        let color = effect.shadow_color;
        let opacity = effect.shadow_opacity.clamp(0., 1.);
        // Match Qt's shader uniform construction, including its color-alpha treatment.
        uniforms.extend([
            color.r * opacity,
            color.g * opacity,
            color.b * opacity,
            color.a * opacity,
        ]);
        uniforms.extend([
            effect.shadow_offset.0 / logical.0,
            effect.shadow_offset.1 / logical.1,
            1. / effect.shadow_scale.max(0.000001),
            if effect.shadow_enabled { 1. } else { 0. },
        ]);
        // Missing LOD children are transparent (their Qt shader branches are compiled out).
        let mut transparent = surface(c, 1, 1)?;
        transparent.canvas().clear(sk::Color::TRANSPARENT);
        let transparent = transparent.image_snapshot();
        let mut children = Vec::with_capacity(7);
        for i in 0..6 {
            children.push(image_shader(levels.get(i).unwrap_or(&transparent), true)?.into());
        }
        let cc = effect.colorization_color;
        uniforms.extend([cc.r, cc.g, cc.b, (cc.a * effect.colorization).clamp(0., 1.)]);
        let mask_image = effect.mask.as_ref().map(|mask| {
            mask_texture(c, &mask.source, (logical.0-2.*effect.padding(),logical.1-2.*effect.padding()),scale)
        }).transpose()?;
        match &effect.mask {
            Some(mask) => {
                children.push(image_shader(mask_image.as_ref().unwrap(), true)?.into());
                uniforms.extend(mask.uniform());
                uniforms.extend([if mask.inverted { 1. } else { 0. }, 1.]);
            }
            None => {
                children.push(image_shader(&transparent, true)?.into());
                uniforms.extend([0., 0., 0., 0., 0., 0.]);
            }
        }
        let shader = self
            .compose
            .make_shader(bytes(&uniforms), &children, None)
            .ok_or(RenderError::Skia)?;
        // The GL path composes straight into the framebuffer; on CPU keep the composed
        // effect in float so drawing it rounds once (ENG-BLEND-1).
        let mut target = if c.recording_context().is_none() && crate::render::exact_enabled() {
            c.new_surface(&sk::ImageInfo::new((output_size.0.max(1), output_size.1.max(1)), sk::ColorType::RGBAF32, sk::AlphaType::Premul, None), None).ok_or(RenderError::Skia)?
        } else {
            surface(c, output_size.0, output_size.1)?
        };
        let mut p = Paint::default();
        p.set_shader(shader).set_blend_mode(sk::BlendMode::Src);
        let mut textures=levels.clone();
        while textures.len()<6 {textures.push(transparent.clone());}
        textures.push(mask_image.unwrap_or_else(||transparent.clone()));
        let mut native_uniforms=uniforms.clone();
        let q=|v:f32|(v.clamp(0.,1.)*65535.).round()/65535.;
        for (i,c) in [effect.shadow_color.r,effect.shadow_color.g,effect.shadow_color.b,effect.shadow_color.a].into_iter().enumerate() {native_uniforms[14+i]=q(c)*opacity;}
        for (i,c) in [cc.r,cc.g,cc.b].into_iter().enumerate(){native_uniforms[22+i]=q(c);}
        native_uniforms[25]=(q(cc.a)*effect.colorization).clamp(0.,1.);
        native_uniforms.extend([if effect.blur_enabled {1.}else{0.},effect.levels() as f32]);
        if !crate::native_gl::effect(&mut target,&textures,&native_uniforms,false)? {
            target.canvas().draw_paint(&p);
        }
        Ok((target.image_snapshot(),NativeEffect { images:textures,uniforms:native_uniforms }))
    }
}
/// The mask item's layer texture (item size × scale).
fn mask_texture(c: &Canvas, source: &MaskSource, item: (f32, f32), scale: f32) -> Result<Image, RenderError> {
    match source {
        MaskSource::Image(image) => Ok(image.clone()),
        MaskSource::Rect { radius } => {
            let size = crate::render::physical_size(item.0, item.1, scale as f64)?;
            let mut target = surface(c, size.0, size.1)?;
            target.canvas().clear(sk::Color::TRANSPARENT);
            let r = radius.min(item.0 / 2.).min(item.1 / 2.).max(0.);
            let rect = crate::scene::Rectangle { fill: crate::scene::Fill::Solid(Color::rgba(1., 1., 1., 1.)), radii: [r; 4], border: None };
            if crate::native_gl::materials_active() {
                let _offscreen = crate::native_gl::offscreen(&mut target)?;
                target.canvas().scale((scale, scale));
                let _batch = crate::native_gl::batch_root(target.canvas());
                if let Some(mesh) = crate::qt_rect::tessellate(crate::geometry::Rect::new(0., 0., item.0, item.1), &rect) {
                    let matrix = target.canvas().local_to_device_as_3x3();
                    if crate::native_gl::rect(target.canvas(), std::rc::Rc::new(mesh), &matrix, 1., r > 0.)? {
                        return Ok(target.image_snapshot());
                    }
                }
                target.canvas().reset_matrix();
            }
            if let Some(raster) = crate::qt_rect::rasterize(crate::geometry::Rect::new(0., 0., item.0, item.1), &rect, &sk::Matrix::scale((scale, scale)), 1.) {
                target.canvas().draw_image(&raster.image, (raster.bounds.left as f32, raster.bounds.top as f32), None);
            }
            Ok(target.image_snapshot())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weights_and_padding_follow_qt() {
        let e = MultiEffect {
            shadow_enabled: true,
            ..Default::default()
        };
        assert_eq!(e.padding(), 32.);
        assert_eq!(e.levels(), 4);
        let w = e.weights(10. / 64.);
        assert!((w.iter().sum::<f32>() - 1.).abs() < 1e-6);
        assert!(w[0] > 0. && w[1] > 0.);
    }
    #[test]
    fn shaders_compile_and_preserve_disabled_source() {
        let programs = Programs::new();
        let mut source = sk::surfaces::raster_n32_premul((20, 20)).unwrap();
        source.canvas().clear(sk::Color::RED);
        let image = source.image_snapshot();
        let out = programs
            .apply(
                source.canvas(),
                image,
                (20., 20.),
                1.,
                &MultiEffect::default(),
            )
            .unwrap();
        let mut dest = sk::surfaces::raster_n32_premul((20, 20)).unwrap();
        dest.canvas().draw_image(out, (0., 0.), None);
        assert_eq!(
            dest.peek_pixels().unwrap().get_color((10, 10)),
            sk::Color::RED
        );
    }
}
