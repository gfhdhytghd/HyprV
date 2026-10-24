//! Shared CPU/GPU scene painting. Text shaping is cached across frames.
#[path="qt_batch.rs"]
mod qt_batch;
use crate::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Fill, Node, NodeId, Scene},
};
use skia_safe::{
    self as sk, canvas::SaveLayerRec, textlayout::FontCollection, Canvas, Color4f, Paint,
    RRect, Vector,
};
use std::{collections::HashMap, path::Path};
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("invalid or excessive image extent")]
    InvalidSize,
    #[error("font: {0}")]
    Font(String),
    #[error("native GL text: {0}")]
    NativeGl(String),
    #[error("offscreen backend: {0}")]
    Offscreen(String),
    #[error("Skia allocation/encoding failed")]
    Skia,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub(crate) fn skrect(r: Rect) -> sk::Rect {
    sk::Rect::from_xywh(r.x, r.y, r.width, r.height)
}
pub(crate) fn rounded_rect(r: Rect, radii: [f32; 4]) -> RRect {
    let radii = radii.map(|r| Vector::new(r.max(0.), r.max(0.)));
    RRect::new_rect_radii(skrect(r), &radii)
}
fn color(c: Color) -> Color4f {
    Color4f::new(c.r, c.g, c.b, c.a)
}
thread_local! {
    static GPU_EXACT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SRC_OVER: sk::Blender = sk::RuntimeEffect::make_for_blender(
        // the GL fragment output (src * opacity) is stored to 8 bits before the fixed-function
        // blend; the blend result is then rounded once more on the RGBA8 store
        "vec4 main(vec4 src, vec4 dst) { vec4 s = floor(src * 255.0 + 0.5) / 255.0; return s + dst * (1.0 - s.a); }",
        None,
    )
    .expect("src-over blender")
    .make_blender(sk::Data::new_empty(), None)
    .expect("src-over blender");
}
/// GPU-exact blending on CPU canvases (ENG-BLEND-1): a runtime src-over blender forces
/// skia's highp pipeline, i.e. float blend and one round-to-nearest 8-bit store per draw,
/// as the GL RGBA8 framebuffer does. Only src-over paints are changed; GPU canvases and
/// `Renderer::set_gpu_exact_blending(false)` keep skia's native blending.
pub(crate) fn exact(mut p: Paint) -> Paint {
    if GPU_EXACT.with(|g| g.get()) && p.as_blend_mode() == Some(sk::BlendMode::SrcOver) {
        SRC_OVER.with(|b| p.set_blender(b.clone()));
    }
    p
}
/// [`exact`] for paints that only modulate by their alpha (layers, images, pictures):
/// skia's GL path hands that opacity to the shader as an 8-bit colour alpha.
pub(crate) fn exact_alpha(mut p: Paint) -> Paint {
    if GPU_EXACT.with(|g| g.get()) && p.as_blend_mode() == Some(sk::BlendMode::SrcOver) {
        p.set_alpha_f((p.alpha_f() * 255.).round() / 255.);
    }
    exact(p)
}
/// Whether the current draw targets a CPU canvas in GPU-exact mode.
pub(crate) fn exact_enabled() -> bool {
    GPU_EXACT.with(|g| g.get())
}
/// `exact(Paint::default())` for draws that would pass no paint.
pub(crate) fn exact_default() -> Paint {
    exact(Paint::default())
}
struct ExactGuard(bool);
impl ExactGuard {
    fn new(on: bool) -> Self {
        Self(GPU_EXACT.with(|g| g.replace(on)))
    }
}
impl Drop for ExactGuard {
    fn drop(&mut self) {
        GPU_EXACT.with(|g| g.set(self.0));
    }
}
fn paint(fill: &Fill) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    match fill {
        Fill::Solid(c) => {
            p.set_color4f(color(*c), None);
        }
        Fill::LinearGradient { start, end, stops } => {
            let colors: Vec<_> = stops.iter().map(|s| color(s.1).to_color()).collect();
            let positions: Vec<_> = stops.iter().map(|s| s.0).collect();
            p.set_shader(sk::gradient_shader::linear(
                (*start, *end),
                colors.as_slice(),
                positions.as_slice(),
                sk::TileMode::Clamp,
                None,
                None,
            ));
        }
    };
    exact(p)
}
// Four independent CPU raster caches share an 8 MiB residency envelope.
// Visible images/targets are separately owned; oversize results render uncached.
pub(crate) const RASTER_CACHE_SHARE: usize = 2 * 1024 * 1024;
struct CachedEffect {
    revision: u64,
    scale: f32,
    gpu: bool,
    image: sk::Image,
    native: crate::effects::NativeEffect,
    native_materials:bool,
}
pub struct Renderer {
    offscreen_backend: Option<crate::offscreen::Backend>,
    offscreen_target: Option<u64>,
    fonts: FontCollection,
    native_text: Option<crate::text::TextEngine>,
    effects: Option<crate::effects::Programs>,
    effect_cache: HashMap<NodeId, CachedEffect>,
    scale: f32,
    rect_mode: crate::qt_rect::RectMode,
    rect_cache: crate::qt_rect::Cache,
    bounds_cache: HashMap<NodeId, (u64, Option<Rect>)>,
    pruned: Option<(NodeId, u64)>,
    live: std::collections::HashSet<NodeId>,
    native_gl_scene: Option<(NodeId, u64, bool)>,
    /// CPU canvases blend like the GL framebuffer (see [`exact`]); default on.
    gpu_exact_blending: bool,
    native_cache_epoch: std::rc::Rc<std::cell::Cell<u64>>,
}
impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
impl Renderer {
    pub fn new() -> Self {
        let mut fonts = FontCollection::new();
        fonts.set_default_font_manager(crate::text::font_manager(), None);
        Self {
            offscreen_backend: None,
            offscreen_target: None,
            fonts,
            native_text: None,
            effects: None,
            effect_cache: HashMap::new(),
            scale: 1.,
            rect_mode: crate::qt_rect::RectMode::QtQuick,
            rect_cache: crate::qt_rect::Cache::default(),
            bounds_cache: HashMap::new(),
            pruned: None,
            live: Default::default(),
            native_gl_scene: None,
            gpu_exact_blending: true,
            native_cache_epoch: Default::default(),
        }
    }
    /// Per-draw float blending with an 8-bit store on CPU canvases, like the GL path
    /// (default on). Off = skia's native raster blending.
    pub fn set_gpu_exact_blending(&mut self, on: bool) {
        if self.gpu_exact_blending != on {
            self.gpu_exact_blending = on;
            self.effect_cache.clear();
        }
    }
    /// Release renderer-owned caches and invalidate native caches for the next draw.
    /// For immediate EGL-owned resource release between frames, use
    /// `SharedEgl::clear_caches` (which makes its context current).
    pub fn clear_caches(&mut self) {
        self.native_cache_epoch.set(self.native_cache_epoch.get().wrapping_add(1));
        self.native_gl_scene = None;
        self.effect_cache.clear();
        self.bounds_cache.clear();
        self.pruned = None;
        self.live.clear();
        self.rect_cache.clear();
        crate::image::clear_caches();
        crate::canvas::clear_cache();
        crate::text::clear_measurement_caches();

        if let Some(text) = self.native_text.as_mut() {
            text.clear();
        }
    }
    pub(crate) fn sync_native_caches(&self, context: &mut crate::native_gl::Context) -> bool {
        context.sync_caches(&self.native_cache_epoch)
    }
    pub(crate) fn native_gl_required(&mut self, scene: &Scene) -> bool {
        let root = scene.root();
        if let Some((id, revision, enabled)) = self.native_gl_scene {
            if id == root.id() && revision == scene.revision() {
                return enabled;
            }
        }
        fn uses_gl(node: &Node) -> bool {
            if !node.visible { return false; }
            node.native_materials==Some(true) || (node.native_text_mode == crate::scene::NativeTextMode::QtGl
                && matches!(&node.content, Content::Text(t) if t.render_type == crate::scene::RenderType::NativeRendering))
                || matches!(&node.content, Content::Shared(shared) if uses_gl(shared))
                || node.children.iter().any(uses_gl)
        }
        let enabled = uses_gl(root);
        self.native_gl_scene = Some((root.id(), scene.revision(), enabled));
        enabled
    }
    /// QtQuick is the default. Unsupported geometry falls back to Skia; select
    /// Skia explicitly for application-specific curves/gradients. Changing this
    /// invalidates retained effects; repaint the complete scene afterwards.
    pub fn set_rect_mode(&mut self, mode: crate::qt_rect::RectMode) {
        if self.rect_mode != mode {
            self.rect_mode = mode;
            self.clear_caches();
        }
    }
    pub fn rect_mode(&self) -> crate::qt_rect::RectMode {
        self.rect_mode
    }
    fn qt_rectangle(
        &mut self,
        c: &Canvas,
        n: &Node,
        r: &crate::scene::Rectangle,
        alpha: f32,
    ) -> bool {
        // GPU: cached Qt vertex-AA mesh drawn as triangles; raster: cached per
        // sub-pixel phase (qt_rect::Cache, PERF-UI-5).
        let _merge=crate::native_gl::rect_merge_scope(n.id());
        let antialias = n.rect_antialias.unwrap_or(r.radii[0] > 0.);
        self.rect_cache.draw_with_size(c, n.local_bounds(), r, alpha, antialias,n.qt_rect_size)
    }
    pub fn font_collection(&mut self) -> &mut FontCollection {
        &mut self.fonts
    }
    /// Paints only the damage clip; the target must retain undamaged pixels.
    pub fn paint(
        &mut self,
        canvas: &Canvas,
        scene: &Scene,
        scale: f64,
        damage: Rect,
    ) -> Result<(), RenderError> {
        let _timing = crate::diagnostics::scope("renderer.paint");
        if crate::native_gl::sync_caches(canvas, &self.native_cache_epoch) {
            if let Some(mut context) = canvas.direct_context() { context.reset(None); }
        }
        self.prune(scene);
        self.scale = scale as f32;
        let _restore = sk::AutoCanvasRestore::guard(canvas, true);
        canvas.scale((scale as f32, scale as f32));
        let _batch=crate::native_gl::batch_root(canvas);
        canvas.clip_rect(skrect(damage), sk::ClipOp::Intersect, false);
        if !crate::native_gl::clear_damage(canvas) {canvas.clear(sk::Color::TRANSPARENT);}
        let _exact = ExactGuard::new(self.gpu_exact_blending && canvas.recording_context().is_none());
        self.node(canvas, scene.root(), 1.)
    }
    /// Drops cached state of nodes that left the scene. Runs only when the scene changed
    /// since the last prune, as one traversal (PERF-UI-3).
    fn prune(&mut self, scene: &Scene) {
        let key = (scene.root().id(), scene.revision());
        if self.pruned == Some(key) {
            return;
        }
        self.pruned = Some(key);
        let mut live = std::mem::take(&mut self.live);
        live.clear();
        fn walk(n: &Node, live: &mut std::collections::HashSet<NodeId>) {
            if !live.insert(n.id()) { return; }
            if let Content::Shared(shared) = &n.content { walk(shared, live); }
            for c in &n.children {
                walk(c, live);
            }
        }
        if !self.effect_cache.is_empty() || self.bounds_cache.len() > 4096 {
            walk(scene.root(), &mut live);
            self.effect_cache.retain(|id, _| live.contains(id));
            if self.bounds_cache.len() > 2 * live.len() + 4096 {
                self.bounds_cache.retain(|id, _| live.contains(id));
            }
        }
        self.live = live;
    }
    /// Called at a surface lifetime boundary, never on an idle timer. Preserve
    /// caches referenced by every remaining scene, including shared subtrees.
    pub(crate) fn retain_scenes<'a>(&mut self, scenes: impl Iterator<Item = &'a Scene>) {
        fn walk(node: &Node, live: &mut std::collections::HashSet<NodeId>) {
            if !live.insert(node.id()) { return; }
            if let Content::Shared(shared) = &node.content { walk(shared, live); }
            for child in &node.children { walk(child, live); }
        }
        let mut live = std::collections::HashSet::new();
        for scene in scenes { walk(scene.root(), &mut live); }
        self.effect_cache.retain(|id, _| live.contains(id));
        self.bounds_cache.retain(|id, _| live.contains(id));
        self.pruned = None;
        self.native_gl_scene = None;
    }
    /// Paint extent of `n` and its subtree in `n`'s local space, cached per node
    /// revision (edits bump the revision of the node and all ancestors). `None` for
    /// content whose extent is unknown; such subtrees are never culled.
    fn subtree_bounds(&mut self, n: &Node) -> Option<Rect> {
        if let Some((revision, bounds)) = self.bounds_cache.get(&n.id()) {
            if *revision == n.revision() {
                return *bounds;
            }
        }
        let mut bounds = match &n.content {
            Content::Group
            | Content::Rect(_)
            | Content::Image(_)
            | Content::Svg(_)
            | Content::Picture(_)
            | Content::Path { .. }
            | Content::StrokedPath { .. } => Some(n.paint_bounds()),
            Content::Text(t) => {
                let _native_mode_block = (n.native_text_mode != crate::scene::NativeTextMode::QtGl).then(crate::native_gl::block);
                if self.native_text.is_none() {
                    self.native_text = crate::text::TextEngine::new().ok();
                }
                self.native_text
                    .as_mut()
                    .and_then(|e| e.extent(t, n.frame.width).ok())
                    .map(|e| e.union(n.local_bounds()))
            }
            _ => None,
        };
        for child in &n.children {
            let child_bounds = self.subtree_bounds(child);
            bounds = match (bounds, child_bounds) {
                (Some(b), Some(cb)) => {
                    let (r, _) = child.local_matrix().map_rect(skrect(cb));
                    Some(b.union(Rect::new(r.x(), r.y(), r.width(), r.height())))
                }
                _ => None,
            };
        }
        if n.clip.is_some() {
            // Clipped content stays inside the frame; effects still extend past it.
            let l = n.local_bounds().outset(1.);
            bounds = Some(match bounds {
                Some(b) => {
                    let x0 = b.x.max(l.x);
                    let y0 = b.y.max(l.y);
                    let x1 = (b.x + b.width).min(l.x + l.width);
                    let y1 = (b.y + b.height).min(l.y + l.height);
                    Rect::new(x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
                }
                None => l,
            });
        }
        let bounds = bounds.map(|b| b.outset(n.effects_outset() + 1.));
        self.bounds_cache.insert(n.id(), (n.revision(), bounds));
        bounds
    }
    fn node(&mut self, c: &Canvas, n: &Node, inherited: f32) -> Result<(), RenderError> {
        if !n.visible || n.opacity <= 0. {
            return Ok(());
        }
        crate::diagnostics::count("render.visited_nodes");
        let _materials=crate::native_gl::materials(n.native_materials);
        let _restore = sk::AutoCanvasRestore::guard(c, true);
        let local_matrix=n.local_matrix();
        let _relative=crate::native_gl::node_transform(c,&local_matrix);
        c.concat(&local_matrix);
        // Damage culling (PERF-UI-2): skip subtrees entirely outside the clip.
        if let Some(bounds) = self.subtree_bounds(n) {
            if sk::QuickReject::quick_reject(c, &skrect(bounds)) {
                return Ok(());
            }
        }
        crate::diagnostics::count("render.painted_nodes");
        let mut layer_paint = Paint::default();
        let alpha = inherited * n.opacity.clamp(0., 1.);
        layer_paint.set_alpha_f(alpha);
        let mut filter = n
            .effects
            .tint
            .and_then(|t| sk::color_filters::blend(color(t).to_color(), sk::BlendMode::SrcIn))
            .and_then(|f| sk::image_filters::color_filter(f, None, None));
        if n.effects.blur_sigma > 0. {
            filter = sk::image_filters::blur(
                (n.effects.blur_sigma, n.effects.blur_sigma),
                None,
                None,
                None,
            );
        }
        if let Some(s) = &n.effects.shadow {
            filter = sk::image_filters::drop_shadow(
                s.offset,
                (s.sigma, s.sigma),
                color(s.color),
                None,
                filter,
                None,
            );
        }
        let native_materials=crate::native_gl::materials_active();
        let matrix=c.local_to_device_as_3x3();
        let direct_effect=n.effects.multi_effect.is_some() && filter.is_none()
            // GPU-exact CPU canvases draw the composed effect with the node alpha in one
            // blend, as the native GL material does (ENG-BLEND-1)
            && (crate::native_gl::material_target(c) || (crate::native_gl::materials_enabled() && c.recording_context().is_none() && exact_enabled()))
            && !matrix.has_perspective()
            && matrix.skew_x()==0. && matrix.skew_y()==0. && matrix.scale_x()>0. && matrix.scale_y()>0.;
        // A Group-opacity layer around a single childless rectangle equals per-node
        // opacity; the native Qt rectangle material applies it per vertex, so GL native
        // targets draw it directly (the raster path keeps the layer).
        let single_rect = n.opacity_mode == crate::scene::OpacityMode::Group
            && n.children.is_empty() && matches!(n.content, Content::Rect(_))
            && filter.is_none() && n.effects.multi_effect.is_none()
            && crate::native_gl::material_target(c);
        let layer = !direct_effect && !single_rect && (filter.is_some()
            || n.effects.multi_effect.is_some()
            || n.opacity_mode == crate::scene::OpacityMode::Group);
        layer_paint.set_image_filter(filter);
        let layer_paint = exact_alpha(layer_paint);
        if layer {
            c.save_layer(&SaveLayerRec::default().paint(&layer_paint));
        }
        let _native_gl_block = layer.then(crate::native_gl::block);
        if let Some(effect) = &n.effects.multi_effect {
            let _timing = crate::diagnostics::scope("effect.paint");
            let padding = effect.padding();
            let logical = (n.frame.width + padding * 2., n.frame.height + padding * 2.);
            let gpu = c.recording_context().is_some();
            let cached = self
                .effect_cache
                .get(&n.id())
                .filter(|cache| {
                    cache.revision == n.revision() && cache.scale == self.scale && cache.gpu == gpu && cache.native_materials==native_materials
                })
                .map(|cache| (cache.image.clone(),cache.native.clone()));
            let (image,native) = if let Some(image) = cached {
                image
            } else {
                let size = if let Some((w, h)) = effect.source_texture_size {
                    physical_size(w as f32, h as f32, 1.)?
                } else {
                    physical_size(logical.0, logical.1, self.scale as f64)?
                };
                let mut source = crate::effects::surface(c,size.0,size.1)?;
                source.canvas().clear(sk::Color::TRANSPARENT);
                let source_scale = if effect.source_texture_size.is_some() {
                    (size.0 as f32 / logical.0, size.1 as f32 / logical.1)
                } else {
                    // Preserve the existing fractional-extent capture transform.
                    (self.scale, self.scale)
                };
                source.canvas().scale(source_scale);
                source.canvas().translate((padding, padding));
                {
                    let _offscreen = crate::native_gl::offscreen(&mut source)?;
                    if effect.source_texture_size.is_some() {
                        crate::native_gl::set_layer_rect_projection(source.canvas(), logical, padding);
                    }
                    let _batch=crate::native_gl::batch_root(source.canvas());
                    self.body(source.canvas(), n, 1.)?;
                }
                let programs = self
                    .effects
                    .get_or_insert_with(crate::effects::Programs::new);
                let (image,native) =
                    programs.apply_prepared(c, source.image_snapshot(), logical, self.scale, effect)?;
                self.effect_cache.insert(
                    n.id(),
                    CachedEffect {
                        revision: n.revision(),
                        scale: self.scale,
                        gpu,
                        image: image.clone(),
                        native: native.clone(),
                        native_materials,
                    },
                );
                (image,native)
            };
            if direct_effect && crate::native_gl::effect_at(c,&native,Rect::new(-padding,-padding,logical.0,logical.1),alpha)? {return Ok(())}
            c.scale((1. / self.scale, 1. / self.scale));
            let plain = exact_default();
            c.draw_image(image, (-padding * self.scale, -padding * self.scale), Some(if direct_effect { &layer_paint } else { &plain }));
            Ok(())
        } else {
            self.body(c, n, if layer { 1. } else { alpha })
        }
    }
    fn body(&mut self, c: &Canvas, n: &Node, alpha: f32) -> Result<(), RenderError> {
        let _restore = sk::AutoCanvasRestore::guard(c, true);
        let mut _native_clip_block = None;
        let mut _batch_root=None;
        if n.native_batch_root && n.clip.is_none() {
            _batch_root=Some(crate::native_gl::batch_root(c));
            let matrix=c.local_to_device_as_3x3();
            crate::native_gl::set_unmerged_rects(qt_batch::unmerged_rects(n,alpha,matrix.scale_x()!=self.scale || matrix.scale_y()!=self.scale));
        }
        if let Some(radii) = n.clip {
            let matrix = c.local_to_device_as_3x3();
            if radii == [0.; 4]
                && n.rect_mode.unwrap_or(self.rect_mode) == crate::qt_rect::RectMode::QtQuick
                && !matrix.has_perspective()
                && matrix.skew_x() == 0.
                && matrix.skew_y() == 0.
            {
                let device = crate::native_gl::rectangular_clip(c,skrect(n.local_bounds())).unwrap_or_else(|| {
                    let device=matrix.map_rect(skrect(n.local_bounds())).0;
                    sk::Rect::new(device.left.round(),device.top.round(),device.right.round(),device.bottom.round())
                });
                c.reset_matrix();
                c.clip_rect(device, sk::ClipOp::Intersect, false);
                c.concat(&matrix);
                _batch_root=Some(crate::native_gl::batch_root(c));
                crate::native_gl::set_unmerged_rects(qt_batch::unmerged_rects(n, alpha, matrix.scale_x() != self.scale || matrix.scale_y() != self.scale));
            } else {
                // A rectangular AA clip can still report is_clip_rect(). Its
                // fringe coverage cannot be represented by a GL scissor.
                _native_clip_block = Some(crate::native_gl::block());
                c.clip_rrect(
                    rounded_rect(n.local_bounds(), radii),
                    sk::ClipOp::Intersect,
                    true,
                );
            }
        }
        let paint = |fill: &Fill| {
            let mut p = paint(fill);
            p.set_alpha_f(p.alpha_f() * alpha);
            p
        };
        match &n.content {
            Content::Group => {}
            Content::Canvas(paint) => {
                let width=(n.frame.width.round() as f64*self.scale as f64).round().max(1.) as u32;
                let height=(n.frame.height.round() as f64*self.scale as f64).round().max(1.) as u32;
                if let Some(image)=paint.raster(width,height,self.scale as f64) {
                    if crate::native_gl::sampled_image(c,&image,n.local_bounds(),crate::image::ImageSampling::Linear,alpha)? {
                        // QSG Canvas textures use the same float-opacity image material.
                    } else if !crate::image::draw(c,&image,n.local_bounds(),crate::image::ImageSampling::Linear,alpha) {
                        let mut p=Paint::default();p.set_alpha_f(alpha);let p=exact_alpha(p);
                        c.draw_image_rect_with_sampling_options(&image,None,skrect(n.local_bounds()),sk::SamplingOptions::from(sk::FilterMode::Linear),&p);
                    }
                }
            }
            Content::Shared(node) => {let _scope=crate::native_gl::materials(Some(false));self.node(c, node, alpha)?;}
            Content::Picture(picture) => {
                let mut p = Paint::default();
                p.set_alpha_f(alpha);
                c.draw_picture(picture, None, Some(&exact_alpha(p)));
            }
            Content::StrokedPath { path, fill, stroke } => {
                c.draw_path(path, &paint(fill));
                let mut p = paint(&Fill::Solid(stroke.color));
                p.set_style(sk::paint::Style::Stroke)
                    .set_stroke_width(stroke.width)
                    .set_stroke_cap(stroke.cap)
                    .set_stroke_join(stroke.join);
                c.draw_path(path, &p);
            }
            Content::Rect(r) => {
                if n.rect_mode.unwrap_or(self.rect_mode) == crate::qt_rect::RectMode::QtQuick
                    && self.qt_rectangle(c, n, r, alpha)
                {
                    for child in &n.children {
                        self.node(c, child, alpha)?;
                    }
                    return Ok(());
                }
                let rr = rounded_rect(n.local_bounds(), r.radii);
                if let Some(border) = r.border.as_ref().filter(|b| b.width > 0.) {
                    let width = border
                        .width
                        .min(n.frame.width / 2.)
                        .min(n.frame.height / 2.);
                    let mut inner = rr;
                    inner.inset((width, width));
                    c.draw_rrect(inner, &paint(&r.fill));
                    let mut ring = sk::Path::new();
                    ring.set_fill_type(sk::PathFillType::EvenOdd);
                    ring.add_rrect(rr, None);
                    ring.add_rrect(inner, None);
                    c.draw_path(&ring, &paint(&Fill::Solid(border.color)));
                } else {
                    c.draw_rrect(rr, &paint(&r.fill));
                }
            }
            Content::Text(t) => {
                let _native_mode_block = (n.native_text_mode != crate::scene::NativeTextMode::QtGl).then(crate::native_gl::block);
                if self.native_text.is_none() {
                    self.native_text = Some(crate::text::TextEngine::new()?);
                }
                self.native_text
                    .as_mut()
                    .unwrap()
                    .paint_for_output(c, t, n.frame.width, alpha, Some(self.scale as f64))?;
            }
            Content::Image(image) => {
                if crate::native_gl::sampled_image(c,image,n.local_bounds(),n.image_sampling,alpha)? {
                    for child in &n.children {self.node(c,child,alpha)?;}
                    return Ok(())
                }
                if crate::image::draw(c, image, n.local_bounds(), n.image_sampling, alpha) {
                    for child in &n.children {
                        self.node(c, child, alpha)?;
                    }
                    return Ok(());
                }
                let mut p = Paint::default();
                p.set_alpha_f(alpha);
                let p = exact_alpha(p);
                let mipmapped = if n.image_sampling == crate::image::ImageSampling::Mipmap {
                    Some(crate::image::mipmapped(image))
                } else {
                    None
                };
                c.draw_image_rect_with_sampling_options(
                    mipmapped.as_ref().unwrap_or(image),
                    None,
                    skrect(n.local_bounds()),
                    n.image_sampling.skia(),
                    &p,
                );
            }
            Content::Svg(dom) => {
                let mut dom = dom.clone();
                dom.set_container_size((n.frame.width, n.frame.height));
                if alpha < 1. {
                    let mut p = Paint::default();
                    p.set_alpha_f(alpha);
                    c.save_layer(&SaveLayerRec::default().paint(&exact_alpha(p)));
                }
                dom.render(c);
                if alpha < 1. {
                    c.restore();
                }
            }
            Content::Path { path, fill, stroke } => {
                c.draw_path(path, &paint(fill));
                if let Some(b) = stroke {
                    let mut p = paint(&Fill::Solid(b.color));
                    p.set_style(sk::paint::Style::Stroke)
                        .set_stroke_width(b.width);
                    c.draw_path(path, &p);
                }
            }
        }
        for child in &n.children {
            self.node(c, child, alpha)?;
        }
        Ok(())
    }
    pub(crate) fn sync_offscreen_target(&mut self, id: u64) {
        if self.offscreen_target.is_some_and(|previous| previous != id) {
            self.clear_caches();
        }
        self.offscreen_target = Some(id);
    }
    /// Override `HYPRV_PARITY_BACKEND` for this renderer's offscreen calls.
    /// `paint` and live EGL surfaces always use their supplied destination.
    pub fn set_offscreen_backend(&mut self, backend: crate::offscreen::Backend) {
        self.offscreen_backend = Some(backend);
    }
    pub fn offscreen_backend(&self) -> Result<crate::offscreen::Backend, RenderError> {
        self.offscreen_backend.map_or_else(crate::offscreen::Backend::from_env, Ok)
    }
    /// Offscreen image using the same PNG transport as the original parity helper.
    pub fn image(&mut self, scene: &Scene, width: f32, height: f32, scale: f64) -> Result<sk::Image, RenderError> {
        if let Some(image) = crate::offscreen::render_image_if_enabled(self, scene, width, height, scale)
            .map_err(|e| RenderError::Offscreen(e.to_string()))? { return Ok(image); }
        Ok(self.raster_cpu(scene, width, height, scale)?.image_snapshot())
    }
    pub fn raster(
        &mut self,
        scene: &Scene,
        width: f32,
        height: f32,
        scale: f64,
    ) -> Result<sk::Surface, RenderError> {
        if self.offscreen_backend()? == crate::offscreen::Backend::Gl {
            return crate::offscreen::raster(self, scene, width, height, scale);
        }
        self.raster_cpu(scene, width, height, scale)
    }
    fn raster_cpu(&mut self, scene: &Scene, width: f32, height: f32, scale: f64) -> Result<sk::Surface, RenderError> {
        let (w, h) = physical_size(width, height, scale)?;
        let mut surface = sk::surfaces::raster_n32_premul((w, h)).ok_or(RenderError::Skia)?;
        self.paint(
            surface.canvas(),
            scene,
            scale,
            Rect::new(0., 0., width, height),
        )?;
        Ok(surface)
    }
    pub fn png(
        &mut self,
        scene: &Scene,
        width: f32,
        height: f32,
        scale: f64,
        path: impl AsRef<Path>,
    ) -> Result<(), RenderError> {
        if let Some(bytes) = crate::offscreen::render_png_if_enabled(self, scene, width, height, scale)
            .map_err(|e| RenderError::Offscreen(e.to_string()))? {
            std::fs::write(path, bytes)?;
            return Ok(());
        }
        let mut surface = self.raster(scene, width, height, scale)?;
        let data = surface
            .image_snapshot()
            .encode(None, sk::EncodedImageFormat::PNG, None)
            .ok_or(RenderError::Skia)?;
        std::fs::write(path, data.as_bytes())?;
        Ok(())
    }
}
pub fn physical_size(w: f32, h: f32, scale: f64) -> Result<(i32, i32), RenderError> {
    if !w.is_finite() || !h.is_finite() || !scale.is_finite() || w <= 0. || h <= 0. || scale <= 0. {
        return Err(RenderError::InvalidSize);
    }
    let w = (w as f64 * scale).ceil();
    let h = (h as f64 * scale).ceil();
    if w > 32768. || h > 32768. || w * h > 268435456. {
        return Err(RenderError::InvalidSize);
    }
    Ok((w as i32, h as i32))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::*;
    #[test]
    fn raster_scale_and_alpha() {
        let root = Node::new(
            Rect::new(0., 0., 10., 10.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(1., 0., 0., 0.5)),
                ..Default::default()
            }),
        );
        let mut renderer = Renderer::new();
        let mut surface = renderer.raster(&Scene::new(root), 10., 10., 1.25).unwrap();
        assert_eq!(surface.width(), 13);
        assert_eq!(surface.height(), 13);
        let pixels = surface.peek_pixels().unwrap();
        let c = pixels.get_color((5, 5));
        assert!((c.a() as i32 - 128).abs() <= 1);
        assert_eq!(c.r(), 255);
    }
}

#[cfg(test)]
mod effect_tests {
    use super::*;
    use crate::{effects::MultiEffect, scene::*};
    #[test]
    fn normalized_shadow_has_padded_extent() {
        let mut root = Node::new(
            Rect::new(40., 40., 20., 20.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
                ..Default::default()
            }),
        );
        root.effects.multi_effect = Some(MultiEffect {
            shadow_enabled: true,
            shadow_blur: 10. / 64.,
            ..Default::default()
        });
        let mut surface = Renderer::new()
            .raster(&Scene::new(root), 100., 100., 1.)
            .unwrap();
        let pixels = surface.peek_pixels().unwrap();
        assert_eq!(pixels.get_color((50, 50)), sk::Color::RED);
        assert!(
            pixels.get_color((61, 50)).a() > 0,
            "shadow must extend outside the source"
        );
        assert_eq!(pixels.get_color((0, 0)).a(), 0);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::{effects::MultiEffect, scene::*};
    #[test]
    fn effect_cache_survives_moves_and_invalidates_on_descendant_edits() {
        let mut root = Node::new(Rect::new(0., 0., 60., 60.), Content::Group);
        root.effects.multi_effect = Some(MultiEffect::default());
        let parent = root.id();
        let mut child = Node::new(
            Rect::new(10., 10., 20., 20.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
                ..Default::default()
            }),
        );
        child.effects.multi_effect = Some(MultiEffect::default());
        let id = root.add(child);
        let mut scene = Scene::new(root);
        let mut renderer = Renderer::new();
        renderer.raster(&scene, 100., 100., 1.).unwrap();
        let cached = renderer.effect_cache[&id].image.unique_id();
        scene.set_position(id, 20., 10.);
        let mut surface = renderer.raster(&scene, 100., 100., 1.).unwrap();
        assert_eq!(renderer.effect_cache[&id].image.unique_id(), cached);
        assert_eq!(
            surface.peek_pixels().unwrap().get_color((25, 15)),
            sk::Color::RED
        );
        assert_eq!(surface.peek_pixels().unwrap().get_color((15, 15)).a(), 0);
        let old_parent = renderer.effect_cache[&parent].image.unique_id();
        scene.edit(id, |n| {
            if let Content::Rect(r) = &mut n.content {
                r.fill = Fill::Solid(Color::rgba(0., 0., 1., 1.));
            }
        });
        let mut surface = renderer.raster(&scene, 100., 100., 1.).unwrap();
        assert_ne!(renderer.effect_cache[&parent].image.unique_id(), old_parent);
        assert_eq!(
            surface.peek_pixels().unwrap().get_color((25, 15)),
            sk::Color::BLUE
        );
    }
}

#[cfg(test)]
mod qt_item_semantics {
    use super::*;
    use crate::scene::*;
    #[test]
    fn border_replaces_fill_and_item_opacity_is_unisolated() {
        let mut root = Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(1., 0., 0., 0.8)),
                border: Some(Border {
                    width: 2.,
                    color: Color::rgba(0., 0., 1., 0.1),
                }),
                radii: [0.; 4],
            }),
        );
        let mut r = Renderer::new();
        let mut image = r.raster(&Scene::new(root), 20., 20., 1.).unwrap();
        let px = image.peek_pixels().unwrap();
        assert!((px.get_color((0, 10)).a() as i32 - 26).abs() <= 1);
        assert_eq!(px.get_color((10, 10)).a(), 204);
        root = Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(1., 0., 0., 1.)),
                ..Default::default()
            }),
        );
        root.opacity = 0.5;
        root.children.push(Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Rect(Rectangle {
                fill: Fill::Solid(Color::rgba(0., 0., 1., 1.)),
                ..Default::default()
            }),
        ));
        let id = root.id();
        let mut s = Scene::new(root);
        let mut image = r.raster(&s, 20., 20., 1.).unwrap();
        assert!((image.peek_pixels().unwrap().get_color((10, 10)).a() as i32 - 192).abs() <= 1);
        s.edit(id, |n| n.opacity_mode = OpacityMode::Group);
        let mut image = r.raster(&s, 20., 20., 1.).unwrap();
        assert_eq!(image.peek_pixels().unwrap().get_color((10, 10)).a(), 128);
    }
}
#[cfg(test)]
mod native_mode_tests {
    use super::*;
    use crate::scene::{NativeTextMode, RenderType, Text};
    #[test]
    fn material_only_surfaces_select_context_without_text() {
        let node=Node::new(Rect::new(0.,0.,27.,27.),Content::Rect(Default::default()));
        let id=node.id();let mut scene=Scene::new(node);let mut renderer=Renderer::new();
        assert!(!renderer.native_gl_required(&scene));
        scene.edit(id,|n|n.native_materials=Some(true));
        assert!(renderer.native_gl_required(&scene));
        scene.edit(id,|n|n.visible=false);
        assert!(!renderer.native_gl_required(&scene));
        scene.edit(id,|n|{n.visible=true;n.native_materials=Some(false)});
        assert!(!renderer.native_gl_required(&scene));
    }
    #[test]
    fn native_backend_selection_tracks_retained_edits_and_shared_subtrees() {
        let mut leaf = Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Text(Text {
                render_type: RenderType::NativeRendering,
                ..Default::default()
            }),
        );
        let id = leaf.id();
        let mut scene = Scene::new(leaf);
        let mut renderer = Renderer::new();
        assert!(!renderer.native_gl_required(&scene));
        scene.edit(id, |n| n.native_text_mode = NativeTextMode::QtGl);
        assert!(renderer.native_gl_required(&scene));
        scene.edit(id, |n| n.visible = false);
        assert!(!renderer.native_gl_required(&scene));
        leaf = Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Text(Text {
                render_type: RenderType::NativeRendering,
                ..Default::default()
            }),
        );
        leaf.native_text_mode = NativeTextMode::QtGl;
        let shared = Scene::new(Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Shared(std::rc::Rc::new(leaf)),
        ));
        assert!(renderer.native_gl_required(&shared));
    }
}

#[cfg(test)]
mod review_r2_cache {
    use super::*;
    #[test]
    fn review_r2_shared_effect_survives_unrelated_edit() {
        let mut child = Node::new(
            Rect::new(0., 0., 20., 20.),
            Content::Rect(Default::default()),
        );
        child.effects.multi_effect = Some(Default::default());
        let id = child.id();
        let mut root = Node::new(
            Rect::new(0., 0., 60., 60.),
            Content::Shared(std::rc::Rc::new(child)),
        );
        let sibling = root.add(Node::new(Rect::new(40., 40., 5., 5.), Content::Group));
        let mut s = Scene::new(root);
        let mut r = Renderer::new();
        let mut surface = sk::surfaces::raster_n32_premul((60, 60)).unwrap();
        r.paint(surface.canvas(), &s, 1., Rect::new(0., 0., 60., 60.))
            .unwrap();
        assert!(r.effect_cache.contains_key(&id));
        s.set_position(sibling, 41., 41.);
        r.prune(&s);
        assert!(
            r.effect_cache.contains_key(&id),
            "live immutable Shared effect must not be pruned on sibling movement"
        );
    }
}

