//! Opt-in debug diagnostics. No readback, reset simulation or fault injection in release builds.
use super::*;
use glow::HasContext;

fn loader(shared: &SharedEgl) -> glow::Context {
    // SAFETY: all callers establish this thread's owning EGL context first.
    unsafe {
        glow::Context::from_loader_function(|name| {
            shared
                .api
                .get_proc_address(name)
                .map_or(std::ptr::null(), |p| p as *const c_void)
        })
    }
}

/// Negative control: corrupt one back-buffer pixel, never the retained source.
pub(super) fn inject_copy_fault(shared: &SharedEgl) {
    if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() != Ok("1") {
        return;
    }
    let gl = loader(shared);
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(0, 0, 1, 1);
        gl.color_mask(true, true, true, true);
        gl.clear_color(1., 0., 1., 1.);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
}

pub(super) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

/// Compare exact premultiplied RGBA8 bytes before swap. This validates the actual
/// default framebuffer, not two encodings of the retained source. Readback is
/// intentionally synchronous and can hide a timing race: also run without it.
pub(super) fn check(
    shared: &SharedEgl,
    retained: &mut sk::Surface,
    size: (i32, i32),
) -> Result<(), EglError> {
    let expected=reference(shared,retained,size)?;
    check_pixels(shared,&expected,size)
}
pub(super) fn reference(shared:&SharedEgl,retained:&mut sk::Surface,size:(i32,i32))->Result<Vec<u8>,EglError>{
    let gl = loader(shared);
    let extensions = gl.supported_extensions();
    if extensions.contains("GL_KHR_robustness") || extensions.contains("GL_EXT_robustness") {
        let name = if extensions.contains("GL_KHR_robustness") {
            "glGetGraphicsResetStatusKHR"
        } else {
            "glGetGraphicsResetStatusEXT"
        };
        if let Some(address) = shared.api.get_proc_address(name) {
            // SAFETY: advertised robustness extension specifies this ABI.
            let query = unsafe {
                std::mem::transmute::<extern "system" fn(), unsafe extern "system" fn() -> u32>(
                    address,
                )
            };
            if unsafe { query() } != glow::NO_ERROR {
                return Err(egl::Error::ContextLost.into());
            }
        }
    }
    let mut expected = vec![0; size.0 as usize * size.1 as usize * 4];
    if !retained.read_pixels(
        &sk::ImageInfo::new(size, sk::ColorType::RGBA8888, sk::AlphaType::Premul, None),
        &mut expected,
        size.0 as usize * 4,
        (0, 0),
    ) {
        return Err(EglError::Init(
            "GPU self-check source readback failed".into(),
        ));
    }
    Ok(expected)
}
pub(super) fn check_pixels(shared:&SharedEgl,expected:&[u8],size:(i32,i32))->Result<(),EglError>{
    let gl=loader(shared);
    let mut actual = vec![0; expected.len()];
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        if gl.version().major >= 3 {
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 0);
            gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 0);
            gl.read_buffer(glow::BACK);
        }
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            size.0,
            size.1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut actual)),
        );
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(EglError::Init(format!(
                "GPU self-check GL error {error:#x}"
            )));
        }
    }
    let stride = size.0 as usize * 4;
    for y in 0..size.1 as usize / 2 {
        let (left, right) = actual.split_at_mut((size.1 as usize - 1 - y) * stride);
        left[y * stride..(y + 1) * stride].swap_with_slice(&mut right[..stride]);
    }
    let source = checksum(expected);
    let window = checksum(&actual);
    // Check bytes too: a checksum collision must not silence a mismatch.
    if actual != expected {
        if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() == Ok("1") {
            if let Some(directory) = std::env::var_os("HYPRV_GPU_SELF_CHECK_DUMP") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).map_err(|e| EglError::Init(e.to_string()))?;
                let stem = format!("{}x{}-{source:016x}", size.0, size.1);
                for (kind, data) in [("source", expected), ("window", actual.as_slice())] {
                    std::fs::write(directory.join(format!("{stem}-{kind}.rgba")), data)
                        .map_err(|e| EglError::Init(e.to_string()))?;
                }
            }
        }
        let first = expected
            .iter()
            .zip(&actual)
            .position(|(a, b)| a != b)
            .unwrap();
        return Err(EglError::Init(format!("GPU self-check mismatch: source={source:016x} window={window:016x} byte={first} size={}x{}", size.0, size.1)));
    }
    crate::diagnostics::count("egl.self_check.exact");
    if std::env::var_os("HYPRV_GPU_SELF_CHECK_LOG").is_some() {
        eprintln!(
            "GPU_SELF_CHECK exact source={source:016x} window={window:016x} size={}x{}",
            size.0, size.1
        );
    }
    Ok(())
}

impl SharedEgl {
    fn private_debug(&self) -> Result<(), EglError> {
        if std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref() != Ok("1") {
            return Err(EglError::Init(
                "fault injection requires an explicitly private test session".into(),
            ));
        }
        self.api.make_current(
            self.display,
            Some(self.pbuffer),
            Some(self.pbuffer),
            Some(self.context),
        )?;
        Ok(())
    }
    /// Private-test-only reset notification injection. It does not reset the GPU.
    /// Subsequent draws fail closed with EGL_CONTEXT_LOST until this owner and
    /// all of its windows are destroyed and a new owner is constructed.
    #[doc(hidden)]
    pub fn debug_simulate_context_loss(&self) -> Result<(), EglError> {
        self.private_debug()?;
        self.audit_lost.set(true);
        Ok(())
    }
    /// Exercise legal external GL state changes followed by Skia invalidation.
    #[doc(hidden)]
    pub fn debug_poison_gl_state(&self) -> Result<(), EglError> {
        self.private_debug()?;
        let gl = loader(self);
        unsafe {
            gl.viewport(0, 0, 1, 1);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(0, 0, 0, 0);
            gl.color_mask(false, false, false, false);
            gl.enable(glow::STENCIL_TEST);
            gl.enable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.active_texture(glow::TEXTURE0 + 7);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 8);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 8);
        }
        if let Some(context) = self.skia.borrow_mut().as_mut() {
            context.reset(None);
        }
        Ok(())
    }
    /// Bounded pressure: 32 MiB of live scratch targets under a 2 MiB cache
    /// budget, then release/purge. Existing retained targets remain referenced.
    #[doc(hidden)]
    pub fn debug_memory_pressure(&self) -> Result<usize, EglError> {
        self.private_debug()?;
        let mut owned = self.skia.borrow_mut();
        let context = owned.as_mut().unwrap();
        context.reset(None);
        let previous = context.resource_cache_limit();
        context.set_resource_cache_limit(2 * 1024 * 1024);
        let mut targets = Vec::new();
        let result = (|| {
            for i in 0..8 {
                let mut target = gpu::surfaces::render_target(
                    context,
                    gpu::Budgeted::Yes,
                    &sk::ImageInfo::new_n32_premul((1024, 1024), None),
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .ok_or_else(|| EglError::Init("pressure target allocation".into()))?;
                target.canvas().clear(sk::Color::from_rgb(i * 31, 127, 255));
                targets.push(target);
            }
            context.flush_and_submit();
            Ok(32 * 1024 * 1024)
        })();
        drop(targets);
        context.purge_unlocked_resources(gpu::PurgeResourceOptions::AllResources);
        context.set_resource_cache_limit(previous);
        result
    }
    /// Query actual driver robustness support; no reset is requested.
    #[doc(hidden)]
    pub fn debug_gl_capabilities(&self) -> Result<Vec<String>, EglError> {
        self.private_debug()?;
        let gl = loader(self);
        let mut extensions: Vec<_> = gl
            .supported_extensions()
            .iter()
            .filter(|s| s.contains("robust") || s.contains("sync"))
            .cloned()
            .collect();
        extensions.sort();
        if gl.supported_extensions().contains("GL_KHR_robustness") {
            unsafe {
                extensions.push(format!(
                    "actual_GL_CONTEXT_ROBUST_ACCESS={}",
                    gl.get_parameter_i32(0x90F3)
                ));
                extensions.push(format!(
                    "actual_GL_RESET_NOTIFICATION_STRATEGY={:#x}",
                    gl.get_parameter_i32(0x8256)
                ));
            }
        }
        extensions.extend(
            self.api
                .query_string(Some(self.display), egl::EXTENSIONS)?
                .to_string_lossy()
                .split_whitespace()
                .filter(|e| e.contains("robust"))
                .map(str::to_owned),
        );
        Ok(extensions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_client::{
        globals::{registry_queue_init, GlobalListContents},
        protocol::{wl_compositor::WlCompositor, wl_registry::WlRegistry, wl_surface::WlSurface},
        Dispatch, QueueHandle,
    };
    struct State;
    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wayland_client::protocol::wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    wayland_client::delegate_noop!(State: ignore WlCompositor);
    wayland_client::delegate_noop!(State: ignore WlSurface);
    #[test]
    fn checksum_is_order_sensitive_and_sees_one_bit() {
        assert_ne!(checksum(&[1, 2, 3, 4]), checksum(&[4, 3, 2, 1]));
        assert_ne!(checksum(&[1, 2, 3, 4]), checksum(&[1, 2, 3, 5]));
        assert_eq!(checksum(&[]), 0xcbf29ce484222325);
    }

    #[test]
    #[ignore = "requires private Weston and HYPRV_TEST_PRIVATE_WAYLAND=1"]
    fn actual_back_buffer_fault_is_detected_and_next_frame_recovers() {
        assert_eq!(
            std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(),
            Ok("1")
        );
        let mut target = crate::offscreen::GpuTarget::new().unwrap();
        let mut node = crate::scene::Node::new(
            Rect::new(0., 0., 32., 24.),
            crate::scene::Content::Rect(crate::scene::Rectangle {
                fill: crate::scene::Fill::Solid(crate::animation::Color::rgba(0., 1., 0., 1.)),
                ..Default::default()
            }),
        );
        node.native_materials = Some(true);
        let scene = Scene::new(node);
        let mut renderer = Renderer::new();
        std::env::set_var("HYPRV_GPU_SELF_CHECK", "1");
        target
            .render_png(&mut renderer, &scene, 32., 24., 1.)
            .unwrap();
        std::env::set_var("HYPRV_GPU_SELF_CHECK_INJECT", "1");
        let error = target
            .render_png(&mut renderer, &scene, 32., 24., 1.)
            .unwrap_err();
        std::env::remove_var("HYPRV_GPU_SELF_CHECK_INJECT");
        assert!(
            error.to_string().contains("GPU self-check mismatch"),
            "{error}"
        );
        target
            .render_png(&mut renderer, &scene, 32., 24., 1.)
            .unwrap();
        std::env::remove_var("HYPRV_GPU_SELF_CHECK");
    }

    #[test]
    #[ignore = "requires private Weston; run on both NVIDIA and Mesa"]
    fn first_buffer_after_same_window_resize_matches_cpu_and_retained() {
        assert_eq!(
            std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(),
            Ok("1")
        );
        let connection = Connection::connect_to_env().unwrap();
        let (globals, mut queue) = registry_queue_init::<State>(&connection).unwrap();
        let compositor: WlCompositor = globals.bind(&queue.handle(), 1..=4, ()).unwrap();
        let surface = compositor.create_surface(&queue.handle(), ());
        queue.roundtrip(&mut State).unwrap();
        let shared = SharedEgl::new(&connection).unwrap();
        let mut window = EglWindow::new(shared, &surface, (16, 16)).unwrap();
        let mut renderer = Renderer::new();
        let mut cpu = Renderer::new();
        cpu.set_offscreen_backend(crate::offscreen::Backend::Cpu);
        std::env::set_var("HYPRV_GPU_SELF_CHECK", "1");
        for (i, (logical, scale)) in [
            ((256., 160.), 1.),
            ((640., 64.), 1.),
            ((320., 240.), 1.25),
            ((640., 64.), 2.),
            ((256., 160.), 1.5),
            ((32., 24.), 1.),
            ((320., 240.), 2.),
        ]
        .into_iter()
        .enumerate()
        {
            let mut root = crate::scene::Node::new(
                Rect::new(0., 0., logical.0, logical.1),
                crate::scene::Content::Rect(crate::scene::Rectangle {
                    fill: crate::scene::Fill::Solid(crate::animation::Color::rgba(
                        if i % 2 == 0 { 1. } else { 0. },
                        0.5,
                        0.25,
                        1.,
                    )),
                    ..Default::default()
                }),
            );
            root.native_materials = Some(true);
            let scene = Scene::new(root);
            window
                .draw(
                    &mut renderer,
                    &scene,
                    logical,
                    scale,
                    Rect::new(0., 0., 1., 1.),
                )
                .unwrap();
            let actual = window.snapshot().unwrap();
            let mut expected = cpu.raster(&scene, logical.0, logical.1, scale).unwrap();
            let expected = expected.image_snapshot();
            let bytes = |image: &sk::Image| {
                let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
                assert!(image.read_pixels(
                    &sk::ImageInfo::new(
                        image.dimensions(),
                        sk::ColorType::RGBA8888,
                        sk::AlphaType::Premul,
                        None
                    ),
                    &mut pixels,
                    image.width() as usize * 4,
                    (0, 0),
                    sk::image::CachingHint::Disallow
                ));
                pixels
            };
            assert_eq!(bytes(&actual), bytes(&expected), "resize step {i}");
        }
        std::env::remove_var("HYPRV_GPU_SELF_CHECK");
        drop(window);
        surface.destroy();
    }
    #[test]
    #[ignore="requires private Weston; run on both NVIDIA and Mesa"]
    fn direct_window_has_no_retained_copy_and_fault_check_is_independent(){
        assert_eq!(std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(),Ok("1"));
        let connection=Connection::connect_to_env().unwrap();
        let (globals,mut queue)=registry_queue_init::<State>(&connection).unwrap();
        let compositor:WlCompositor=globals.bind(&queue.handle(),1..=4,()).unwrap();
        let surface=compositor.create_surface(&queue.handle(),());queue.roundtrip(&mut State).unwrap();
        let shared=SharedEgl::new(&connection).unwrap();
        let mut window=EglWindow::new(shared,&surface,(32,24)).unwrap();window.set_full_redraw(true);
        let mut renderer=Renderer::new();
        let scene=Scene::new(crate::scene::Node::new(Rect::new(0.,0.,32.,24.),crate::scene::Content::Rect(crate::scene::Rectangle {
            fill:crate::scene::Fill::Solid(crate::animation::Color::rgba(0.,1.,0.,1.)),..Default::default()
        })));
        std::env::set_var("HYPRV_GPU_SELF_CHECK","1");
        window.draw(&mut renderer,&scene,(32.,24.),1.,Rect::new(0.,0.,1.,1.)).unwrap();
        let target=window.retained.as_mut().unwrap();
        let backend=gpu::surfaces::get_backend_render_target(target,sk::surface::BackendHandleAccess::FlushRead).unwrap();
        assert_eq!(backend.gl_framebuffer_info().unwrap().fboid,0,"direct target must wrap the window, never an offscreen copy");
        std::env::set_var("HYPRV_GPU_SELF_CHECK_INJECT","1");
        let error=window.draw(&mut renderer,&scene,(32.,24.),1.,Rect::new(0.,0.,1.,1.)).unwrap_err();
        std::env::remove_var("HYPRV_GPU_SELF_CHECK_INJECT");
        assert!(error.to_string().contains("GPU self-check mismatch"),"{error}");
        window.draw(&mut renderer,&scene,(32.,24.),1.,Rect::new(0.,0.,1.,1.)).unwrap();
        window.draw_regions_no_swap(&mut renderer,&scene,(32.,24.),1.,vec![Rect::new(0.,0.,1.,1.)]).unwrap();
        let bytes=reference(&window.shared,window.retained.as_mut().unwrap(),(32,24)).unwrap();
        assert!(bytes.chunks_exact(4).all(|p|p==[0,255,0,255]),"partial damage must still redraw the whole direct window");
        std::env::remove_var("HYPRV_GPU_SELF_CHECK");
        drop(window);surface.destroy();
    }
}
