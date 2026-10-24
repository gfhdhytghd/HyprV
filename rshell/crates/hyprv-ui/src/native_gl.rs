//! Qt's native A8 mask shader, interleaved with Skia on an explicitly registered
//! bottom-left EGL target. Unsupported clips/layers keep the Skia path.
use crate::{render::RenderError, skia as sk};
use glow::HasContext;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
pub(crate) fn shader_source(gl: &glow::Context, source: &str) -> String {
    if gl.version().is_embedded {
        source.to_owned()
    } else {
        format!(
            "#version 120\n{}",
            source
                .replace("#version 100", "")
                .replace("precision highp float;", "")
                .replace("precision highp int;", "")
                .replace("precision mediump float;", "")
                .replace("highp ", "")
                .replace("mediump ", "")
                .replace("lowp ", "")
        )
    }
}
thread_local! {
    static ACTIVE: RefCell<Option<Target>> = const {RefCell::new(None)};
    static BLOCKED: Cell<usize> = const {Cell::new(0)};
    static MATERIALS: Cell<bool> = const {Cell::new(false)};
    static BATCH: RefCell<Option<sk::Matrix>> = const {RefCell::new(None)};
    static RELATIVE: RefCell<Option<(sk::Matrix, sk::Matrix)>> = const {RefCell::new(None)};
    static RECT_MERGE_ALLOWED: Cell<bool> = const {Cell::new(true)};
    static UNMERGED_RECTS: RefCell<Vec<crate::scene::NodeId>> = const {RefCell::new(Vec::new())};
}
pub(crate) struct BatchGuard(Option<sk::Matrix>, Vec<crate::scene::NodeId>, Option<(sk::Matrix, sk::Matrix)>);
impl Drop for BatchGuard {fn drop(&mut self){BATCH.with(|b|*b.borrow_mut()=self.0.take()); UNMERGED_RECTS.with(|v|*v.borrow_mut()=std::mem::take(&mut self.1)); RELATIVE.with(|v|*v.borrow_mut()=self.2.take());}}
pub(crate) fn batch_root(c:&sk::Canvas)->BatchGuard {
    BatchGuard(BATCH.with(|b|b.replace(Some(c.local_to_device_as_3x3()))), UNMERGED_RECTS.with(|v|v.replace(Vec::new())), RELATIVE.with(|v|v.replace(Some((c.local_to_device_as_3x3(),sk::Matrix::new_identity())))))
}
pub(crate) struct TransformGuard(Option<(sk::Matrix, sk::Matrix)>);
impl Drop for TransformGuard { fn drop(&mut self) { RELATIVE.with(|v|*v.borrow_mut()=self.0.take()); } }
pub(crate) fn node_transform(c: &sk::Canvas, local: &sk::Matrix) -> TransformGuard {
    TransformGuard(RELATIVE.with(|v| {
        let mut v=v.borrow_mut();
        let next=v.as_ref().filter(|(absolute,_)|*absolute==c.local_to_device_as_3x3()).map(|(absolute,relative)| (sk::Matrix::concat(absolute,local),sk::Matrix::concat(relative,local)));
        std::mem::replace(&mut *v,next)
    }))
}
pub(crate) fn set_unmerged_rects(values: Vec<crate::scene::NodeId>) { UNMERGED_RECTS.with(|v|*v.borrow_mut()=values); }
pub(crate) struct RectMergeGuard(bool);
impl Drop for RectMergeGuard {fn drop(&mut self){RECT_MERGE_ALLOWED.with(|v|v.set(self.0));}}
pub(crate) fn rect_merge_scope(id:crate::scene::NodeId)->RectMergeGuard {
    let allowed=UNMERGED_RECTS.with(|v|!v.borrow().contains(&id));
    RectMergeGuard(RECT_MERGE_ALLOWED.with(|v|v.replace(allowed)))
}
fn image_geometry(m:sk::Matrix,bounds:crate::geometry::Rect)->(sk::Matrix,crate::geometry::Rect) {
    BATCH.with(|b|{
        let b=b.borrow();let Some(base)=b.as_ref()else{return(m,bounds)};
        if let Some(relative)=RELATIVE.with(|v|v.borrow().as_ref().filter(|(absolute,_)|*absolute==m).map(|(_,relative)|*relative)) {
            if !relative.has_perspective() && relative.skew_x()==0. && relative.skew_y()==0. && relative.scale_x()>0. && relative.scale_y()>0. {
                return (*base,crate::geometry::Rect::new(bounds.x*relative.scale_x()+relative.translate_x(),bounds.y*relative.scale_y()+relative.translate_y(),bounds.width*relative.scale_x(),bounds.height*relative.scale_y()));
            }
        }
        if !base.has_perspective() && !m.has_perspective() && base.skew_x()==0. && base.skew_y()==0. && m.skew_x()==0. && m.skew_y()==0.
            && base.scale_x()==m.scale_x() && base.scale_y()==m.scale_y() && base.scale_x()>0. && base.scale_y()>0. {
            return(*base,crate::geometry::Rect::new(bounds.x+(m.translate_x()-base.translate_x())/base.scale_x(),bounds.y+(m.translate_y()-base.translate_y())/base.scale_y(),bounds.width,bounds.height));
        }
        let Some(inverse)=base.invert()else{return(m,bounds)};
        let relative=sk::Matrix::concat(&inverse,&m);
        if relative.has_perspective()||relative.skew_x()!=0.||relative.skew_y()!=0.||relative.scale_x()<=0.||relative.scale_y()<=0. {return(m,bounds)}
        let (r,_)=relative.map_rect(sk::Rect::from_xywh(bounds.x,bounds.y,bounds.width,bounds.height));
        (*base,crate::geometry::Rect::new(r.x(),r.y(),r.width(),r.height()))
    })
}
/// QSG batch renderer: float projection, qreal bounds, then viewport qRound.
pub(crate) fn rectangular_clip(c:&sk::Canvas,bounds:sk::Rect)->Option<sk::Rect> {
    if !materials_enabled() || !enabled(c) {return None;}
    let target=ACTIVE.with(|a|a.borrow().clone())?;
    let m=c.local_to_device_as_3x3();
    Some(project_clip(bounds,&m,target.size,target.top_left))
}
fn project_clip(bounds:sk::Rect,m:&sk::Matrix,size:(i32,i32),top_left:bool)->sk::Rect {
    let (px,py)=(2./size.0 as f32,2./size.1 as f32);
    let sx=px*m.scale_x();let tx=px*m.translate_x()-1.;
    let (sy,ty)=if top_left {(py*m.scale_y(),py*m.translate_y()-1.)}else{(-py*m.scale_y(),1.-py*m.translate_y())};
    let edge=|v:f32,s:f32,t:f32,extent:i32| {
        let v=(v as f64*s as f64+t as f64+1.)*extent as f64*0.5;
        (v+0.5f64.copysign(v)) as i32
    };
    let x0=edge(bounds.left,sx,tx,size.0);let x1=edge(bounds.right,sx,tx,size.0);
    let y0=edge(bounds.top,sy,ty,size.1);let y1=edge(bounds.bottom,sy,ty,size.1);
    let (yt,yb)=if top_left {(y0.min(y1),y0.max(y1))}else{(size.1-y0.max(y1),size.1-y0.min(y1))};
    sk::Rect::new(x0.min(x1)as f32,yt as f32,x0.max(x1)as f32,yb as f32)
}
#[derive(Clone)]
struct Target {
    context: Rc<RefCell<Context>>,
    canvas: usize,
    fbo: u32,
    size: (i32, i32),
    top_left: bool,
    // Qt layers project their logical padded extent directly, independently of
    // layer.textureSize. Refactoring via the physical scale loses float bits.
    merged_rect_projection: Option<(sk::Matrix, [f32; 16])>,
}
pub(crate) struct TargetGuard(Option<Target>);
impl Drop for TargetGuard {
    fn drop(&mut self) {
        ACTIVE.with(|a| *a.borrow_mut() = self.0.take());
    }
}
pub(crate) fn target(
    context: Rc<RefCell<Context>>,
    canvas: &sk::Canvas,
    fbo: u32,
    size: (i32, i32),
) -> TargetGuard {
    let previous = ACTIVE.with(|a| {
        a.replace(Some(Target {
            context,
            canvas: canvas as *const _ as usize,
            fbo,
            size,
            top_left: false,
            merged_rect_projection: None,
        }))
    });
    TargetGuard(previous)
}
/// A separately allocated layer source has its own resolved framebuffer. Native
/// draws are safe there even while its parent destination is inside saveLayer.
pub(crate) struct OffscreenGuard {
    _target: TargetGuard,
    blocked: usize,
}
impl Drop for OffscreenGuard {
    fn drop(&mut self) {
        BLOCKED.with(|b| b.set(self.blocked));
    }
}
fn surface_top_left(surface:&mut sk::Surface)->bool {
    sk::gpu::images::get_backend_texture_from_image(&surface.image_snapshot(),false)
        .is_some_and(|(_,origin)|origin==sk::gpu::SurfaceOrigin::TopLeft)
}
pub(crate) fn offscreen(surface: &mut sk::Surface) -> Result<Option<OffscreenGuard>, RenderError> {
    let Some(context) = ACTIVE.with(|a| a.borrow().as_ref().map(|t| t.context.clone())) else {
        return Ok(None);
    };
    let Some(backend) = sk::gpu::surfaces::get_backend_render_target(
        surface,
        sk::surface::BackendHandleAccess::FlushWrite,
    ) else {
        return Ok(None);
    };
    let Some(info) = backend.gl_framebuffer_info() else {
        return Ok(None);
    };
    let size = (surface.width(), surface.height());
    let top_left=surface_top_left(surface);
    let guard = target(context, surface.canvas(), info.fboid, size);
    ACTIVE.with(|a|a.borrow_mut().as_mut().unwrap().top_left=top_left);
    let blocked = BLOCKED.with(|b| b.replace(0));
    Ok(Some(OffscreenGuard {
        _target: guard,
        blocked,
    }))
}
fn layer_rect_projection(logical: (f32, f32), padding: f32, top_left: bool) -> [f32; 16] {
    let y = if top_left { 1. } else { -1. };
    [
        2. / logical.0, 0., 0., 0.,
        0., y * 2. / logical.1, 0., 0.,
        0., 0., 1., 0.,
        -(logical.0 - 2. * padding) / logical.0,
        -y * (logical.1 - 2. * padding) / logical.1, 0., 1.,
    ]
}
/// QSG layer orthographic basis for rectangles merged into the source root.
/// The offscreen target guard owns/restores this metadata with the target.
pub(crate) fn set_layer_rect_projection(c: &sk::Canvas, logical: (f32, f32), padding: f32) {
    ACTIVE.with(|a| {
        if let Some(t) = a.borrow_mut().as_mut().filter(|t| t.canvas == c as *const _ as usize) {
            t.merged_rect_projection = Some((
                c.local_to_device_as_3x3(),
                layer_rect_projection(logical, padding, t.top_left),
            ));
        }
    });
}
pub(crate) struct MaterialScope(bool);
pub(crate) fn materials(value: Option<bool>) -> MaterialScope {
    MaterialScope(MATERIALS.with(|v| {
        let old = v.get();
        if let Some(value) = value {
            v.set(value)
        }
        old
    }))
}
impl Drop for MaterialScope {
    fn drop(&mut self) {
        MATERIALS.with(|v| v.set(self.0));
    }
}
pub(crate) fn materials_enabled() -> bool {
    MATERIALS.with(Cell::get)
}
pub(crate) fn materials_active() -> bool {
    materials_enabled() && ACTIVE.with(|a| a.borrow().is_some())
}
pub(crate) fn material_target(c: &sk::Canvas) -> bool {
    materials_enabled() && enabled(c)
}
pub(crate) struct Block;
pub(crate) fn block() -> Block {
    BLOCKED.with(|b| b.set(b.get() + 1));
    Block
}
impl Drop for Block {
    fn drop(&mut self) {
        BLOCKED.with(|b| b.set(b.get() - 1));
    }
}
pub(crate) fn enabled(c: &sk::Canvas) -> bool {
    BLOCKED.with(|b| b.get() == 0)
        && c.is_clip_rect()
        && ACTIVE.with(|a| {
            a.borrow()
                .as_ref()
                .is_some_and(|t| t.canvas == c as *const _ as usize)
        })
}
/// Clear a rectangular damaged portion of the registered RGBA8 target directly.
/// Avoid recording a Skia clear op solely to flush it at the next native node.
pub(crate) fn clear_damage(canvas:&sk::Canvas)->bool {
    if !enabled(canvas){return false;}
    let Some(clip)=canvas.device_clip_bounds()else{return true;};
    let Some(mut skia)=canvas.direct_context()else{return false;};
    skia.flush(None);
    ACTIVE.with(|active|{
        let active=active.borrow();let target=active.as_ref().unwrap();let context=target.context.borrow();
        unsafe{
            let gl=&context.gl;
            gl.bind_framebuffer(glow::FRAMEBUFFER,std::num::NonZeroU32::new(target.fbo).map(glow::NativeFramebuffer));
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(clip.left,if target.top_left{clip.top}else{target.size.1-clip.bottom},clip.width(),clip.height());
            gl.color_mask(true,true,true,true);gl.clear_color(0.,0.,0.,0.);gl.clear(glow::COLOR_BUFFER_BIT);
        }
    });
    skia.reset(None);
    true
}
pub(crate) fn sync_caches(canvas: &sk::Canvas, epoch: &Rc<Cell<u64>>) -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .filter(|target| target.canvas == canvas as *const _ as usize)
            .is_some_and(|target| target.context.borrow_mut().sync_caches(epoch))
    })
}
const TEXTURE_BYTES: usize = 8 * 1024 * 1024;
fn texture_bytes(mut width: usize, mut height: usize, mipmaps: bool) -> usize {
    let mut bytes = width.saturating_mul(height).saturating_mul(4);
    if mipmaps {
        while width > 1 || height > 1 {
            width = (width / 2).max(1);
            height = (height / 2).max(1);
            bytes = bytes.saturating_add(width.saturating_mul(height).saturating_mul(4));
        }
    }
    bytes
}
pub(crate) fn rect(
    c: &sk::Canvas,
    mesh: Rc<crate::qt_rect::Mesh>,
    matrix: &sk::Matrix,
    opacity: f32,
    aa: bool,
) -> Result<bool, RenderError> {
    if !materials_enabled() || !enabled(c) || std::env::var_os("HYPRV_RECT_GL_DISABLE").is_some() {
        return Ok(false);
    }
    let target = ACTIVE.with(|a| a.borrow().clone()).unwrap();
    let Some(clip) = c.device_clip_bounds() else {
        return Ok(true);
    };
    let (base,bounds)=image_geometry(*matrix,crate::geometry::Rect::new(0.,0.,1.,1.));
    let translated=(!aa || RECT_MERGE_ALLOWED.with(Cell::get)) && base.scale_x()==matrix.scale_x() && base.scale_y()==matrix.scale_y()
        && base.skew_x()==matrix.skew_x() && base.skew_y()==matrix.skew_y();
    let (matrix,translation)=if translated {(&base,[bounds.x,bounds.y])}else{(matrix,[0.,0.])};
    let (px, py) = (2.0 / target.size.0 as f32, 2.0 / target.size.1 as f32);
    let mut transform = [
        px * matrix.scale_x(),
        -py * matrix.skew_y(),
        0.,
        0.,
        px * matrix.skew_x(),
        -py * matrix.scale_y(),
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        px * matrix.translate_x() - 1.,
        1. - py * matrix.translate_y(),
        0.,
        1.,
    ];
    if target.top_left {for i in [1,5,9,13]{transform[i] = -transform[i];}}
    if let Some((root, projection))=&target.merged_rect_projection {
        if root == matrix {
            transform = *projection;
        }
    }
    let mut skia = c.direct_context().ok_or(RenderError::Skia)?;
    skia.flush(None);
    let result = (|| -> Result<(), String> {
        let mut context = target.context.borrow_mut();
        unsafe {
            let gl = &context.gl;
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(target.fbo).map(glow::NativeFramebuffer),
            );
            gl.viewport(0, 0, target.size.0, target.size.1);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left,
                if target.top_left {clip.top}else{target.size.1 - clip.bottom},
                clip.width(),
                clip.height(),
            );
            for flag in [
                glow::STENCIL_TEST,
                glow::DEPTH_TEST,
                glow::CULL_FACE,
                glow::DITHER,
            ] {
                if flag==glow::DITHER {gl.enable(flag)}else{gl.disable(flag)}
            }
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.color_mask(true, true, true, true);
            if let Some(vao) = context.vao {
                gl.bind_vertex_array(Some(vao))
            }
            if context.rectangles.is_none() {
                context.rectangles = Some(crate::qt_gl_rect::Cache::new(&context.gl)?)
            }
            let Context { gl, rectangles, .. } = &mut *context;
            rectangles
                .as_mut()
                .unwrap()
                .draw(gl, target.size, mesh, (&transform,translation), opacity, aa)
        }
    })();
    skia.reset(None);
    result.map_err(RenderError::NativeGl)?;
    Ok(true)
}
pub(crate) fn effect(
    output: &mut sk::Surface,
    images: &[sk::Image],
    data: &[f32],
    down: bool,
) -> Result<bool, RenderError> {
    if !materials_enabled() || std::env::var_os("HYPRV_EFFECT_GL_DISABLE").is_some() {
        return Ok(false);
    }
    let Some(context) = ACTIVE.with(|a| a.borrow().as_ref().map(|t| t.context.clone())) else {
        return Ok(false);
    };
    if context.borrow().gl.version().major < 3 {
        return Ok(false);
    }
    let textures = images
        .iter()
        .map(|image| {
            let (backend, origin) = sk::gpu::images::get_backend_texture_from_image(image, true)?;
            Some((backend.gl_texture_info()?, origin))
        })
        .collect::<Option<Vec<_>>>();
    let Some(textures) = textures else {
        return Ok(false);
    };
    let Some(backend) = sk::gpu::surfaces::get_backend_render_target(
        output,
        sk::surface::BackendHandleAccess::FlushWrite,
    ) else {
        return Ok(false);
    };
    let Some(info) = backend.gl_framebuffer_info() else {
        return Ok(false);
    };
    let size = (output.width(), output.height());
    let top_left=surface_top_left(output);
    let mut skia = output.canvas().direct_context().ok_or(RenderError::Skia)?;
    skia.flush(None);
    let result = (|| -> Result<(), String> {
        let mut context = context.borrow_mut();
        unsafe {
            let gl = &context.gl;
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(info.fboid).map(glow::NativeFramebuffer),
            );
            gl.viewport(0, 0, size.0, size.1);
            for flag in [
                glow::SCISSOR_TEST,
                glow::STENCIL_TEST,
                glow::DEPTH_TEST,
                glow::CULL_FACE,
                glow::DITHER,
                glow::BLEND,
            ] {
                if flag==glow::DITHER {gl.enable(flag)}else{gl.disable(flag)}
            }

            gl.color_mask(true, true, true, true);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.clear_color(0., 0., 0., 0.);
            gl.clear(glow::COLOR_BUFFER_BIT);
            if let Some(vao) = context.vao {
                gl.bind_vertex_array(Some(vao))
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(context.buffer));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            if context.effects.is_none() {
                context.effects = Some(crate::qt_gl_effect::Programs::new(&context.gl)?)
            }
            // Effect passes can change sampling on shared texture objects.
            for texture in context.textures.values_mut(){texture.sampling=None;}
            context.effects.as_ref().unwrap().draw(
                &context.gl,
                &textures,
                data,
                down,
                crate::qt_gl_effect::Placement{size,rect:[0.,0.,size.0 as f32,size.1 as f32],opacity:1.,top_left,scale:[1.,1.],extent:[size.0 as f32,size.1 as f32]},
            )
        }
    })();
    skia.reset(None);
    result.map_err(RenderError::NativeGl)?;
    Ok(true)
}
pub(crate) fn effect_at(
    c: &sk::Canvas,
    prepared: &crate::effects::NativeEffect,
    bounds: crate::geometry::Rect,
    opacity: f32,
) -> Result<bool, RenderError> {
    if !materials_enabled()
        || !enabled(c)
        || std::env::var_os("HYPRV_EFFECT_ONSCREEN_DISABLE").is_some()
    {
        return Ok(false);
    }
    let target = ACTIVE.with(|a| a.borrow().clone()).unwrap();
    if target.context.borrow().gl.version().major < 3 {
        return Ok(false);
    }
    let Some(clip) = c.device_clip_bounds() else {
        return Ok(true);
    };
    let matrix = c.local_to_device_as_3x3();
    if matrix.has_perspective()
        || matrix.skew_x() != 0.
        || matrix.skew_y() != 0.
        || matrix.scale_x() <= 0.
        || matrix.scale_y() <= 0.
    {
        return Ok(false);
    }
    let rect = [
        bounds.x * matrix.scale_x() + matrix.translate_x(),
        bounds.y * matrix.scale_y() + matrix.translate_y(),
        bounds.width * matrix.scale_x(),
        bounds.height * matrix.scale_y(),
    ];
    let textures = prepared
        .images
        .iter()
        .map(|image| {
            let (backend, origin) = sk::gpu::images::get_backend_texture_from_image(image, true)?;
            Some((backend.gl_texture_info()?, origin))
        })
        .collect::<Option<Vec<_>>>();
    let Some(textures) = textures else {
        return Ok(false);
    };
    let mut skia = c.direct_context().ok_or(RenderError::Skia)?;
    skia.flush(None);
    let result = (|| -> Result<(), String> {
        let mut context = target.context.borrow_mut();
        unsafe {
            let gl = &context.gl;
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(target.fbo).map(glow::NativeFramebuffer),
            );
            gl.viewport(0, 0, target.size.0, target.size.1);
            for flag in [
                glow::STENCIL_TEST,
                glow::DEPTH_TEST,
                glow::CULL_FACE,
                glow::DITHER,
            ] {
                if flag==glow::DITHER {gl.enable(flag)}else{gl.disable(flag)}
            }
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left,
                if target.top_left {clip.top}else{target.size.1 - clip.bottom},
                clip.width(),
                clip.height(),
            );
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.color_mask(true, true, true, true);
            if let Some(vao) = context.vao {
                gl.bind_vertex_array(Some(vao))
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(context.buffer));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            if context.effects.is_none() {
                context.effects = Some(crate::qt_gl_effect::Programs::new(&context.gl)?)
            }
            // Effect passes can change sampling on shared texture objects.
            for texture in context.textures.values_mut(){texture.sampling=None;}
            context.effects.as_ref().unwrap().draw(
                &context.gl,
                &textures,
                &prepared.uniforms,
                false,
                crate::qt_gl_effect::Placement{size:target.size,rect,opacity,top_left:target.top_left,scale:[matrix.scale_x(),matrix.scale_y()],extent:[bounds.width,bounds.height]},
            )
        }
    })();
    skia.reset(None);
    result.map_err(RenderError::NativeGl)?;
    Ok(true)
}
pub(crate) struct Glyph {
    pub image: sk::Image,
    pub x: f32,
    pub y: f32,
}
pub(crate) fn sampled_image(
    c: &sk::Canvas,
    image: &sk::Image,
    bounds: crate::geometry::Rect,
    sampling: crate::image::ImageSampling,
    opacity: f32,
) -> Result<bool, RenderError> {
    if !materials_enabled() || !enabled(c) {
        return Ok(false);
    }
    let m = c.local_to_device_as_3x3();
    if m.has_perspective()
        || m.skew_x() != 0.
        || m.skew_y() != 0.
        || m.scale_x() <= 0.
        || m.scale_y() <= 0.
    {
        return Ok(false);
    }
    let rect = crate::geometry::Rect::new(
        bounds.x * m.scale_x() + m.translate_x(),
        bounds.y * m.scale_y() + m.translate_y(),
        bounds.width * m.scale_x(),
        bounds.height * m.scale_y(),
    );
    if image.width() as i64 * image.height() as i64 * 4 > 8 * 1024 * 1024
        || std::env::var_os("HYPRV_IMAGE_GL_DISABLE").is_some()
    {
        return Ok(false);
    }
    match draw_kind(
        c,
        &[Glyph {
            image: image.clone(),
            x: rect.x,
            y: rect.y,
        }],
        [opacity; 4],
        true,
        Some((rect.width, rect.height)),
        sampling,
        Some(image_geometry(m, bounds)),
    ) {
        Ok(()) => Ok(true),
        Err(error) => {
            log_fallback_once(&error);
            // GPU snapshots require their owning DirectContext for readback. Never
            // turn an optional upload optimization into a fatal render error.
            if let Some(copy) = raster_copy(c, image) {
                if !crate::image::draw(c, &copy, bounds, sampling, opacity) {
                    let mut paint = sk::Paint::default();
                    paint.set_alpha_f(opacity);
                    c.draw_image_rect_with_sampling_options(
                        &copy,
                        None,
                        crate::render::skrect(bounds),
                        sampling.skia(),
                        &paint,
                    );
                }
                Ok(true)
            } else {
                // Even if readback fails (e.g. context loss), leave the original
                // image to Skia's ordinary path rather than stopping the host.
                Ok(false)
            }
        }
    }
}
fn raster_copy(c: &sk::Canvas, image: &sk::Image) -> Option<sk::Image> {
    let mut context = c.direct_context();
    image.make_raster_image(context.as_mut(), sk::image::CachingHint::Allow)
}
fn log_fallback_once(error: &RenderError) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| eprintln!("native GL upload/draw failed; using Skia CPU-copy fallback: {error} (further errors suppressed)"));
}

pub(crate) fn draw(c: &sk::Canvas, glyphs: &[Glyph], color: [f32; 4]) -> Result<(), RenderError> {
    if let Err(error) = draw_kind(
        c,
        glyphs,
        color,
        false,
        None,
        crate::image::ImageSampling::Nearest,
        None,
    ) {
        log_fallback_once(&error);
        // Preserve batching on success. An upload failure may follow already
        // painted masks: only copy the remaining masks, avoiding double alpha.
        let completed = ACTIVE.with(|a| {
            a.borrow()
                .as_ref()
                .map_or(0, |target| target.context.borrow().completed_glyphs)
        });
        for glyph in &glyphs[completed.min(glyphs.len())..] {
            let copy = raster_copy(c, &glyph.image);
            let image = copy.as_ref().unwrap_or(&glyph.image);
            let mut paint = sk::Paint::default();
            let alpha = color[3];
            let straight = if alpha > 0. {
                [color[0] / alpha, color[1] / alpha, color[2] / alpha]
            } else {
                [0.; 3]
            };
            paint.set_color4f(
                sk::Color4f::new(straight[0], straight[1], straight[2], alpha),
                None,
            );
            c.draw_image(image, (glyph.x, glyph.y), Some(&paint));
        }
    }
    Ok(())
}
/// Qt textmask.vert: transform fixed-DPR glyph quads, then snap every vertex.
pub(crate) fn draw_transformed(c:&sk::Canvas,glyphs:&[Glyph],color:[f32;4],matrix:&sk::Matrix,dpr:f32)->Result<(),RenderError> {
    let snap=|v:f32|(v+0.5).floor();
    for glyph in glyphs {
        let p0=matrix.map_point((glyph.x/dpr,glyph.y/dpr));
        let p1=matrix.map_point(((glyph.x+glyph.image.width()as f32)/dpr,(glyph.y+glyph.image.height()as f32)/dpr));
        let (x,y)=(snap(p0.x),snap(p0.y));let size=(snap(p1.x)-x,snap(p1.y)-y);
        if size.0<=0. || size.1<=0. {continue;}
        let transformed=Glyph{image:glyph.image.clone(),x,y};
        if let Err(error)=draw_kind(c,std::slice::from_ref(&transformed),color,false,Some(size),crate::image::ImageSampling::Nearest,None) {
            log_fallback_once(&error);
            if ACTIVE.with(|a|a.borrow().as_ref().map_or(0,|t|t.context.borrow().completed_glyphs))>0 {continue;}
            let copy=raster_copy(c,&glyph.image);let image=copy.as_ref().unwrap_or(&glyph.image);
            let alpha=color[3];let mut paint=sk::Paint::default();
            let rgb=if alpha>0. {[color[0]/alpha,color[1]/alpha,color[2]/alpha]}else{[0.;3]};
            paint.set_color4f(sk::Color4f::new(rgb[0],rgb[1],rgb[2],alpha),None);
            c.draw_image_rect_with_sampling_options(image,None,sk::Rect::from_xywh(x,y,size.0,size.1),sk::SamplingOptions::from(sk::FilterMode::Nearest),&paint);
        }
    }
    Ok(())
}

fn draw_kind(
    c: &sk::Canvas,
    glyphs: &[Glyph],
    color: [f32; 4],
    rgba: bool,
    size: Option<(f32, f32)>,
    sampling: crate::image::ImageSampling,
    geometry: Option<(sk::Matrix, crate::geometry::Rect)>,
) -> Result<(), RenderError> {
    let target = ACTIVE
        .with(|a| a.borrow().clone())
        .expect("eligible native GL target");
    let Some(clip) = c.device_clip_bounds() else {
        return Ok(());
    };
    let mut skia = c.direct_context().ok_or(RenderError::Skia)?;
    // Flush Skia recording into this same GL command stream, without submitting
    // the driver queue per text/rectangle. GL orders native draws after these
    // commands; the final EGL swap submits the frame once.
    skia.flush(None);
    let result = target.context.borrow_mut().draw(
        &target,
        glyphs,
        color,
        clip,
        ImageStyle {
            rgba,
            extent: size,
            sampling,
            geometry,
        },
    );
    // We changed bindings, attributes, viewport, blend and pixel-store state.
    // Skia must re-establish its complete state on its next operation.
    skia.reset(None);
    result.map_err(RenderError::NativeGl)
}
struct RgbaProgram {
    program: glow::NativeProgram,
    buffer: glow::NativeBuffer,
    matrix: glow::NativeUniformLocation,
    color: Option<glow::NativeUniformLocation>,
}
impl RgbaProgram {
    unsafe fn new(gl:&glow::Context,opaque:bool)->Result<Self,String>{
        let program=gl.create_program()?;
        let setup=(||->Result<_,String>{
            for(kind,source)in[(glow::VERTEX_SHADER,include_str!("qt_gl_image.vert")),(glow::FRAGMENT_SHADER,if opaque{include_str!("qt_gl_image.frag")}else{include_str!("qt_gl_image_alpha.frag")})]{
                let shader=gl.create_shader(kind)?;gl.shader_source(shader,&shader_source(gl,source));gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader){let e=gl.get_shader_info_log(shader);gl.delete_shader(shader);return Err(e)}
                gl.attach_shader(program,shader);gl.delete_shader(shader);
            }
            gl.bind_attrib_location(program,0,"qt_VertexPosition");gl.bind_attrib_location(program,1,"qt_VertexTexCoord");gl.bind_attrib_location(program,7,"_qt_order");gl.link_program(program);
            if !gl.get_program_link_status(program){return Err(gl.get_program_info_log(program))}
            gl.use_program(Some(program));gl.uniform_1_i32(gl.get_uniform_location(program,"qt_Texture").as_ref(),0);
            let matrix=gl.get_uniform_location(program,"_25.qt_Matrix").ok_or("image matrix")?;
            let color=gl.get_uniform_location(program,"_23.opacity");let buffer=gl.create_buffer()?;
            Ok((matrix,color,buffer))
        })();
        match setup{Ok((matrix,color,buffer))=>Ok(Self{program,matrix,color,buffer}),Err(e)=>{gl.delete_program(program);Err(e)}}
    }
}
struct ImageStyle {
    rgba: bool,
    extent: Option<(f32, f32)>,
    sampling: crate::image::ImageSampling,
    geometry: Option<(sk::Matrix, crate::geometry::Rect)>,
}
struct Texture {
    _source: sk::Image,
    owners: Vec<u64>,
    texture: glow::NativeTexture,
    used: u64,
    bytes: usize,
    mipmaps: bool,
    opaque: bool,
    canvas: bool,
    sampling: Option<crate::image::ImageSampling>,
}
struct MaskProgram {
    program: glow::NativeProgram,
    rect: glow::NativeUniformLocation,
    size: glow::NativeUniformLocation,
    color: glow::NativeUniformLocation,
    direction: Option<glow::NativeUniformLocation>,
}
impl MaskProgram {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        let setup = (|| -> Result<_, String> {
            for(kind,source)in[
            (glow::VERTEX_SHADER,"attribute vec2 pos; uniform vec2 size; uniform vec4 rect; uniform float direction; varying highp vec2 uv; void main(){vec2 p=rect.xy+pos*rect.zw;gl_Position=vec4(2.0*p.x/size.x-1.0,(1.0-2.0*p.y/size.y)*direction,0.0,1.0);uv=pos;}"),
            (glow::FRAGMENT_SHADER,"precision highp float; uniform highp sampler2D mask; uniform vec4 color; varying highp vec2 uv; void main(){gl_FragColor=color*texture2D(mask,uv).a;}")]{
            let shader=gl.create_shader(kind)?;gl.shader_source(shader,&shader_source(gl,source));gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader){let error=gl.get_shader_info_log(shader);gl.delete_shader(shader);return Err(error);}
            gl.attach_shader(program,shader);gl.delete_shader(shader);
        }
            gl.bind_attrib_location(program, 0, "pos");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                let e = gl.get_program_info_log(program);
                return Err(e);
            }
            let rect = gl
                .get_uniform_location(program, "rect")
                .ok_or("missing rect uniform")?;
            let size = gl
                .get_uniform_location(program, "size")
                .ok_or("missing size uniform")?;
            let color = gl
                .get_uniform_location(program, "color")
                .ok_or("missing color uniform")?;
            gl.use_program(Some(program));
            gl.uniform_1_i32(gl.get_uniform_location(program, "mask").as_ref(), 0);
            let direction=gl.get_uniform_location(program,"direction");
            Ok(Self { program, rect, size, color,direction })
        })();
        if setup.is_err() { gl.delete_program(program); }
        setup
    }
}
pub(crate) struct Context {
    gl: glow::Context,
    mask: Option<MaskProgram>,
    glyph_atlas:Option<crate::qt_gl_glyphs::Atlas>,
    buffer: glow::NativeBuffer,
    vao: Option<glow::NativeVertexArray>,
    rgba: Option<RgbaProgram>,
    opaque_rgba: Option<RgbaProgram>,
    rectangles: Option<crate::qt_gl_rect::Cache>,
    effects: Option<crate::qt_gl_effect::Programs>,
    textures: HashMap<(u32,u32,u32), Texture>,
    clock: u64,
    bytes: usize,
    completed_glyphs: usize,
    resource_owner: u64,
    #[cfg(test)]
    fail_upload_after: Option<usize>,
    cache_epoch: Option<(std::rc::Weak<Cell<u64>>, u64)>,
}
impl Context {
    pub(crate) fn can_blit(&self) -> bool { self.gl.version().major >= 3 }
    /// Copy single-sample RGBA8 framebuffer bytes without a texture snapshot,
    /// shader compilation or copy-on-write. Caller resets Skia's cached GL state.
    pub(crate) fn blit_to_window(&self, source: u32, damage: [i32;4]) -> Result<(), String> {
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, std::num::NonZeroU32::new(source).map(glow::NativeFramebuffer));
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            let [x,y,w,h]=damage;
            self.gl.blit_framebuffer(x,y,x+w,y+h,x,y,x+w,y+h,glow::COLOR_BUFFER_BIT,glow::NEAREST);
            let error = self.gl.get_error();
            if error != glow::NO_ERROR { return Err(format!("framebuffer blit GL error {error:#x}")); }
        }
        Ok(())
    }
    /// Explicit copy-parity diagnostic only; never used by normal frames.
    pub(crate) fn window_pixels(&self, size: (i32, i32)) -> Result<Vec<u8>, String> {
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.read_buffer(glow::BACK);
            self.gl.read_pixels(0,0,size.0,size.1,glow::RGBA,glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)));
            let error = self.gl.get_error();
            if error != glow::NO_ERROR { return Err(format!("framebuffer read GL error {error:#x}")); }
        }
        Ok(pixels)
    }
    /// Startup preparation barrier; caller owns the current context.
    pub(crate) fn finish(&self) { unsafe { self.gl.finish(); } }
    /// Caller owns the current EGL context for this object's complete lifetime.
    pub(crate) unsafe fn new(gl: glow::Context) -> Result<Self, String> {
        let mut allocated_buffer = None;
        let mut allocated_vao = None;
        let setup = (|| -> Result<_, String> {
            let buffer = gl.create_buffer()?;
            allocated_buffer = Some(buffer);
            let vao = if gl.version().major >= 3
                || gl
                    .supported_extensions()
                    .contains("GL_OES_vertex_array_object")
            {
                Some(gl.create_vertex_array()?)
            } else {
                None
            };
            allocated_vao = vao;
            if let Some(vao) = vao {
                gl.bind_vertex_array(Some(vao));
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            let vertices = [0f32, 0., 0., 1., 1., 0., 1., 1.];
            let bytes: Vec<u8> = vertices.into_iter().flat_map(f32::to_ne_bytes).collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            Ok((buffer, vao))
        })();
        let (buffer, vao) = match setup {
            Ok(value) => value,
            Err(error) => {
                if let Some(vao) = allocated_vao {
                    gl.delete_vertex_array(vao);
                }
                if let Some(buffer) = allocated_buffer {
                    gl.delete_buffer(buffer);
                }
                return Err(error);
            }
        };
        Ok(Self {
            gl,
            mask: None,
            glyph_atlas:None,
            buffer,
            vao,
            rgba: None,
            opaque_rgba: None,
            rectangles: None,
            effects: None,
            textures: HashMap::new(),
            clock: 0,
            bytes: 0,
            completed_glyphs: 0,
            resource_owner: 0,
            #[cfg(test)]
            fail_upload_after: None,
            cache_epoch: None,
        })
    }
    pub(crate) fn set_resource_owner(&mut self, owner: u64) { self.resource_owner = owner;
        if let Some(atlas) = &mut self.glyph_atlas { atlas.set_resource_owner(owner); }
    }
    /// Release only textures last used by this retired window. Programs and
    /// resources shared with other windows/preparation remain warm.
    pub(crate) fn release_owner(&mut self, owner: u64) {
        self.textures.retain(|_, texture| {
            texture.owners.retain(|id| *id != owner);
            if !texture.owners.is_empty() { return true; }
            unsafe { self.gl.delete_texture(texture.texture); }
            self.bytes -= texture.bytes;
            false
        });
        if let Some(atlas) = &mut self.glyph_atlas { atlas.release_owner(&self.gl, owner); }
    }
    /// Caller must make this Context's owning EGL context current first.
    pub(crate) fn clear_caches(&mut self) {
        unsafe {
            if let Some(mask) = self.mask.take() { self.gl.delete_program(mask.program); }
            if let Some(atlas)=self.glyph_atlas.take(){atlas.destroy(&self.gl);}
            for (_, texture) in self.textures.drain() {
                self.gl.delete_texture(texture.texture);
            }
            if let Some(mut cache) = self.rectangles.take() {
                cache.destroy(&self.gl);
            }
            if let Some(cache) = self.effects.take() {
                cache.destroy(&self.gl);
            }
            if let Some(program) = self.rgba.take() { self.gl.delete_program(program.program);self.gl.delete_buffer(program.buffer); }
            if let Some(program) = self.opaque_rgba.take() { self.gl.delete_program(program.program);self.gl.delete_buffer(program.buffer); }
        }
        self.bytes = 0;
        self.cache_epoch = None;
    }
    pub(crate) fn sync_caches(&mut self, epoch: &Rc<Cell<u64>>) -> bool {
        let valid = self
            .cache_epoch
            .as_ref()
            .is_some_and(|(owner, generation)| {
                owner.ptr_eq(&Rc::downgrade(epoch)) && *generation == epoch.get()
            });
        if !valid {
            self.clear_caches();
            self.cache_epoch = Some((Rc::downgrade(epoch), epoch.get()));
        }
        !valid
    }
    fn draw(
        &mut self,
        target: &Target,
        glyphs: &[Glyph],
        color: [f32; 4],
        clip: sk::IRect,
        style: ImageStyle,
    ) -> Result<(), String> {
        self.completed_glyphs = 0;
        if !style.rgba && style.geometry.is_none() && glyphs.iter().all(|glyph| {
            let(w,h)=style.extent.unwrap_or((glyph.image.width()as f32,glyph.image.height()as f32));
            glyph.x>=clip.right as f32 || glyph.y>=clip.bottom as f32
                || glyph.x+w<=clip.left as f32 || glyph.y+h<=clip.top as f32
        }){
            self.completed_glyphs=glyphs.len();return Ok(());
        }
        if !style.rgba && style.extent.is_none() && style.geometry.is_none()
            && std::env::var_os("HYPRV_GLYPH_ATLAS_DISABLE").is_none() {
            if self.glyph_atlas.is_none(){self.glyph_atlas=Some(crate::qt_gl_glyphs::Atlas::new(&self.gl,self.resource_owner)?);}
            if let Some(vao)=self.vao{unsafe{self.gl.bind_vertex_array(Some(vao));}}
            #[cfg(test)] let mut budget=self.fail_upload_after.take();
            let result=self.glyph_atlas.as_mut().unwrap().draw(&self.gl,(target.size,target.fbo,target.top_left),glyphs,color,clip,||{
                #[cfg(test)] if let Some(remaining)=budget.as_mut(){if *remaining==0{budget=None;return Err("injected texture upload failure".into());}*remaining-=1;}
                Ok(())
            });
            #[cfg(test)] {self.fail_upload_after=budget;}
            if result?{self.completed_glyphs=glyphs.len();return Ok(());}
        }
        // SAFETY: TargetGuard is installed only by EglWindow::draw while its
        // owning context is current. Text geometry is device-space, root-target
        // only; layer and nonrectangular-clip traversal cannot enter this path.
        unsafe {
            if style.rgba && self.rgba.is_none() {
                self.rgba = Some(RgbaProgram::new(&self.gl, false)?)
            }
            if !style.rgba && self.mask.is_none() {
                self.mask = Some(MaskProgram::new(&self.gl)?);
            }
            let program = if style.rgba { self.rgba.as_ref().unwrap().program }
                else { self.mask.as_ref().unwrap().program };
            let rect = self.mask.as_ref().map(|p| &p.rect);
            let size = self.mask.as_ref().map(|p| &p.size);
            let uniform_color = self.mask.as_ref().map(|p| &p.color);
            let gl = &self.gl;
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(target.fbo).map(glow::NativeFramebuffer),
            );
            gl.viewport(0, 0, target.size.0, target.size.1);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left,
                if target.top_left {clip.top}else{target.size.1 - clip.bottom},
                clip.width(),
                clip.height(),
            );
            for flag in [
                glow::STENCIL_TEST,
                glow::DEPTH_TEST,
                glow::CULL_FACE,
                glow::DITHER,
            ] {
                if flag==glow::DITHER {gl.enable(flag)}else{gl.disable(flag)}
            }
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);

            gl.color_mask(true, true, true, true);
            gl.use_program(Some(program));
            if let Some(vao) = self.vao {
                gl.bind_vertex_array(Some(vao));
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.buffer));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            gl.active_texture(glow::TEXTURE0);
            if gl.version().major >= 3 {
                gl.bind_sampler(0, None);
                gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
            }
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            if !style.rgba {
                gl.uniform_2_f32(size, target.size.0 as f32, target.size.1 as f32);
                gl.uniform_1_f32(self.mask.as_ref().unwrap().direction.as_ref(),if target.top_left {-1.}else{1.});
            }
            if !style.rgba {
                gl.uniform_4_f32(uniform_color, color[0], color[1], color[2], color[3]);
            }
            for glyph in glyphs {
                if !style.rgba && style.geometry.is_none() {
                    let(w,h)=style.extent.unwrap_or((glyph.image.width()as f32,glyph.image.height()as f32));
                    if glyph.x>=clip.right as f32 || glyph.y>=clip.bottom as f32
                        || glyph.x+w<=clip.left as f32 || glyph.y+h<=clip.top as f32 {
                        self.completed_glyphs+=1;
                        crate::diagnostics::count("native.clipped_glyphs");
                        continue;
                    }
                }
                self.clock += 1;
                let atlas=match style.sampling {crate::image::ImageSampling::QtCanvasAtlas {size,..}=>Some(size),_=>None};
                let key = (glyph.image.unique_id(),atlas.map_or(0,|s|s.0),atlas.map_or(0,|s|s.1));
                if !self.textures.contains_key(&key) {
                    crate::diagnostics::count("native.texture_uploads");
                    #[cfg(test)]
                    if let Some(remaining) = self.fail_upload_after.as_mut() {
                        if *remaining == 0 {
                            self.fail_upload_after = None;
                            return Err("injected texture upload failure".into());
                        }
                        *remaining -= 1;
                    }
                    if let Some(size)=atlas {
                        let charge=size.0 as usize*size.1 as usize;
                        evict_textures(gl,&mut self.textures,&mut self.bytes,charge,None,true,charge);
                        let texture=crate::qt_canvas::upload(gl,&glyph.image,size)?;
                        self.bytes+=charge;
                        self.textures.insert(key,Texture {_source:glyph.image.clone(),owners:vec![self.resource_owner],texture,used:self.clock,
                            bytes:charge,mipmaps:false,opaque:false,canvas:true,sampling:Some(style.sampling)});
                    } else {
                    let (w, h) = (glyph.image.width() as usize, glyph.image.height() as usize);
                    let mipmaps = style.sampling == crate::image::ImageSampling::Mipmap;
                    let charge = texture_bytes(w, h, mipmaps);
                    if charge > TEXTURE_BYTES {
                        return Err("native texture exceeds cache budget".into());
                    }
                    let mut rgba = vec![0; texture_bytes(w, h, false)];
                    if glyph.image.color_type() == sk::ColorType::Alpha8 {
                        let pixmap = glyph
                            .image
                            .peek_pixels()
                            .ok_or("native mask is not raster backed")?;
                        let source = pixmap.bytes().ok_or("native mask bytes")?;
                        for y in 0..h {
                            for x in 0..w {
                                let a = source[y * pixmap.row_bytes() + x];
                                rgba[(y * w + x) * 4..(y * w + x + 1) * 4].fill(a);
                            }
                        }
                    } else {
                        let info = sk::ImageInfo::new(
                            (w as i32, h as i32),
                            sk::ColorType::RGBA8888,
                            sk::AlphaType::Premul,
                            None,
                        );
                        if !glyph.image.read_pixels(
                            &info,
                            &mut rgba,
                            w * 4,
                            (0, 0),
                            sk::image::CachingHint::Allow,
                        ) {
                            return Err("native image pixels".into());
                        }
                    }
                    let opaque = rgba.chunks_exact(4).all(|p| p[3] == 255);
                    evict_textures(gl, &mut self.textures, &mut self.bytes, charge, None, true,0);
                    let texture = gl.create_texture()?;
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    for p in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::NEAREST as i32);
                    }
                    for p in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, p, glow::CLAMP_TO_EDGE as i32);
                    }
                    let bgra=style.rgba && gl.version().is_embedded
                        && (gl.supported_extensions().contains("GL_EXT_texture_format_BGRA8888")||gl.supported_extensions().contains("GL_APPLE_texture_format_BGRA8888"));
                    if bgra {for pixel in rgba.chunks_exact_mut(4){pixel.swap(0,2);}}
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        if bgra {glow::BGRA} else if gl.version().major >= 3 {
                            glow::RGBA8
                        } else {
                            glow::RGBA
                        } as i32,
                        w as i32,
                        h as i32,
                        0,
                        if bgra {glow::BGRA}else{glow::RGBA},
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&rgba)),
                    );
                    if mipmaps {
                        gl.generate_mipmap(glow::TEXTURE_2D);
                    }
                    let error = gl.get_error();
                    if error != glow::NO_ERROR {
                        gl.delete_texture(texture);
                        return Err(format!("texture upload GL error 0x{error:x}"));
                    }
                    self.bytes += charge;
                    self.textures.insert(
                        key,
                        Texture {
                            _source: glyph.image.clone(),
                            owners: vec![self.resource_owner],
                            texture,
                            used: self.clock,
                            bytes: charge,
                            mipmaps,
                            opaque,
                            canvas:false,
                            sampling:Some(crate::image::ImageSampling::Nearest),
                        },
                    );
                    }
                }
                if style.sampling == crate::image::ImageSampling::Mipmap
                    && !self.textures[&key].mipmaps
                {
                    let charge = texture_bytes(
                        glyph.image.width() as usize,
                        glyph.image.height() as usize,
                        true,
                    );
                    if charge > TEXTURE_BYTES {
                        return Err("native mip chain exceeds cache budget".into());
                    }
                    let additional = charge - self.textures[&key].bytes;
                    evict_textures(
                        gl,
                        &mut self.textures,
                        &mut self.bytes,
                        additional,
                        Some(key),
                        false,
                        0,
                    );
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[&key].texture));
                    gl.generate_mipmap(glow::TEXTURE_2D);
                    let error = gl.get_error();
                    if error != glow::NO_ERROR {
                        // A failed generation may have allocated only part of the
                        // chain. Discard it rather than retaining uncharged levels.
                        let failed = self.textures.remove(&key).unwrap();
                        self.bytes -= failed.bytes;
                        gl.delete_texture(failed.texture);
                        return Err(format!("mipmap upload GL error 0x{error:x}"));
                    }
                    let texture = self.textures.get_mut(&key).unwrap();
                    texture.mipmaps = true;
                    texture.bytes = charge;
                    self.bytes += additional;
                }
                let texture = self.textures.get_mut(&key).unwrap();
                texture.used = self.clock;
                if !texture.owners.contains(&self.resource_owner) { texture.owners.push(self.resource_owner); }
                gl.bind_texture(glow::TEXTURE_2D, Some(texture.texture));
                let (min, mag) = match style.sampling {
                    crate::image::ImageSampling::Nearest => (glow::NEAREST, glow::NEAREST),
                    crate::image::ImageSampling::Linear | crate::image::ImageSampling::QtCanvasAtlas {..} => (glow::LINEAR, glow::LINEAR),
                    crate::image::ImageSampling::Mipmap => {
                        (glow::LINEAR_MIPMAP_LINEAR, glow::LINEAR)
                    }
                };
                if texture.sampling!=Some(style.sampling) {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, mag as i32);
                    texture.sampling=Some(style.sampling);
                }
                let full_opacity=style.rgba && color[3]==1.;
                let opaque = full_opacity && texture.opaque;
                if full_opacity && self.opaque_rgba.is_none() {
                    self.opaque_rgba = Some(RgbaProgram::new(&self.gl, true)?);
                }
                let rgba = if full_opacity {
                    self.opaque_rgba.as_ref()
                } else {
                    self.rgba.as_ref()
                };
                if let Some(rgba) = rgba.filter(|_| style.rgba) {
                    if opaque {
                        gl.disable(glow::BLEND)
                    } else {
                        gl.enable(glow::BLEND)
                    }
                    gl.use_program(Some(rgba.program));
                    gl.uniform_1_f32(rgba.color.as_ref(), color[3]);

                    let fallback = (
                        sk::Matrix::translate((glyph.x, glyph.y)),
                        crate::geometry::Rect::new(
                            0.,
                            0.,
                            style.extent.map_or(glyph.image.width() as f32, |s| s.0),
                            style.extent.map_or(glyph.image.height() as f32, |s| s.1),
                        ),
                    );
                    let (m, b) = style.geometry.as_ref().unwrap_or(&fallback);
                    let (px, py) = (2.0 / target.size.0 as f32, 2.0 / target.size.1 as f32);
                    let mut matrix = [
                        px * m.scale_x(),
                        -py * m.skew_y(),
                        0.,
                        0.,
                        px * m.skew_x(),
                        -py * m.scale_y(),
                        0.,
                        0.,
                        0.,
                        0.,
                        1.,
                        0.,
                        px * m.translate_x() - 1.,
                        1. - py * m.translate_y(),
                        0.,
                        1.,
                    ];
                    if target.top_left {for i in [1,5,9,13]{matrix[i] = -matrix[i];}}
                    gl.uniform_matrix_4_f32_slice(Some(&rgba.matrix), false, &matrix);
                    let uv=if let crate::image::ImageSampling::QtCanvasAtlas {size,corner}=style.sampling {
                        crate::qt_canvas::uv(size,glyph.image.width(),corner)
                    } else {[0.,0.,1.,1.]};
                    let vertices=[b.x,b.y,uv[0],uv[1],b.x,b.y+b.height,uv[0],uv[1]+uv[3],b.x+b.width,b.y,uv[0]+uv[2],uv[1],b.x+b.width,b.y+b.height,uv[0]+uv[2],uv[1]+uv[3]];
                    let mut data=[0u8;64];for(i,v)in vertices.into_iter().enumerate(){data[i*4..i*4+4].copy_from_slice(&v.to_ne_bytes());}
                    gl.bind_buffer(glow::ARRAY_BUFFER,Some(rgba.buffer));gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,&data,glow::STREAM_DRAW);
                    gl.enable_vertex_attrib_array(0);gl.enable_vertex_attrib_array(1);gl.disable_vertex_attrib_array(7);gl.vertex_attrib_1_f32(7,0.);
                    gl.vertex_attrib_pointer_f32(0,2,glow::FLOAT,false,16,0);gl.vertex_attrib_pointer_f32(1,2,glow::FLOAT,false,16,8);
                } else {
                    gl.uniform_4_f32(
                        rect,
                        glyph.x,
                        glyph.y,
                        style.extent.map_or(glyph.image.width() as f32, |s| s.0),
                        style.extent.map_or(glyph.image.height() as f32, |s| s.1),
                    );
                }
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                if style.rgba {gl.disable_vertex_attrib_array(1);}
                self.completed_glyphs += 1;
            }
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                return Err(format!("GL error 0x{error:x}"));
            }
        }
        Ok(())
    }
}
fn evict_textures(
    gl: &glow::Context,
    textures: &mut HashMap<(u32,u32,u32), Texture>,
    bytes: &mut usize,
    additional: usize,
    protected: Option<(u32,u32,u32)>,
    inserting: bool,
    incoming_canvas: usize,
) {
    // Canvas storage is explicitly charged in diagnostics, separately from the
    // ordinary 8MiB image/mask working set. Do not evict the overlay atlas on
    // every unrelated text draw. At most four live output/scale atlases survive.
    loop {
        let canvas_bytes:usize=textures.values().filter(|t|t.canvas).map(|t|t.bytes).sum();
        let budget=TEXTURE_BYTES.saturating_add(canvas_bytes).saturating_add(incoming_canvas);
        if bytes.saturating_add(additional)<=budget && (!inserting || textures.len()<512)
            && (incoming_canvas==0 || textures.values().filter(|t|t.canvas).count()<4) {break;}
        let Some(key) = textures
            .iter()
            .filter(|(key, _)| Some(**key) != protected)
            .filter(|(_,t)|!t.canvas || incoming_canvas>0)
            .min_by_key(|(_, texture)| texture.used)
            .map(|(key, _)| *key)
        else {
            break;
        };
        let old = textures.remove(&key).unwrap();
        *bytes -= old.bytes;
        unsafe {
            gl.delete_texture(old.texture);
        }
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            for (_, t) in self.textures.drain() {
                self.gl.delete_texture(t.texture);
            }
            if let Some(rects) = &mut self.rectangles {
                rects.destroy(&self.gl);
            }
            if let Some(effects) = &self.effects {
                effects.destroy(&self.gl);
            }
            self.gl.delete_buffer(self.buffer);
            if let Some(vao) = self.vao {
                self.gl.delete_vertex_array(vao);
            }
            if let Some(rgba) = &self.rgba {
                self.gl.delete_program(rgba.program);self.gl.delete_buffer(rgba.buffer);
            }
            if let Some(rgba) = &self.opaque_rgba {
                self.gl.delete_program(rgba.program);self.gl.delete_buffer(rgba.buffer);
            }
            if let Some(mask) = &self.mask { self.gl.delete_program(mask.program); }
            if let Some(atlas)=self.glyph_atlas.take(){atlas.destroy(&self.gl);}
        }
    }
}

#[cfg(test)]
impl Context {
    pub(crate) fn review_masked_compose(&self) -> bool {
        self.effects.as_ref().is_some_and(|effects| effects.review_masked_compose())
    }
    pub(crate) fn review_lazy_programs(&self) {
        assert!(self.mask.is_none() && self.rgba.is_none() && self.opaque_rgba.is_none()
            && self.rectangles.is_none() && self.effects.is_none() && self.glyph_atlas.is_none(), "constructing the shared native context must compile no programs");
        unsafe { crate::qt_gl_effect::Programs::review_lazy_programs(&self.gl); }
    }

    pub(crate) fn review_force_atlas_wrap(&mut self){if let Some(atlas)=&mut self.glyph_atlas{atlas.force_next_wrap();}}
    pub(crate) fn review_upload_budget_untouched(&self)->bool {self.fail_upload_after==Some(0)}
    pub(crate) fn fail_upload_after(&mut self, successful_uploads: usize) {
        self.fail_upload_after = Some(successful_uploads);
    }

    pub(crate) fn review_stats(&self) -> (usize, usize, usize, usize) {
        let full:usize = self
            .textures
            .values()
            .map(|t| {
                if t.canvas {return t.bytes;}
                let (mut w, mut h) = (t._source.width() as usize, t._source.height() as usize);
                let mut bytes = w * h * 4;
                if t.mipmaps {
                    while w > 1 || h > 1 {
                        w = (w / 2).max(1);
                        h = (h / 2).max(1);
                        bytes += w * h * 4;
                    }
                }
                bytes
            })
            .sum();
        let (count, vbo) = self
            .rectangles
            .as_ref()
            .map_or((0, 0), |r| unsafe { r.review_buffers(&self.gl) });
        let(_,atlas_bytes)=self.glyph_atlas.as_ref().map_or((0,0),|atlas|atlas.stats());
        (self.bytes+atlas_bytes, full+atlas_bytes, count, vbo)
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    #[test]
    fn explicit_layer_projection_preserves_qt_orthographic_coefficients() {
        // Observed installed Qt6.12 smoothcolor uniforms for a548x572 source
        // capturing its612x636 padded logical extent (stats chrome).
        let matrix=layer_rect_projection((612.,636.),32.,true);
        assert_eq!(matrix[0].to_bits(),0x3b56_2b81);
        assert_eq!(matrix[5].to_bits(),0x3b4e_168a);
        assert_eq!(matrix[12].to_bits(),0xbf65_3a90);
        assert_eq!(matrix[13].to_bits(),0xbf66_3d2f);
        let factored=(2f32/572.)*(572f32/636.);
        assert_ne!(matrix[5].to_bits(),factored.to_bits());
    }
    #[test]
    fn identical_nondyadic_batch_basis_preserves_bounds_exactly() {
        let mut surface=sk::surfaces::raster_n32_premul((8,8)).unwrap();let c=surface.canvas();
        c.scale((648./388.,380./254.));c.translate((32.,32.));let matrix=c.local_to_device_as_3x3();
        let bounds=crate::geometry::Rect::new(0.,0.,324.,190.);
        {let _batch=batch_root(c);let(base,recovered)=image_geometry(matrix,bounds);assert_eq!(base,matrix);assert_eq!(recovered,bounds);}
        assert!(BATCH.with(|b|b.borrow().is_none()));
    }

    #[test]
    fn direct_relative_translation_avoids_world_coordinate_cancellation() {
        let mut surface=sk::surfaces::raster_n32_premul((8,8)).unwrap();
        let c=surface.canvas();c.scale((2.,2.));c.translate((144.07408,10.));
        let base=c.local_to_device_as_3x3();
        let _batch=batch_root(c);
        let local=sk::Matrix::translate((114.703705,0.));
        let _parent=node_transform(c,&local);c.concat(&local);
        let local=sk::Matrix::translate((98.,0.));
        let _child=node_transform(c,&local);c.concat(&local);
        let world=c.local_to_device_as_3x3();
        let (_,bounds)=image_geometry(world,crate::geometry::Rect::new(0.,0.,38.,38.));
        let exact=114.703705f32+98.;
        assert_eq!(bounds.x,exact);
        assert_ne!((world.translate_x()-base.translate_x())/base.scale_x(),exact);
    }

    #[test]
    fn material_scope_inherits_and_foreign_boundary_restores() {
        assert!(!materials_enabled());
        {
            let _bar = materials(Some(true));
            assert!(materials_enabled());
            {
                let _child = materials(None);
                assert!(materials_enabled());
            }
            {
                let _foreign = materials(Some(false));
                assert!(!materials_enabled());
                {
                    let _opt_in = materials(Some(true));
                    assert!(materials_enabled());
                }
                assert!(!materials_enabled());
            }
            assert!(materials_enabled());
        }
        assert!(!materials_enabled());
    }
}
