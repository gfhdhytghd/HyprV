//! One EGL/Skia context shared by all windows on the UI thread.
//! A persistent GPU render target retains undamaged pixels independently of EGL buffer age.
use crate::{
    geometry::Rect,
    render::{physical_size, Renderer},
    scene::Scene,
};
use khronos_egl as egl;
use skia_safe::{self as sk, gpu};
use std::{cell::RefCell, ffi::c_void, rc::Rc};
use wayland_client::{protocol::wl_surface::WlSurface, Connection, Proxy};
type Api = egl::DynamicInstance<egl::EGL1_5>;
#[cfg(debug_assertions)]
mod frame_audit;
type SetDamage = unsafe extern "system" fn(*mut c_void, *mut c_void, *const i32, i32) -> egl::Boolean;
type SwapDamage =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const i32, i32) -> egl::Boolean;
#[derive(Debug, thiserror::Error)]
pub enum EglError {
    #[error("EGL: {0}")]
    Egl(#[from] egl::Error),
    #[error("graphics initialization: {0}")]
    Init(String),
    #[error(transparent)]
    Render(#[from] crate::render::RenderError),
}
pub struct SharedEgl {
    api: Api,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    pbuffer: egl::Surface,
    skia: RefCell<Option<gpu::DirectContext>>,
    native_text: RefCell<Option<Rc<RefCell<crate::native_gl::Context>>>>,
    swap_damage: Option<SwapDamage>,
    buffer_age: bool,
    set_damage: Option<SetDamage>,
    _connection: Connection,
    #[cfg(debug_assertions)]
    audit_lost: std::cell::Cell<bool>,
}
impl SharedEgl {
    /// Drop unused render targets between component lifetimes, retaining live
    /// surfaces, glyph atlases and shader programs needed by the next frame.
    fn purge_scratch(&self) {
        let _timing = crate::diagnostics::scope("egl.scratch_purge");
        if let Some(context) = self.skia.borrow_mut().as_mut() {
            context.reset(None);
            // draw/preparation already submitted their work. Do not submit the
            // shared context again during native window retirement.
            context.purge_unlocked_resources(gpu::PurgeResourceOptions::ScratchResourcesOnly);
        }
    }
    /// Replay a direct-window scene to a temporary GPU target for diagnostics.
    /// Never re-create an EGL stream on a still-mapped wl_surface just to read
    /// pixels: doing so can change its compositor-visible front buffer.
    pub(crate) fn capture_scene(&self, renderer: &mut Renderer, scene: &Scene,
        logical: (f32,f32), scale: f64, owner: u64, path: &std::path::Path) -> Result<(), EglError> {
        self.api.make_current(self.display,Some(self.pbuffer),Some(self.pbuffer),Some(self.context))?;
        let size = physical_size(logical.0,logical.1,scale)?;
        let mut skia = self.skia.borrow_mut();
        let context = skia.as_mut().unwrap();
        context.reset(None);
        let mut surface = gpu::surfaces::render_target(context,gpu::Budgeted::Yes,
            &sk::ImageInfo::new_n32_premul(size,None),None,gpu::SurfaceOrigin::BottomLeft,
            None,None,None).ok_or_else(||EglError::Init("capture target".into()))?;
        let native = self.native_context(context)?;
        native.borrow_mut().set_resource_owner(owner);
        let backend = gpu::surfaces::get_backend_render_target(&mut surface,
            sk::surface::BackendHandleAccess::FlushWrite).ok_or_else(||EglError::Init("capture framebuffer".into()))?;
        {
            let _target = crate::native_gl::target(native,surface.canvas(),
                backend.gl_framebuffer_info().unwrap().fboid,size);
            renderer.paint(surface.canvas(),scene,scale,Rect::new(0.,0.,logical.0,logical.1))?;
        }
        let image = surface.image_snapshot();
        let png = image.encode(Some(context),sk::EncodedImageFormat::PNG,None)
            .ok_or_else(||EglError::Init("capture PNG".into()))?;
        std::fs::write(path,png.as_bytes()).map_err(|e|EglError::Init(e.to_string()))?;
        drop(image); drop(surface); drop(skia);
        self.purge_scratch();
        Ok(())
    }
    /// Immediately release renderer/native caches while this EGL context is
    /// current, then restore the caller's context and draw/read surfaces.
    /// Call between frames; Renderer::clear_caches alone defers native eviction.
    pub fn clear_caches(&self, renderer: &mut Renderer) -> Result<(), EglError> {
        let previous = (self.api.get_current_display(), self.api.get_current_surface(egl::DRAW),
            self.api.get_current_surface(egl::READ), self.api.get_current_context());
        self.api.make_current(self.display, Some(self.pbuffer), Some(self.pbuffer), Some(self.context))?;
        renderer.clear_caches();
        if let Some(native) = self.native_text.borrow().as_ref() { native.borrow_mut().clear_caches(); }
        if let Some(context) = self.skia.borrow_mut().as_mut() {
            context.reset(None);
            context.free_gpu_resources();
        }
        self.api.make_current(previous.0.unwrap_or(self.display), previous.1, previous.2, previous.3)?;
        Ok(())
    }
    fn native_context(&self, context: &mut gpu::DirectContext) -> Result<Rc<RefCell<crate::native_gl::Context>>, EglError> {
            if self.native_text.borrow().is_none() {
                // SAFETY: this EGL context is current. The resource owner is dropped
                // with this context current, before SharedEgl destroys it.
                let gl = unsafe {
                    glow::Context::from_loader_function(|name| {
                        let address = self.api.get_proc_address(name).or_else(|| {
                            if matches!(
                                name,
                                "glGenVertexArrays" | "glBindVertexArray" | "glDeleteVertexArrays"
                            ) {
                                self.api.get_proc_address(&format!("{name}OES"))
                            } else {
                                None
                            }
                        });
                        address.map_or(std::ptr::null(), |f| f as *const c_void)
                    })
                };
                let native = unsafe { crate::native_gl::Context::new(gl) };
                context.reset(None);
                let native = native.map_err(EglError::Init)?;
                *self.native_text.borrow_mut() = Some(Rc::new(RefCell::new(native)));
            }
        Ok(self.native_text.borrow().as_ref().unwrap().clone())
    }
    /// Compile and execute the popup's effect/material pipelines on the production
    /// context before accepting input. No window buffer, frame callback or timer.
    pub(crate) fn prepare_renderer(&self, renderer: &mut Renderer) -> Result<(), EglError> {
        let _timing = crate::diagnostics::scope("graphics.prepare");
        self.api.make_current(self.display, Some(self.pbuffer), Some(self.pbuffer), Some(self.context))?;
        let mut owned = self.skia.borrow_mut();
        let context = owned.as_mut().ok_or_else(|| EglError::Init("Skia context".into()))?;
        let native = self.native_context(context)?;
        native.borrow_mut().set_resource_owner(0);
        let mut surface = gpu::surfaces::render_target(context, gpu::Budgeted::Yes,
            &sk::ImageInfo::new_n32_premul((64,64), None), None,
            gpu::SurfaceOrigin::BottomLeft, None, None, None)
            .ok_or_else(|| EglError::Init("effect preparation target".into()))?;
        let backend = gpu::surfaces::get_backend_render_target(&mut surface,
            sk::surface::BackendHandleAccess::FlushWrite).ok_or_else(|| EglError::Init("effect preparation framebuffer".into()))?;
        let fbo = backend.gl_framebuffer_info().ok_or_else(|| EglError::Init("effect preparation GL target".into()))?.fboid;
        let mut root = crate::scene::Node::new(Rect::new(16.,16.,32.,32.), crate::scene::Content::Rect(crate::scene::Rectangle {
            radii:[8.;4], fill:crate::scene::Fill::Solid(crate::animation::Color::rgba(0.3,0.4,0.5,0.8)), ..Default::default()
        }));
        root.effects.multi_effect = Some(crate::effects::MultiEffect {shadow_enabled:true, ..Default::default()});
        let mut group = crate::scene::Node::new(Rect::new(0.,0.,64.,64.), crate::scene::Content::Group);
        group.native_materials = Some(true);
        group.add(root);
        // Control-panel chrome also uses the masked compose variant. Exercise
        // that real material here; lazy compilation must not land on its first
        // reveal (12ms measured for this program alone on NVIDIA).
        let mut masked = crate::scene::Node::new(Rect::new(16.,16.,32.,32.),
            crate::scene::Content::Rect(crate::scene::Rectangle {
                radii: [8.;4],
                fill: crate::scene::Fill::Solid(crate::animation::Color::rgba(0.3,0.4,0.5,0.8)),
                ..Default::default()
            }));
        masked.effects.multi_effect = Some(crate::effects::MultiEffect {
            shadow_enabled: true,
            mask: Some(crate::effects::Mask { source: crate::effects::MaskSource::Rect { radius: 8. }, ..Default::default() }),
            ..Default::default()
        });
        group.add(masked);
        // Calendar/panel pictures use colorized A8 masks through Skia rather
        // than the native text material. Its shader is compiled at submission.
        let alpha = sk::images::raster_from_data(
            &sk::ImageInfo::new((8,8), sk::ColorType::Alpha8, sk::AlphaType::Premul, None),
            sk::Data::new_copy(&[173u8;64]), 8).ok_or(EglError::Render(crate::render::RenderError::Skia))?;
        for opacity in [1.,0.75] {
            let mut recorder = sk::PictureRecorder::new();
            let canvas = recorder.begin_recording(sk::Rect::from_xywh(0.,0.,8.,8.), false);
            let mut tint = sk::Paint::default();
            tint.set_color4f(sk::Color4f::new(0.3,0.4,0.8,opacity), None);
            canvas.draw_image_rect(&alpha, None, sk::Rect::from_xywh(0.,0.,8.,8.), &tint);
            let picture = recorder.finish_recording_as_picture(None).ok_or(EglError::Render(crate::render::RenderError::Skia))?;
            group.add(crate::scene::Node::new(Rect::new(4.,4.,8.,8.), crate::scene::Content::Picture(picture.clone())));
            let mut scaled = crate::scene::Node::new(Rect::new(8.,8.,8.,8.), crate::scene::Content::Picture(picture.clone()));
            scaled.transform = sk::Matrix::scale((0.75,0.75)); group.add(scaled);
            let mut faded = crate::scene::Node::new(Rect::new(12.,8.,8.,8.), crate::scene::Content::Picture(picture));
            faded.opacity = 0.5; faded.opacity_mode = crate::scene::OpacityMode::Group;
            faded.clip = Some([2.;4]);
            group.add(faded);
        }
        let image = sk::images::raster_from_data(
            &sk::ImageInfo::new((8,8), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None),
            sk::Data::new_copy(&[127u8;256]), 32).ok_or(EglError::Render(crate::render::RenderError::Skia))?;
        for opacity in [1.,0.5] {
            let mut n = crate::scene::Node::new(Rect::new(16.,4.,8.,8.), crate::scene::Content::Image(image.clone()));
            n.opacity = opacity; group.add(n);
        }
        {
            let _target = crate::native_gl::target(native.clone(), surface.canvas(), fbo, (64,64));
            renderer.paint(surface.canvas(), &Scene::new(group), 1., Rect::new(0.,0.,64.,64.))?;
        }
        context.flush_and_submit();
        // A startup barrier keeps compile/upload execution out of the reveal.
        native.borrow().finish();
        context.reset(None);
        drop(surface);
        drop(owned);
        self.purge_scratch();
        Ok(())
    }
    /// Prepare a hidden component on the production context before it accepts
    /// input. No native window, mapped buffer or callback is created.
    pub(crate) fn prepare_scene(&self, renderer: &mut Renderer, scene: &Scene, logical: (f32, f32), scale: f64) -> Result<(), EglError> {
        let _timing = crate::diagnostics::scope("graphics.prepare_scene");
        let size = physical_size(logical.0, logical.1, scale)?;
        self.api.make_current(self.display, Some(self.pbuffer), Some(self.pbuffer), Some(self.context))?;
        let mut owned = self.skia.borrow_mut();
        let context = owned.as_mut().ok_or_else(|| EglError::Init("Skia context".into()))?;
        context.reset(None);
        let native = self.native_context(context)?;
        native.borrow_mut().set_resource_owner(0);
        {
            let mut surface = gpu::surfaces::render_target(context, gpu::Budgeted::Yes,
                &sk::ImageInfo::new_n32_premul(size, None), None,
                gpu::SurfaceOrigin::BottomLeft, None, None, None)
                .ok_or_else(|| EglError::Init("component preparation target".into()))?;
            let backend = gpu::surfaces::get_backend_render_target(&mut surface,
                sk::surface::BackendHandleAccess::FlushWrite)
                .ok_or_else(|| EglError::Init("component preparation framebuffer".into()))?;
            let fbo = backend.gl_framebuffer_info().ok_or_else(|| EglError::Init("component preparation GL target".into()))?.fboid;
            {
                let _target = crate::native_gl::target(native.clone(), surface.canvas(), fbo, size);
                renderer.paint(surface.canvas(), scene, scale, Rect::new(0., 0., logical.0, logical.1))?;
            }
            context.flush_and_submit();
            native.borrow().finish();
        }
        drop(owned);
        // Preparation must not leave a closed component's scratch target resident.
        self.purge_scratch();
        Ok(())
    }
    /// GL driver used by this production EGL context, for capture provenance.
    pub fn renderer_name(&self) -> Option<String> {
        self.api
            .make_current(
                self.display,
                Some(self.pbuffer),
                Some(self.pbuffer),
                Some(self.context),
            )
            .ok()?;
        let address = self.api.get_proc_address("glGetString")?;
        // SAFETY: GL specifies this ABI; a current context is established above.
        let get = unsafe {
            std::mem::transmute::<extern "system" fn(), unsafe extern "system" fn(u32) -> *const u8>(
                address,
            )
        };
        let ptr = unsafe { get(0x1f01) };
        if ptr.is_null() {
            None
        } else {
            Some(
                unsafe { std::ffi::CStr::from_ptr(ptr.cast()) }
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
    pub fn new(connection: &Connection) -> Result<Rc<Self>, EglError> {
        // SAFETY: the system libwayland backend is enabled; the connection remains owned by App.
        let api = unsafe { Api::load_required() }.map_err(|e| EglError::Init(e.to_string()))?;
        let display = unsafe { api.get_display(connection.backend().display_ptr() as *mut c_void) }
            .ok_or_else(|| EglError::Init("no EGL display".into()))?;
        api.initialize(display)?;
        // Keep partial initialization exception-safe, including failures while creating Skia.
        struct Guard<'a> {
            api: &'a Api,
            display: egl::Display,
            context: Option<egl::Context>,
            pbuffer: Option<egl::Surface>,
            armed: bool,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if self.armed {
                    let _ = self.api.make_current(self.display, None, None, None);
                    if let Some(s) = self.pbuffer {
                        let _ = self.api.destroy_surface(self.display, s);
                    }
                    if let Some(c) = self.context {
                        let _ = self.api.destroy_context(self.display, c);
                    }
                    let _ = self.api.terminate(self.display);
                }
            }
        }
        let mut guard = Guard {
            api: &api,
            display,
            context: None,
            pbuffer: None,
            armed: true,
        };
        api.bind_api(egl::OPENGL_ES_API)?;
        let config = api
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::WINDOW_BIT | egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    egl::STENCIL_SIZE,
                    8,
                    egl::NONE,
                ],
            )?
            .ok_or_else(|| EglError::Init("no RGBA8 EGL config".into()))?;
        let context_attributes = vec![egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];
        #[cfg(debug_assertions)]
        let context_attributes = if std::env::var_os("HYPRV_GPU_ROBUST_CONTEXT").is_some()
            && std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() == Ok("1") {
            let extensions = api.query_string(Some(display), egl::EXTENSIONS)?;
            if !extensions.to_string_lossy().split_whitespace().any(|e| e == "EGL_EXT_create_context_robustness") {
                return Err(EglError::Init("private robust-context probe requires EGL_EXT_create_context_robustness".into()));
            }
            // EGL_EXT_create_context_robustness: ROBUST_ACCESS=true,
            // RESET_NOTIFICATION_STRATEGY=LOSE_CONTEXT_ON_RESET. No device reset.
            vec![egl::CONTEXT_CLIENT_VERSION, 2, 0x30BF, egl::TRUE as i32, 0x3138, 0x31BF, egl::NONE]
        } else { context_attributes };
        let context = api.create_context(
            display,
            config,
            None,
            &context_attributes,
        )?;
        guard.context = Some(context);
        let pbuffer = api.create_pbuffer_surface(
            display,
            config,
            &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE],
        )?;
        guard.pbuffer = Some(pbuffer);
        api.make_current(display, Some(pbuffer), Some(pbuffer), Some(context))?;
        let interface = gpu::gl::Interface::new_load_with(|name| {
            api.get_proc_address(name)
                .map_or(std::ptr::null(), |p| p as *const c_void)
        })
        .ok_or_else(|| EglError::Init("Skia GL interface".into()))?;
        let mut context_skia = gpu::direct_contexts::make_gl(interface, None)
            .ok_or_else(|| EglError::Init("Skia context".into()))?;
        // Live targets are locked and excluded from eviction. Bound unlocked GPU
        // cache residency across popup lifetimes; native textures have their own budget.
        context_skia.set_resource_cache_limit(8 * 1024 * 1024);
        let extensions = api
            .query_string(Some(display), egl::EXTENSIONS)?
            .to_string_lossy();
        let swap_name = if extensions
            .split_whitespace()
            .any(|e| e == "EGL_KHR_swap_buffers_with_damage")
        {
            Some("eglSwapBuffersWithDamageKHR")
        } else if extensions
            .split_whitespace()
            .any(|e| e == "EGL_EXT_swap_buffers_with_damage")
        {
            Some("eglSwapBuffersWithDamageEXT")
        } else {
            None
        };
        // SAFETY: both advertised extensions specify this exact EGL ABI.
        let swap_damage = swap_name
            .and_then(|name| api.get_proc_address(name))
            .map(|f| unsafe { std::mem::transmute::<extern "system" fn(), SwapDamage>(f) });
        let buffer_age = extensions.split_whitespace().any(|e| matches!(e,"EGL_EXT_buffer_age"|"EGL_KHR_partial_update"));
        let set_damage = extensions.split_whitespace().any(|e|e=="EGL_KHR_partial_update")
            .then(|| api.get_proc_address("eglSetDamageRegionKHR")).flatten()
            .map(|f| unsafe {std::mem::transmute::<extern "system" fn(),SetDamage>(f)});
        guard.armed = false;
        drop(guard);
        Ok(Rc::new(Self {
            api,
            display,
            config,
            context,
            pbuffer,
            skia: RefCell::new(Some(context_skia)),
            native_text: RefCell::new(None),
            swap_damage,
            buffer_age,
            set_damage,
            _connection: connection.clone(),
            #[cfg(debug_assertions)]
            audit_lost: std::cell::Cell::new(false),
        }))
    }
}
impl Drop for SharedEgl {
    fn drop(&mut self) {
        let _ = self.api.make_current(
            self.display,
            Some(self.pbuffer),
            Some(self.pbuffer),
            Some(self.context),
        );
        self.native_text.get_mut().take();
        self.skia.get_mut().take();
        let _ = self.api.make_current(self.display, None, None, None);
        let _ = self.api.destroy_surface(self.display, self.pbuffer);
        let _ = self.api.destroy_context(self.display, self.context);
        let _ = self.api.terminate(self.display);
    }
}
pub struct EglWindow {
    native: Option<wayland_egl::WlEglSurface>,
    wl_surface: WlSurface,
    shared: Rc<SharedEgl>,
    surface: Option<egl::Surface>,
    size: (i32, i32),
    retained: Option<sk::Surface>,
    damage_history: BufferDamage,
    interval_set: bool,
    full_redraw: bool,
    /// Scene point drawn at this window's origin (a sparse-layer region).
    origin: (f32, f32),
    resource_owner: u64,
}
impl EglWindow {
    pub fn new(
        shared: Rc<SharedEgl>,
        surface: &WlSurface,
        size: (i32, i32),
    ) -> Result<Self, EglError> {
        let surface_clone = surface.clone();
        let native = wayland_egl::WlEglSurface::new(surface.id(), size.0, size.1)
            .map_err(|e| EglError::Init(e.to_string()))?;
        // SAFETY: native wl_egl_window remains alive until after EGL surface destruction.
        let surface = unsafe {
            shared.api.create_window_surface(
                shared.display,
                shared.config,
                native.ptr() as *mut c_void,
                None,
            )
        }?;
        Ok(Self {
            shared,
            surface: Some(surface),
            native: Some(native),
            wl_surface: surface_clone,
            size,
            retained: None,
            damage_history: BufferDamage::default(),
            interval_set: false,
            full_redraw: false,
            origin: (0., 0.),
            resource_owner: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
        })
    }
    /// Physical size of the buffers this window submits.
    pub fn size(&self) -> (i32, i32) {
        self.size
    }
    pub fn set_full_redraw(&mut self, enabled: bool) {
        if self.full_redraw != enabled {
            self.retained=None;
            self.damage_history.clear();
            self.full_redraw=enabled;
        }
    }
    /// Present the scene region starting at `origin` (logical, on the device grid).
    pub fn set_origin(&mut self, origin: (f32, f32)) {
        self.origin = origin;
    }
    pub(crate) fn capture(&self, renderer: &mut Renderer, scene: &Scene,
        logical: (f32,f32), scale: f64, path: &std::path::Path) -> Result<(), EglError> {
        self.shared.capture_scene(renderer,scene,logical,scale,self.resource_owner,path)
    }
    /// Retire a static direct-window swapchain while the compositor keeps the
    /// last committed buffer. The layer/viewport and its pixels remain mapped.
    /// Recreate only on damage; there is no timer, readback, or extra commit.
    pub fn release_idle_drawable(&mut self) -> Result<(), EglError> {
        if !self.full_redraw || self.surface.is_none() { return Ok(()); }
        let _timing = crate::diagnostics::scope("egl.idle_retire");
        self.shared.api.make_current(self.shared.display, Some(self.shared.pbuffer),
            Some(self.shared.pbuffer), Some(self.shared.context))?;
        self.retained = None;
        self.damage_history.clear();
        self.interval_set = false;
        if let Some(surface) = self.surface.take() {
            self.shared.api.destroy_surface(self.shared.display, surface)?;
        }
        self.native.take();
        self.shared.purge_scratch();
        crate::diagnostics::count("egl.idle_drawable_retired");
        Ok(())
    }
    /// Encode the retained RGBA8 target without filesystem transport.
    pub fn png_bytes(&mut self) -> Result<Vec<u8>, EglError> {
        let shared = &self.shared;
        shared.api.make_current(shared.display, self.surface, self.surface, Some(shared.context))?;
        let image = self.retained.as_mut().ok_or_else(|| EglError::Init("surface has not rendered".into()))?.image_snapshot();
        let mut context = shared.skia.borrow_mut();
        let data = image.encode(context.as_mut(), sk::EncodedImageFormat::PNG, None)
            .ok_or_else(|| EglError::Init("GPU PNG encoding".into()))?;
        Ok(data.as_bytes().to_vec())
    }
    /// Read the retained target into a CPU image while its EGL context is current.
    pub fn snapshot(&mut self) -> Result<sk::Image, EglError> {
        let shared = &self.shared;
        shared.api.make_current(shared.display, self.surface, self.surface, Some(shared.context))?;
        let image = self.retained.as_mut().ok_or_else(|| EglError::Init("surface has not rendered".into()))?.image_snapshot();
        let mut context = shared.skia.borrow_mut();
        image.make_raster_image(context.as_mut(), sk::image::CachingHint::Disallow)
            .ok_or_else(|| EglError::Init("GPU image readback".into()))
    }
    /// Capture the retained GPU target actually submitted to this EGL surface.
    pub fn png(&mut self, path: &std::path::Path) -> Result<(), EglError> {
        std::fs::write(path, self.png_bytes()?).map_err(|e| EglError::Init(e.to_string()))
    }
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        scene: &Scene,
        logical: (f32, f32),
        scale: f64,
        damage: Rect,
    ) -> Result<(), EglError> {
        self.draw_regions(renderer,scene,logical,scale,vec![damage])
    }
    /// Disjoint damage clips retain unchanged pixels and avoid painting intervening
    /// content when separate values change at opposite ends of a surface.
    pub fn draw_regions(&mut self,renderer:&mut Renderer,scene:&Scene,logical:(f32,f32),scale:f64,damages:Vec<Rect>)->Result<(),EglError>{
        self.draw_regions_mode(renderer,scene,logical,scale,damages,true)
    }
    #[cfg(test)]
    pub(crate) fn draw_regions_no_swap(&mut self,renderer:&mut Renderer,scene:&Scene,logical:(f32,f32),scale:f64,damages:Vec<Rect>)->Result<(),EglError>{
        self.draw_regions_mode(renderer,scene,logical,scale,damages,false)
    }
    fn draw_regions_mode(&mut self,renderer:&mut Renderer,scene:&Scene,logical:(f32,f32),scale:f64,damages:Vec<Rect>,present:bool)->Result<(),EglError>{
        let mut damage_set=crate::geometry::Damage::default();
        for damage in damages {
            let(x,y,w,h)=damage.physical(scale);
            damage_set.add(Rect::new(x as f32/scale as f32,y as f32/scale as f32,w as f32/scale as f32,h as f32/scale as f32));
        }
        let mut damages=damage_set.take_regions(Rect::new(0.,0.,logical.0,logical.1));
        let Some(mut damage)=damages.iter().copied().reduce(Rect::union) else{return Ok(())};
        let _timing = crate::diagnostics::scope("egl.draw");
        #[cfg(debug_assertions)]
        if self.shared.audit_lost.get() { return Err(egl::Error::ContextLost.into()); }
        let size = physical_size(logical.0, logical.1, scale)?;
        let shared = &self.shared;
        let resized = size != self.size;
        if resized || self.surface.is_none() {
            // A driver may have acquired the next back buffer at the old size.
            // Merely resizing wl_egl_window (even with a pbuffer rebind) does not
            // guarantee that the FIRST submitted buffer uses the new extent.
            // Recreate the EGL drawable, as Engine already does for output
            // configure/rotation, so direct EglWindow users have the same contract.
            shared.api.make_current(shared.display, Some(shared.pbuffer),
                Some(shared.pbuffer), Some(shared.context))?;
            if let Some(old) = self.surface {
                shared.api.destroy_surface(shared.display, old)?;
                self.surface = None;
            }
            if self.native.is_none() {
                self.native = Some(wayland_egl::WlEglSurface::new(self.wl_surface.id(),size.0,size.1)
                    .map_err(|error|EglError::Init(error.to_string()))?);
            }
            self.native.as_ref().unwrap().resize(size.0, size.1, 0, 0);
            self.interval_set=false;
            // SAFETY: native wl_egl_window remains live; its old EGL surface
            // was destroyed before creating another surface for this window.
            self.surface = Some(unsafe { shared.api.create_window_surface(
                shared.display, shared.config, self.native.as_ref().unwrap().ptr() as *mut c_void, None,
            ) }?);
        }
        let surface = self.surface.ok_or_else(|| EglError::Init("EGL drawable unavailable after failed resize".into()))?;
        {
            let _timing=crate::diagnostics::scope("egl.bind");
            if shared.api.get_current_context()!=Some(shared.context)
                || shared.api.get_current_surface(egl::DRAW)!=Some(surface)
                || shared.api.get_current_surface(egl::READ)!=Some(surface) {
                shared.api.make_current(shared.display,Some(surface),Some(surface),Some(shared.context))?;
            }
            // The interval is surface state, not a per-frame operation. A
            // recreated drawable below gets its own initial interval setting.
            if !self.interval_set {shared.api.swap_interval(shared.display,0)?;self.interval_set=true;}
        }
        let mut context = shared.skia.borrow_mut();
        let context = context.as_mut().unwrap();
        if let Some(native) = shared.native_text.borrow().as_ref() {
            if renderer.sync_native_caches(&mut native.borrow_mut()) { context.reset(None); }
        }
        if resized {
            let shrank = size.0 < self.size.0 || size.1 < self.size.1;
            self.size = size;
            self.retained = None;
            self.damage_history.clear();
            context.reset(None);
            if shrank {
                // Direct users can resize without recreating the EGL window.
                // Do not retain the expanded popup's scratch allocation.
                context.purge_unlocked_resources(gpu::PurgeResourceOptions::ScratchResourcesOnly);
            }
        }
        if self.full_redraw {
            damage=Rect::new(0.,0.,logical.0,logical.1);
            damages=vec![damage];
        }
        if self.retained.is_none() {
            let _timing = crate::diagnostics::scope("egl.retained_alloc");
            self.retained = if self.full_redraw {
                let backend=gpu::backend_render_targets::make_gl(size,0,8,
                    gpu::gl::FramebufferInfo {fboid:0,format:gpu::gl::Format::RGBA8.into(),..Default::default()});
                gpu::surfaces::wrap_backend_render_target(context,&backend,gpu::SurfaceOrigin::BottomLeft,
                    sk::ColorType::RGBA8888,None,None)
            } else {gpu::surfaces::render_target(
                context,
                gpu::Budgeted::Yes,
                &sk::ImageInfo::new_n32_premul(size,None),
                None,
                None,
                None,
                None,
                None,
            )};
            damage = Rect::new(0., 0., logical.0, logical.1);
            damages.clear();damages.push(damage);
        }
        // Buffer damage repairs this recycled back buffer; surface damage below
        // describes only this frame's changes to the compositor. Age 0, lost
        // history, or unsupported age queries require a complete copy.
        let age = if present && shared.buffer_age {
            let _timing = crate::diagnostics::scope("egl.buffer_age");
            shared.api.query_surface(shared.display,surface,0x313d).unwrap_or(0)
        } else {0};
        crate::diagnostics::count(match age {0=>"egl.age0",1=>"egl.age1",2=>"egl.age2",_=>"egl.age_other"});
        let repair = self.damage_history.repair(age,damage,Rect::new(0.,0.,logical.0,logical.1));
        let repair_device = device_damage(repair,scale,size);
        if let Some(set_damage)=shared.set_damage.filter(|_|present) {
            // Only an optimization hint: failure keeps the complete age-safe
            // repair plan and never discards required drawing.
            unsafe {set_damage(shared.display.as_ptr(),surface.as_ptr(),repair_device.as_ptr(),1);}
        }
        crate::diagnostics::count_by("egl.repair_pixels",(repair_device[2] as u64)*(repair_device[3] as u64));
        let retained = self
            .retained
            .as_mut()
            .ok_or_else(|| EglError::Init("GPU retained target".into()))?;
        // Diagnostic A/B switch; no alternative runtime backend or Qt dependency.
        if std::env::var_os("HYPRV_NATIVE_GL_DISABLE").is_some() || !renderer.native_gl_required(scene) {
            for damage in &damages {renderer.paint_at(retained.canvas(), scene, scale, self.origin, *damage)?;}
        } else {
            shared.native_context(context)?.borrow_mut().set_resource_owner(self.resource_owner);
            // FlushWrite resolves pending work/COW before exposing this framebuffer.
            let backend = gpu::surfaces::get_backend_render_target(
                retained,
                sk::surface::BackendHandleAccess::FlushWrite,
            )
            .ok_or_else(|| EglError::Init("native text target".into()))?;
            let fbo = backend
                .gl_framebuffer_info()
                .ok_or_else(|| EglError::Init("native text GL framebuffer".into()))?
                .fboid;
            {
                let _target = crate::native_gl::target(
                    shared.native_text.borrow().as_ref().unwrap().clone(),
                    retained.canvas(),
                    fbo,
                    size,
                );
                for damage in &damages {renderer.paint_at(retained.canvas(), scene, scale, self.origin, *damage)?;}
            }
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("HYPRV_GPU_STRESS_PURGE_AFTER_PAINT").is_some()
            && std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() == Ok("1") {
            // Deliberately evict between paint and presentation, while retained
            // targets and queued work are live. Debug stress only.
            context.purge_unlocked_resources(gpu::PurgeResourceOptions::AllResources);
            if let Some(native) = shared.native_text.borrow().as_ref() { native.borrow_mut().clear_caches(); }
            context.reset(None);
        }
        if !present {context.flush(None);return Ok(());}
        if !self.full_redraw {
            let _timing = crate::diagnostics::scope("egl.copy");
            let native = shared.native_text.borrow().clone();
            let blit = native.as_ref().is_some_and(|n| n.borrow().can_blit())
                && std::env::var_os("HYPRV_SKIA_WINDOW_COPY").is_none();
            if blit {
                let native = native.unwrap();
                let verify = std::env::var_os("HYPRV_VERIFY_FRAME_COPY").is_some();
                let reference = if verify {
                    skia_window_copy(context, retained, size)?;
                    Some(native.borrow().window_pixels(size).map_err(EglError::Init)?)
                } else { None };
                let backend = {
                    let _timing = crate::diagnostics::scope("egl.source_target");
                    gpu::surfaces::get_backend_render_target(retained,
                        sk::surface::BackendHandleAccess::FlushRead)
                        .ok_or_else(|| EglError::Init("retained copy target".into()))?
                };
                let source = backend.gl_framebuffer_info().ok_or_else(|| EglError::Init("retained GL target".into()))?.fboid;
                { let _timing = crate::diagnostics::scope("egl.source_flush"); context.flush(None); }
                {
                    let _timing = crate::diagnostics::scope("egl.blit");
                    native.borrow().blit_to_window(source, repair_device).map_err(EglError::Init)?;
                }
                context.reset(None);
                if let Some(reference) = reference {
                    let actual = native.borrow().window_pixels(size).map_err(EglError::Init)?;
                    context.reset(None);
                    if reference != actual { return Err(EglError::Init("native window copy differs from Skia RGBA8 reference".into())); }
                    crate::diagnostics::count("egl.copy.exact");
                }
            } else {
                skia_window_copy(context, retained, size)?;
            }
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("HYPRV_GPU_SELF_CHECK").is_some() {
            context.flush_and_submit();
            // Direct presentation has no independent retained target: freeze
            // expected bytes before the fault hook/window readback instead.
            let direct_reference=if self.full_redraw {
                Some(frame_audit::reference(shared,retained,size)?)
            }else{None};
            if std::env::var_os("HYPRV_GPU_SELF_CHECK_INJECT").is_some() {
                frame_audit::inject_copy_fault(shared);
            }
            let result = if let Some(expected)=direct_reference {
                frame_audit::check_pixels(shared,&expected,size)
            }else{frame_audit::check(shared, retained, size)};
            context.reset(None);
            result?;
        }
        let _swap_timing = crate::diagnostics::scope("egl.swap");
        // The repaired buffer is complete; expose only current surface damage.
        if let Some(swap) = shared.swap_damage {
            let (x, y, w, h) = damage.physical(scale);
            let x0 = x.clamp(0, size.0);
            let y0 = y.clamp(0, size.1);
            let x1 = (x + w).clamp(0, size.0);
            let y1 = (y + h).clamp(0, size.1);
            let rectangle = [x0, size.1 - y1, (x1 - x0).max(0), (y1 - y0).max(0)];
            // SAFETY: display/surface belong to the current EGL context; the slice is live.
            let result = unsafe {
                swap(
                    shared.display.as_ptr(),
                    surface.as_ptr(),
                    rectangle.as_ptr(),
                    1,
                )
            };
            if result != egl::TRUE {
                return Err(shared
                    .api
                    .get_error()
                    .unwrap_or(egl::Error::BadSurface)
                    .into());
            }
        } else {
            shared.api.swap_buffers(shared.display, surface)?;
        }
        self.damage_history.push(damage);
        Ok(())
    }
}
fn device_damage(damage:Rect,scale:f64,size:(i32,i32))->[i32;4] {
    let(x,y,w,h)=damage.physical(scale);
    let x0=x.clamp(0,size.0);let y0=y.clamp(0,size.1);
    let x1=(x+w).clamp(0,size.0);let y1=(y+h).clamp(0,size.1);
    [x0,size.1-y1,(x1-x0).max(0),(y1-y0).max(0)]
}
#[derive(Default)]
struct BufferDamage(std::collections::VecDeque<Rect>);
impl BufferDamage {
    fn clear(&mut self){self.0.clear();}
    fn repair(&self,age:i32,current:Rect,extent:Rect)->Rect {
        if age<=0 || age as usize>self.0.len()+1 {return extent;}
        self.0.iter().take(age as usize-1).fold(current,|damage,previous|damage.union(*previous))
    }
    fn push(&mut self,damage:Rect){
        if self.0.len()==8{self.0.pop_back();}
        self.0.push_front(damage);
    }
}
#[cfg(test)]
mod buffer_damage_tests {
    use super::*;
    #[test]
    fn repair_includes_every_intervening_frame_and_falls_back_when_unknown(){
        let extent=Rect::new(0.,0.,100.,40.);let a=Rect::new(1.,1.,3.,3.);let b=Rect::new(40.,1.,3.,3.);let c=Rect::new(80.,1.,3.,3.);
        let mut history=BufferDamage::default();
        assert_eq!(history.repair(0,a,extent),extent);
        assert_eq!(history.repair(2,a,extent),extent);
        history.push(a);history.push(b);
        assert_eq!(history.repair(1,c,extent),c);
        assert_eq!(history.repair(2,c,extent),b.union(c));
        assert_eq!(history.repair(3,c,extent),a.union(b).union(c));
        assert_eq!(history.repair(4,c,extent),extent);
        history.clear();assert_eq!(history.repair(2,c,extent),extent);
        assert_eq!(device_damage(Rect::new(-1.,1.25,4.,3.),2.,(100,40)),[0,31,6,7]);
    }
}
fn skia_window_copy(context: &mut gpu::DirectContext, retained: &mut sk::Surface, size: (i32,i32)) -> Result<(), EglError> {
        let image = retained.image_snapshot();
        let backend = gpu::backend_render_targets::make_gl(
            size,
            None,
            8,
            gpu::gl::FramebufferInfo {
                fboid: 0,
                format: 0x8058,
                ..Default::default()
            },
        );
        let mut output = gpu::surfaces::wrap_backend_render_target(
            context,
            &backend,
            gpu::SurfaceOrigin::BottomLeft,
            sk::ColorType::RGBA8888,
            None,
            None,
        )
        .ok_or_else(|| EglError::Init("EGL framebuffer".into()))?;
        let mut paint = sk::Paint::default();
        paint.set_blend_mode(sk::BlendMode::Src);
        output.canvas().draw_image(&image, (0., 0.), Some(&paint));
        context.flush_and_submit();
        drop(output);
    Ok(())
}
impl Drop for EglWindow {
    fn drop(&mut self) {
        let _timing = crate::diagnostics::scope("egl.window_drop");
        let s = &self.shared;
        let _ = s.api.make_current(
            s.display,
            Some(self.surface.unwrap_or(s.pbuffer)),
            Some(self.surface.unwrap_or(s.pbuffer)),
            Some(s.context),
        );
        self.retained.take();
        let current = s
            .api
            .make_current(s.display, Some(s.pbuffer), Some(s.pbuffer), Some(s.context));
        if current.is_ok() {
            if let Some(native) = s.native_text.borrow().as_ref() { native.borrow_mut().release_owner(self.resource_owner); }
            s.purge_scratch();
        }
        let _destroy = crate::diagnostics::scope("egl.surface_destroy");
        if let Some(surface) = self.surface.take() {
            let _ = s.api.destroy_surface(s.display, surface);
        }
    }
}

#[cfg(test)]
mod cache_flush_tests {
    use super::*;
    use crate::scene::{Content, Node, Scene};

    #[test]
    #[ignore = "requires private Weston; run tests/run-renderer-regressions.sh"]
    fn production_preparation_compiles_masked_popup_material() {
        assert_eq!(std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(), Ok("1"));
        let shared = SharedEgl::new(&Connection::connect_to_env().unwrap()).unwrap();
        shared.prepare_renderer(&mut Renderer::new()).unwrap();
        assert!(shared.native_text.borrow().as_ref().unwrap().borrow().review_masked_compose(),
            "masked control material must execute before input; a transparent warmup node is culled");
    }

    #[test]
    #[ignore = "requires private Weston; run tests/run-renderer-regressions.sh"]
    fn closed_component_scratch_is_released_without_evicting_live_targets() {
        assert_eq!(std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(), Ok("1"));
        let connection = Connection::connect_to_env().unwrap();
        let shared = SharedEgl::new(&connection).unwrap();
        let live = {
            let mut skia = shared.skia.borrow_mut();
            let context = skia.as_mut().unwrap();
            let allocate = |context: &mut gpu::DirectContext, side| {
                gpu::surfaces::render_target(context, gpu::Budgeted::Yes,
                    &sk::ImageInfo::new_n32_premul((side, side), None), None,
                    None, None, None, None).unwrap()
            };
            let mut live = allocate(context, 128);
            live.canvas().clear(sk::Color::RED);
            let mut popup = allocate(context, 1024);
            popup.canvas().clear(sk::Color::BLUE);
            context.flush_and_submit();
            drop(popup);
            live
        };
        let before = shared.skia.borrow().as_ref().unwrap().resource_cache_usage().resource_bytes;
        shared.purge_scratch();
        let after = shared.skia.borrow().as_ref().unwrap().resource_cache_usage().resource_bytes;
        assert!(before >= after + 1024 * 1024 * 4, "closed popup target must be reclaimed: {before} -> {after}");
        let mut live = live;
        let mut pixel = [0_u8; 4];
        assert!(live.read_pixels(&sk::ImageInfo::new((1, 1), sk::ColorType::RGBA8888,
            sk::AlphaType::Premul, None), &mut pixel, 4, (0, 0)));
        assert_eq!(pixel, [255, 0, 0, 255], "another output's live target must survive");
    }

    #[test]
    #[ignore = "requires private Weston; run tests/run-renderer-regressions.sh"]
    fn coordinated_flush_releases_native_caches_and_restores_egl_binding() {
        assert_eq!(std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(),Ok("1"),"use the isolated test launcher");
        let connection=Connection::connect_to_env().unwrap();
        let shared=SharedEgl::new(&connection).unwrap();
        let gl=unsafe{glow::Context::from_loader_function(|name|shared.api.get_proc_address(name).map_or(std::ptr::null(),|p|p as *const c_void))};
        let native=Rc::new(RefCell::new(unsafe{crate::native_gl::Context::new(gl).unwrap()}));
        native.borrow().review_lazy_programs();
        *shared.native_text.borrow_mut()=Some(native.clone());
        let mut renderer=Renderer::new();
        let mut node=Node::new(Rect::new(0.,0.,16.,16.),Content::Rect(Default::default()));
        node.native_materials=Some(true);
        let scene=Scene::new(node);
        for _ in 0..2 {
            {
                let mut skia=shared.skia.borrow_mut();
                let context=skia.as_mut().unwrap();
                context.reset(None);
                let mut surface=gpu::surfaces::render_target(context,gpu::Budgeted::Yes,
                    &sk::ImageInfo::new_n32_premul((16,16),None),None,None,None,None,None).unwrap();
                let backend=gpu::surfaces::get_backend_render_target(&mut surface,sk::surface::BackendHandleAccess::FlushWrite).unwrap();
                let _target=crate::native_gl::target(native.clone(),surface.canvas(),backend.gl_framebuffer_info().unwrap().fboid,(16,16));
                renderer.paint(surface.canvas(),&scene,1.,Rect::new(0.,0.,16.,16.)).unwrap();
            }
            assert!(native.borrow().review_stats().2>0,"must populate actual native mesh cache");
            let before=(shared.api.get_current_context(),shared.api.get_current_surface(egl::DRAW),shared.api.get_current_surface(egl::READ));
            shared.clear_caches(&mut renderer).unwrap();
            assert_eq!(native.borrow().review_stats(),(0,0,0,0));
            assert_eq!((shared.api.get_current_context(),shared.api.get_current_surface(egl::DRAW),shared.api.get_current_surface(egl::READ)),before);
        }
        shared.api.make_current(shared.display,None,None,None).unwrap();
        shared.clear_caches(&mut renderer).unwrap();
        assert!(shared.api.get_current_context().is_none(),"an unbound caller must remain unbound");
        // Last Context owner must be SharedEgl so its destructor makes EGL current.
        drop(native);
    }
}
