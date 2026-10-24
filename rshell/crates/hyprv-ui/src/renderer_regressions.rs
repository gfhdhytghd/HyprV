//! UI-R2 regression probes: real surfaceless EGL, never a live display.
use crate::{animation::Color, geometry::Rect, render::Renderer, scene::*, skia as sk};
use glow::HasContext;
use std::{cell::RefCell, rc::Rc};
struct Gpu {
    native: std::mem::ManuallyDrop<Rc<RefCell<crate::native_gl::Context>>>,
    skia: sk::gpu::DirectContext,
    api: khronos_egl::DynamicInstance<khronos_egl::EGL1_5>,
    display: khronos_egl::Display,
    context: khronos_egl::Context,
    pbuffer: khronos_egl::Surface,
}
impl Gpu {
    fn new() -> Self {
        unsafe {
            use khronos_egl as e;
            let api = e::DynamicInstance::<e::EGL1_5>::load_required().unwrap();
            let display = api
                .get_platform_display(0x31dd, std::ptr::null_mut(), &[e::ATTRIB_NONE])
                .unwrap();
            api.initialize(display).unwrap();
            api.bind_api(e::OPENGL_ES_API).unwrap();
            let config = api
                .choose_first_config(
                    display,
                    &[
                        e::SURFACE_TYPE,
                        e::PBUFFER_BIT,
                        e::RENDERABLE_TYPE,
                        e::OPENGL_ES2_BIT,
                        e::RED_SIZE,
                        8,
                        e::GREEN_SIZE,
                        8,
                        e::BLUE_SIZE,
                        8,
                        e::ALPHA_SIZE,
                        8,
                        e::NONE,
                    ],
                )
                .unwrap()
                .unwrap();
            let context = api
                .create_context(
                    display,
                    config,
                    None,
                    &[e::CONTEXT_CLIENT_VERSION, 3, e::NONE],
                )
                .unwrap();
            let pbuffer = api
                .create_pbuffer_surface(display, config, &[e::WIDTH, 1, e::HEIGHT, 1, e::NONE])
                .unwrap();
            api.make_current(display, Some(pbuffer), Some(pbuffer), Some(context))
                .unwrap();
            let load = |s: &str| {
                api.get_proc_address(s)
                    .map_or(std::ptr::null(), |p| p as *const std::ffi::c_void)
            };
            let gl = glow::Context::from_loader_function(load);
            eprintln!("GL {}", gl.get_parameter_string(glow::RENDERER));
            let skia = sk::gpu::direct_contexts::make_gl(
                sk::gpu::gl::Interface::new_load_with(load).unwrap(),
                None,
            )
            .unwrap();
            let native = Rc::new(RefCell::new(crate::native_gl::Context::new(gl).unwrap()));
            Self {
                native: std::mem::ManuallyDrop::new(native),
                skia,
                api,
                display,
                context,
                pbuffer,
            }
        }
    }
    fn surface(&mut self, w: i32, h: i32) -> sk::Surface {
        sk::gpu::surfaces::render_target(
            &mut self.skia,
            sk::gpu::Budgeted::Yes,
            &sk::ImageInfo::new_n32_premul((w, h), None),
            None,
            sk::gpu::SurfaceOrigin::BottomLeft,
            None,
            false,
            None,
        )
        .unwrap()
    }
    fn paint(
        &mut self,
        r: &mut Renderer,
        s: &Scene,
        w: i32,
        h: i32,
    ) -> Result<Vec<u8>, crate::render::RenderError> {
        self.paint_scaled(r, s, w, h, 1.)
    }
    fn paint_scaled(
        &mut self,
        r: &mut Renderer,
        s: &Scene,
        w: i32,
        h: i32,
        scale: f64,
    ) -> Result<Vec<u8>, crate::render::RenderError> {
        let mut surface = self.surface(w, h);
        let backend = sk::gpu::surfaces::get_backend_render_target(
            &mut surface,
            sk::surface::BackendHandleAccess::FlushWrite,
        )
        .unwrap();
        let guard = crate::native_gl::target(
            Rc::clone(&self.native),
            surface.canvas(),
            backend.gl_framebuffer_info().unwrap().fboid,
            (w, h),
        );
        r.paint(
            surface.canvas(),
            s,
            scale,
            Rect::new(0., 0., w as f32 / scale as f32, h as f32 / scale as f32),
        )?;
        drop(guard);
        let mut data = vec![0; w as usize * h as usize * 4];
        assert!(surface.read_pixels(
            &sk::ImageInfo::new((w, h), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None),
            &mut data,
            w as usize * 4,
            (0, 0)
        ));
        Ok(data)
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.native);
            self.skia.release_resources_and_abandon();
            self.api
                .make_current(self.display, None, None, None)
                .unwrap();
            self.api
                .destroy_surface(self.display, self.pbuffer)
                .unwrap();
            self.api
                .destroy_context(self.display, self.context)
                .unwrap();
            self.api.terminate(self.display).unwrap();
        }
    }
}
fn material_scene(mut root: Node) -> Scene {
    // Explicit opt-in: the production material path is independent of text.
    root.native_materials = Some(true);
    Scene::new(root)
}
fn rect(x: f32, y: f32, w: f32, h: f32, c: Color) -> Node {
    Node::new(
        Rect::new(x, y, w, h),
        Content::Rect(Rectangle {
            fill: Fill::Solid(c),
            ..Default::default()
        }),
    )
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_gpu_image_acceptance() {
    let mut g = Gpu::new();
    let mut source = g.surface(16, 16);
    source.canvas().clear(sk::Color::TRANSPARENT);
    let mut paint = sk::Paint::default();
    paint.set_color4f(sk::Color4f::new(1.,0.,0.,0.7),None);
    source.canvas().draw_rect(sk::Rect::from_xywh(0.,0.,16.,8.),&paint);
    paint.set_color4f(sk::Color4f::new(0.,0.,1.,0.4),None);
    source.canvas().draw_rect(sk::Rect::from_xywh(0.,8.,16.,8.),&paint);
    // Asymmetric, translucent rows catch origin flips and premultiplication loss.
    let image = source.image_snapshot();
    assert!(image.is_texture_backed());
    let mut root = Node::new(Rect::new(0., 0., 32., 32.), Content::Group);
    root.add(Node::new(
        Rect::new(0., 0., 16., 16.),
        Content::Image(image),
    ));
    let mut marker = Node::new(
        Rect::new(20., 0., 12., 20.),
        Content::Text(Text {
            text: "A".into(),
            families: vec!["JetBrainsMono Nerd Font".into()],
            render_type: RenderType::NativeRendering,
            ..Default::default()
        }),
    );
    marker.native_text_mode = NativeTextMode::QtGl;
    root.add(marker);
    let s = material_scene(root);
    assert!(
        Renderer::new().native_gl_required(&s),
        "production EglWindow must select this path"
    );
    std::env::set_var("HYPRV_IMAGE_GL_DISABLE", "1");
    let expected = g
        .paint(&mut Renderer::new(), &s, 32, 32)
        .expect("Skia control");
    std::env::remove_var("HYPRV_IMAGE_GL_DISABLE");
    let result = g.paint(&mut Renderer::new(), &s, 32, 32);
    let actual = result.expect("valid GPU-backed Content::Image must render");
    assert_eq!(actual, expected, "CPU copy must preserve pixels");
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_shadow_orientation() {
    let mut g = Gpu::new();
    let mut n = Node::new(Rect::new(40., 40., 32., 32.), Content::Group);
    n.add(rect(0., 0., 32., 8., Color::rgba(1., 0., 0., 1.)));
    n.effects.multi_effect = Some(crate::effects::MultiEffect {
        shadow_enabled: true,
        shadow_offset: (0., 7.),
        ..Default::default()
    });
    let mut s = material_scene(n);
    let mut r = Renderer::new();
    let a = g.paint(&mut r, &s, 128, 128).unwrap();
    let id = s.root().id();
    s.edit(id, |n| {
        n.effects.multi_effect.as_mut().unwrap().blur_enabled = true
    });
    let b = g.paint(&mut r, &s, 128, 128).unwrap();
    let diff = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
        .count();
    eprintln!("specialized vs generic differing pixels (>2): {diff}");
    assert_eq!(diff, 0, "blur=0 must not flip unblurred asymmetric source");
}
fn raster_image(w: i32, h: i32, c: sk::Color) -> sk::Image {
    let mut s = sk::surfaces::raster_n32_premul((w, h)).unwrap();
    s.canvas().clear(c);
    s.image_snapshot()
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_mip_budget() {
    let mut g = Gpu::new();
    let mut root = Node::new(Rect::new(0., 0., 64., 32.), Content::Group);
    for i in 0..2 {
        let mut n = Node::new(
            Rect::new(i as f32 * 32., 0., 32., 32.),
            Content::Image(raster_image(1024, 1024, sk::Color::RED)),
        );
        n.image_sampling = crate::image::ImageSampling::Mipmap;
        root.add(n);
    }
    g.paint(&mut Renderer::new(), &material_scene(root), 64, 32)
        .unwrap();
    let stats = g.native.borrow().review_stats();
    assert!(stats.0 > 0, "probe must upload mipmapped textures");
    assert_eq!(stats.0, stats.1, "charge must include all mip levels");
    eprintln!("charged={}, complete mip storage={}", stats.0, stats.1);
    assert!(
        stats.1 <= 8 * 1024 * 1024,
        "8 MiB GPU texture cache over budget: {stats:?}"
    );
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_clear_gpu_retention() {
    let mut g = Gpu::new();
    let mut r = Renderer::new();
    let mut root = Node::new(Rect::new(0., 0., 128., 64.), Content::Group);
    root.add(Node::new(
        Rect::new(0., 0., 16., 16.),
        Content::Image(raster_image(64, 64, sk::Color::RED)),
    ));
    let mut n = rect(10., 20., 80., 32., Color::rgba(1., 0., 0., 1.));
    if let Content::Rect(ref mut q) = n.content {
        q.radii = [16.; 4];
    }
    let id = root.add(n);
    let mut s = material_scene(root);
    for i in 0..300 {
        s.edit(id, |n| n.frame.width = 80. + i as f32 / 100.);
        g.paint(&mut r, &s, 128, 64).unwrap();
    }
    let before = g.native.borrow().review_stats();
    assert!(
        before.0 > 0 && before.2 == 300 && before.3 > 0,
        "probe must exercise live native caches: {before:?}"
    );
    r.clear_caches();
    // Invalidation works even when the next scene no longer needs native GL.
    let empty = Scene::new(Node::new(Rect::new(0., 0., 128., 64.), Content::Group));
    assert!(!r.native_gl_required(&empty));
    g.paint(&mut r, &empty, 128, 64).unwrap();
    let after = g.native.borrow().review_stats();
    eprintln!("GPU caches before clear={before:?}; after clear={after:?}");
    assert_eq!(
        after,
        (0, 0, 0, 0),
        "clear_caches must release cached GPU resources"
    );
}
fn cpu_pixels(r: &mut Renderer, s: &Scene) -> Vec<u8> {
    let mut p = r.raster(s, 32., 32., 1.).unwrap();
    let pix = p.peek_pixels().unwrap();
    pix.bytes().unwrap().to_vec()
}
#[test]
fn review_r2_blending_mode_invalidation() {
    let mut root = Node::new(Rect::new(0., 0., 32., 32.), Content::Group);
    root.effects.multi_effect = Some(Default::default());
    for i in 0..8 {
        let mut n = rect(
            2.,
            2.,
            28.,
            28.,
            Color::rgba(
                0.1234 + i as f32 * 0.091,
                0.823,
                0.316,
                0.143 + i as f32 * 0.037,
            ),
        );
        n.rect_mode = Some(crate::qt_rect::RectMode::Skia);
        root.add(n);
    }
    let s = material_scene(root);
    let mut cached = Renderer::new();
    let a = cpu_pixels(&mut cached, &s);
    cached.set_gpu_exact_blending(false);
    let b = cpu_pixels(&mut cached, &s);
    let mut fresh = Renderer::new();
    fresh.set_gpu_exact_blending(false);
    let expected = cpu_pixels(&mut fresh, &s);
    let diff = a.iter().zip(&expected).filter(|(a, b)| a != b).count();
    eprintln!("blending-mode fresh changed bytes={diff}");
    assert!(diff > 0, "control must distinguish requested modes");
    assert!(
        b == expected,
        "switching blending mode must invalidate captured effects; stale bytes={}",
        b.iter().zip(&expected).filter(|(a, b)| a != b).count()
    );
    cached.set_gpu_exact_blending(true);
    assert_eq!(cpu_pixels(&mut cached,&s),a,"reverse mode transition must invalidate too");
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_scale_rotation_theme_fresh_control() {
    let mut g = Gpu::new();
    let mut root = Node::new(Rect::new(0., 0., 64., 64.), Content::Group);
    let mut effect = rect(8., 8., 40., 24., Color::rgba(1., 0., 0., 0.7));
    effect.effects.multi_effect = Some(Default::default());
    let leaf = effect.id();
    root.add(effect);
    root.add(Node::new(
        Rect::new(8., 32., 40., 20.),
        Content::Text(Text {
            text: "CACHE".into(),
            families: vec!["JetBrainsMono Nerd Font".into()],
            ..Default::default()
        }),
    ));
    let id = root.id();
    let mut scene = material_scene(root);
    let mut cached = Renderer::new();
    for step in 0..8 {
        let scale = [1., 1.25, 2., 1.][step % 4];
        if step == 4 {
            scene.edit(id, |n| {
                n.transform = sk::Matrix::rotate_deg_pivot(90., (32., 32.))
            });
        }
        if step == 6 {
            scene.edit(leaf, |n| {
                if let Content::Rect(ref mut r) = n.content {
                    r.fill = Fill::Solid(Color::rgba(0., 1., 0., 0.4));
                }
            });
        }
        let w = (64. * scale) as i32;
        let a = g.paint_scaled(&mut cached, &scene, w, w, scale).unwrap();
        let b = g
            .paint_scaled(&mut Renderer::new(), &scene, w, w, scale)
            .unwrap();
        assert!(
            a == b,
            "warm/fresh mismatch at transition {step}, scale={scale}"
        );
    }
    eprintln!("8 scale/rotation/theme transitions match fresh renderer exactly");
}
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn review_r2_mixed_gl_skia_state_control() {
    let mut g = Gpu::new();
    let mut root = Node::new(Rect::new(0., 0., 128., 96.), Content::Group);
    for i in 0..6 {
        let y = i as f32 * 14.;
        let mut n = rect(2., y, 50., 12., Color::rgba(0.3, 0.5, 0.7, 0.8));
        if i % 2 == 1 {
            n.rect_mode = Some(crate::qt_rect::RectMode::Skia);
        }
        root.add(n);
        let mut t = Node::new(
            Rect::new(58., y, 60., 14.),
            Content::Text(Text {
                text: "AB".into(),
                size: 10.,
                families: vec!["JetBrainsMono Nerd Font".into()],
                render_type: RenderType::NativeRendering,
                ..Default::default()
            }),
        );
        if i % 2 == 0 {
            t.native_text_mode = NativeTextMode::QtGl;
        }
        root.add(t);
    }
    let s = material_scene(root);
    let a = g.paint(&mut Renderer::new(), &s, 128, 96).unwrap();
    let block = crate::native_gl::block();
    let b = g.paint(&mut Renderer::new(), &s, 128, 96).unwrap();
    drop(block);
    let max = a
        .iter()
        .zip(b.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    eprintln!("alternating direct GL/Skia vs Skia-only max channel delta={max}");
    assert!(max <= 2, "interleaved draw state contamination");
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn upload_failure_copies_pixels_and_next_frame_recovers() {
    let mut g = Gpu::new();
    let scene = material_scene(Node::new(
        Rect::new(2., 2., 16., 16.),
        Content::Image(raster_image(16, 16, sk::Color::RED)),
    ));
    let mut renderer = Renderer::new();
    g.native.borrow_mut().fail_upload_after(0);
    let fallback = g.paint(&mut renderer, &scene, 24, 24).unwrap();
    assert_eq!(
        g.native.borrow().review_stats().0,
        0,
        "failed upload must not enter cache"
    );
    let recovered = g.paint(&mut renderer, &scene, 24, 24).unwrap();
    assert!(
        g.native.borrow().review_stats().0 > 0,
        "next frame must upload normally"
    );
    assert_eq!(fallback, recovered);
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn mipmap_upgrade_evicts_other_textures_and_keeps_its_charge() {
    let mut g = Gpu::new();
    let mut root = Node::new(Rect::new(0., 0., 64., 32.), Content::Group);
    for i in 0..3 {
        root.add(Node::new(
            Rect::new(i as f32 * 8., 0., 8., 8.),
            Content::Image(raster_image(512, 512, sk::Color::RED)),
        ));
    }
    let id = root.add(Node::new(
        Rect::new(24., 0., 16., 16.),
        Content::Image(raster_image(1024, 1024, sk::Color::BLUE)),
    ));
    let mut scene = material_scene(root);
    let mut renderer = Renderer::new();
    g.paint(&mut renderer, &scene, 64, 32).unwrap();
    assert_eq!(g.native.borrow().review_stats().0, 7 * 1024 * 1024);
    scene.edit(id, |n| {
        n.image_sampling = crate::image::ImageSampling::Mipmap
    });
    g.paint(&mut renderer, &scene, 64, 32).unwrap();
    let upgraded = g.native.borrow().review_stats();
    assert!(upgraded.0 <= 8 * 1024 * 1024);
    assert_eq!(upgraded.0, upgraded.1);
    assert_eq!(upgraded.0, 2 * 1024 * 1024 + 5_592_404);
    // Revisit only this image: Linear filtering does not free its existing mips.
    scene.edit(scene.root().id(), |n| {
        n.children.retain(|child| child.id() == id)
    });
    scene.edit(id, |n| {
        n.image_sampling = crate::image::ImageSampling::Linear
    });
    g.paint(&mut renderer, &scene, 64, 32).unwrap();
    assert_eq!(g.native.borrow().review_stats(), upgraded);
    // Explicit owner release works immediately, with its context current.
    g.native.borrow_mut().clear_caches();
    g.skia.reset(None);
    assert_eq!(g.native.borrow().review_stats(), (0, 0, 0, 0));
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn partial_glyph_upload_failure_does_not_double_translucent_ink() {
    let mut g = Gpu::new();
    let info = sk::ImageInfo::new((4, 4), sk::ColorType::Alpha8, sk::AlphaType::Premul, None);
    let masks: Vec<_> = (0..2)
        .map(|i| crate::native_gl::Glyph {
            image: sk::images::raster_from_data(&info, sk::Data::new_copy(&[255; 16]), 4).unwrap(),
            x: i as f32 * 5.,
            y: 0.,
        })
        .collect();
    let mut surface = g.surface(12, 8);
    surface.canvas().clear(sk::Color::TRANSPARENT);
    let backend = sk::gpu::surfaces::get_backend_render_target(
        &mut surface,
        sk::surface::BackendHandleAccess::FlushWrite,
    )
    .unwrap();
    let _guard = crate::native_gl::target(
        Rc::clone(&g.native),
        surface.canvas(),
        backend.gl_framebuffer_info().unwrap().fboid,
        (12, 8),
    );
    g.native.borrow_mut().fail_upload_after(1);
    crate::native_gl::draw(surface.canvas(), &masks, [0.25, 0.1, 0.05, 0.5]).unwrap();
    let mut pixels = vec![0; 12 * 8 * 4];
    assert!(surface.read_pixels(
        &sk::ImageInfo::new(
            (12, 8),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None
        ),
        &mut pixels,
        12 * 4,
        (0, 0)
    ));
    let first = &pixels[(12 + 1) * 4..(12 + 1) * 4 + 4];
    let second = &pixels[(12 + 6) * 4..(12 + 6) * 4 + 4];
    assert!(
        first.iter().zip(second).all(|(a, b)| a.abs_diff(*b) <= 1),
        "partial fallback changed alpha: {first:?} vs {second:?}"
    );
    assert!(first[3].abs_diff(128) <= 1, "first glyph was painted twice");
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn unrelated_cpu_paint_does_not_flush_an_active_gpu_target() {
    let mut g=Gpu::new();
    let scene=material_scene(Node::new(Rect::new(0.,0.,16.,16.),Content::Image(raster_image(16,16,sk::Color::RED))));
    let mut renderer=Renderer::new();
    g.paint(&mut renderer,&scene,24,24).unwrap();
    let before=g.native.borrow().review_stats();
    assert!(before.0>0);
    let mut surface=g.surface(24,24);
    let backend=sk::gpu::surfaces::get_backend_render_target(&mut surface,sk::surface::BackendHandleAccess::FlushWrite).unwrap();
    let _guard=crate::native_gl::target(Rc::clone(&g.native),surface.canvas(),backend.gl_framebuffer_info().unwrap().fboid,(24,24));
    Renderer::new().raster(&Scene::new(Node::new(Rect::new(0.,0.,16.,16.),Content::Group)),16.,16.,1.).unwrap();
    assert_eq!(g.native.borrow().review_stats(),before,"a CPU renderer must not invalidate a different active canvas");
}

/// Frozen original-QML left edge, excluding text. Full-output projection matters:
/// moving these curves to a small target hides the float-precision regression.
#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn scaled_quick_adjust_clip_matches_qt_corner_pixels() {
    let mut gpu = Gpu::new();
    let scale = 0.999_375_f32;
    let mut transform = sk::Matrix::translate((278., 0.));
    transform
        .pre_scale((scale, scale), None)
        .pre_translate((-278., 0.));
    let parent = ((2784. + transform.translate_x()) / scale, 58. / scale);
    let dpr = 2. * scale;
    let snapped = |origin: f32, len: f32| {
        let start = (origin * dpr).round() / dpr;
        (start - origin, ((origin + len) * dpr).round() / dpr - start)
    };
    let (dx, w) = snapped(parent.0, 97.3);
    let (dy, h) = snapped(parent.1, 62.);
    for (dark, golden) in [
        (
            true,
            include_bytes!("../tests/quick-adjust/dark.png").as_slice(),
        ),
        (
            false,
            include_bytes!("../tests/quick-adjust/light.png").as_slice(),
        ),
    ] {
        let rgba = |r: u8, g: u8, b: u8, a: f32| {
            Color::rgba(r as f32 / 255., g as f32 / 255., b as f32 / 255., a)
        };
        let rounded = |fill: Color, border: Option<Border>| {
            Node::new(
                Rect::new(0., 0., 278., 62.),
                Content::Rect(Rectangle {
                    fill: Fill::Solid(fill),
                    radii: [19.; 4],
                    border,
                }),
            )
        };
        let mut card = rounded(rgba(255, 255, 255, if dark { 0.07 } else { 0.22 }), None);
        card.frame.x = 2784.;
        card.frame.y = 58.;
        card.transform = transform;
        let mut clip = Node::new(Rect::new(dx, dy, w, h), Content::Group);
        clip.clip = Some([0.; 4]);
        let mut fill = rounded(
            if dark {
                rgba(137, 180, 250, 0.28)
            } else {
                rgba(64, 124, 221, 0.22)
            },
            None,
        );
        fill.frame.x = -dx;
        fill.frame.y = -dy;
        clip.add(fill);
        card.add(clip);
        card.add(rounded(
            Color::default(),
            Some(Border {
                width: 1.,
                color: if dark {
                    rgba(205, 214, 244, 0.12)
                } else {
                    rgba(43, 43, 44, 0.08)
                },
            }),
        ));
        let actual = gpu
            .paint_scaled(&mut Renderer::new(), &material_scene(card), 6144, 3456, 2.)
            .unwrap();
        let reference =
            sk::images::deferred_from_encoded_data(sk::Data::new_copy(golden), None).unwrap();
        let mut expected = vec![0; 32 * 130 * 4];
        assert!(reference.read_pixels(
            &sk::ImageInfo::new(
                (32, 130),
                sk::ColorType::RGBA8888,
                sk::AlphaType::Premul,
                None
            ),
            &mut expected,
            32 * 4,
            (0, 0),
            sk::image::CachingHint::Disallow
        ));
        for y in 0..130 {
            for x in 0..32 {
                let a = ((100 + y) * 6144 + 5560 + x) * 4;
                let b = (y * 32 + x) * 4;
                assert_eq!(
                    &actual[a..a + 4],
                    &expected[b..b + 4],
                    "dark={dark} corner ({x},{y})"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn numeric_text_changes_reuse_warmed_glyph_textures() {
    let mut gpu=Gpu::new();
    let mut text=Text{text:"0123456789".repeat(8),size:16.,weight:700,elide:false,render_type:RenderType::NativeRendering,..Default::default()};
    let node=|text:Text|{let mut n=Node::new(Rect::new(0.,0.,850.,32.),Content::Text(text));n.native_text_mode=NativeTextMode::QtGl;n};
    let mut renderer=Renderer::new();
    gpu.paint_scaled(&mut renderer,&material_scene(node(text.clone())),1800,80,2.).unwrap();
    let before=gpu.native.borrow().review_stats();
    gpu.native.borrow_mut().fail_upload_after(0);
    text.text="81".into();
    gpu.paint_scaled(&mut renderer,&material_scene(node(text)),1800,80,2.).expect("changing a number must not re-upload existing glyphs");
    assert!(gpu.native.borrow().review_upload_budget_untouched(),"a CPU fallback must not hide an attempted glyph re-upload");
    assert_eq!(gpu.native.borrow().review_stats(),before,"all required glyph textures must remain resident");
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn disjoint_retained_damage_matches_a_complete_gpu_frame() {
    let mut gpu=Gpu::new();let mut renderer=Renderer::new();
    let mut root=Node::new(Rect::new(0.,0.,320.,48.),Content::Group);
    root.add(rect(0.,0.,320.,48.,Color::rgba(0.2,0.3,0.4,0.6)));
    let mut a=rect(10.,5.,30.,32.,Color::rgba(1.,0.,0.,0.7));if let Content::Rect(r)=&mut a.content{r.radii=[9.;4];}
    let mut b=rect(270.,5.,30.,32.,Color::rgba(1.,0.,0.,0.7));if let Content::Rect(r)=&mut b.content{r.radii=[9.;4];}
    let aid=a.id();let bid=b.id();
    root.add(a);root.add(b);let mut scene=material_scene(root);let extent=Rect::new(0.,0.,320.,48.);
    let mut target=gpu.surface(640,96);
    let native=Rc::clone(&gpu.native);
    let draw=|renderer:&mut Renderer,scene:&Scene,regions:Vec<Rect>,target:&mut sk::Surface|{
        let backend=sk::gpu::surfaces::get_backend_render_target(target,sk::surface::BackendHandleAccess::FlushWrite).unwrap();
        let _guard=crate::native_gl::target(Rc::clone(&native),target.canvas(),backend.gl_framebuffer_info().unwrap().fboid,(640,96));
        for region in regions{renderer.paint(target.canvas(),scene,2.,region).unwrap();}
    };
    let initial=scene.take_damage_regions(extent);
    draw(&mut renderer,&scene,initial,&mut target);
    for step in 0..4 {
        for id in [aid,bid] {scene.edit(id,|n|if let Content::Rect(r)=&mut n.content{r.fill=Fill::Solid(Color::rgba(0.1,0.3+step as f32*0.1,0.8,0.4));});}
        let regions=scene.take_damage_regions(extent);assert_eq!(regions.len(),2);
        draw(&mut renderer,&scene,regions,&mut target);
        let mut actual=vec![0;640*96*4];assert!(target.read_pixels(&sk::ImageInfo::new((640,96),sk::ColorType::RGBA8888,sk::AlphaType::Premul,None),&mut actual,640*4,(0,0)));
        let expected=gpu.paint_scaled(&mut Renderer::new(),&scene,640,96,2.).unwrap();
        assert_eq!(actual,expected,"partial alpha composition changed pixels at step {step}");
    }
}

#[test]
#[ignore="requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn batched_glyph_atlas_matches_original_mask_shader_exactly(){
    let mut gpu=Gpu::new();let mut renderer=Renderer::new();
    for scale in [1.,1.25,1.5,2.] {for weight in [400,500,700]{
        let mut root=rect(0.,0.,360.,50.,Color::rgba(0.19,0.25,0.31,0.66));
        let mut text=Node::new(Rect::new(8.,5.,340.,40.),Content::Text(Text{text:"ffi 0123456789% \u{f2db} — 字".into(),size:21.,weight,elide:false,color:Color::rgba(0.3,0.6,0.9,0.71),..Default::default()}));
        text.native_text_mode=NativeTextMode::QtGl;root.add(text);let scene=material_scene(root);
        std::env::remove_var("HYPRV_GLYPH_ATLAS_DISABLE");
        gpu.native.borrow_mut().review_force_atlas_wrap();
        let a=gpu.paint_scaled(&mut renderer,&scene,(360.*scale)as i32,(50.*scale)as i32,scale).unwrap();
        std::env::set_var("HYPRV_GLYPH_ATLAS_DISABLE","1");
        let b=gpu.paint_scaled(&mut renderer,&scene,(360.*scale)as i32,(50.*scale)as i32,scale).unwrap();
        std::env::remove_var("HYPRV_GLYPH_ATLAS_DISABLE");
        assert_eq!(a,b,"atlas changed native pixels atDPR{scale},weight{weight}");
    }}
}

/// Actual original-QML pre-swap pixels, before any compositor sampling. A
/// standalone texture is not equivalent to QSG's screen-sized Canvas atlas.
#[test]
#[ignore="requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn canvas_r8_atlas_matches_original_window_alpha_exactly(){
    let mut gpu=Gpu::new();
    let get=unsafe{std::mem::transmute::<extern "system" fn(),unsafe extern "system" fn(u32)->*const u8>(gpu.api.get_proc_address("glGetString").unwrap())};
    let driver=unsafe{std::ffi::CStr::from_ptr(get(glow::RENDERER).cast())}.to_string_lossy();
    if !driver.contains("NVIDIA") {eprintln!("UNAVAILABLE frozen NVIDIA Qt-window oracle on {driver}");return;}
    let info=sk::ImageInfo::new((54,216),sk::ColorType::Alpha8,sk::AlphaType::Premul,None);
    let image=sk::images::raster_from_data(&info,sk::Data::new_copy(include_bytes!("../tests/qt-canvas/corners-a8.bin")),54).unwrap();
    let mut root=Node::new(Rect::new(0.,0.,3072.,1728.),Content::Group);root.native_materials=Some(true);
    for (corner,(x,y)) in [(0.,0.),(3045.,0.),(0.,1701.),(3045.,1701.)].into_iter().enumerate(){
        let mut node=Node::new(Rect::new(x,y,27.,27.),Content::Image(image.clone()));
        node.image_sampling=crate::image::ImageSampling::QtCanvasAtlas{size:(8192,4096),corner:corner as u8};root.add(node);
    }
    let pixels=gpu.paint_scaled(&mut Renderer::new(),&Scene::new(root),6144,3456,2.).unwrap();
    let golden=sk::Image::from_encoded(sk::Data::new_copy(include_bytes!("../tests/qt-canvas/bottom-left-window@2.png"))).unwrap();
    let mut expected=vec![0;54*54*4];
    assert!(golden.read_pixels(&sk::ImageInfo::new((54,54),sk::ColorType::RGBA8888,sk::AlphaType::Premul,None),&mut expected,54*4,(0,0),sk::image::CachingHint::Allow));
    for row in 0..54 {assert_eq!(&pixels[((3402+row)*6144)*4..][..54*4],&expected[row*54*4..][..54*4],"Qt window row {row}");}
    assert_eq!(gpu.native.borrow().review_stats().0,8192*4096,"one shared R8 atlas, not four RGBA textures");
}

#[test]
#[ignore="requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn promoted_batch_root_matches_original_bar_window_aa() {
    let mut gpu=Gpu::new();
    let get=unsafe{std::mem::transmute::<extern "system" fn(),unsafe extern "system" fn(u32)->*const u8>(gpu.api.get_proc_address("glGetString").unwrap())};
    let driver=unsafe{std::ffi::CStr::from_ptr(get(glow::RENDERER).cast())}.to_string_lossy();
    if !driver.contains("NVIDIA") {eprintln!("UNAVAILABLE frozen NVIDIA Qt-window oracle on {driver}");return;}
    let draw=|promote:bool| {
        let mut root=Node::new(Rect::new(0.,0.,3072.,58.),Content::Group);root.native_materials=Some(true);
        let mut row=Node::new(Rect::new(1603.65625_f64 as f32,10.,1458.84375_f64 as f32,38.),Content::Group);
        row.native_batch_root=promote;
        let mut pill=rect(244.3125,0.,1214.53125_f64 as f32,38.,Color::rgba(30./255.,30./255.,46./255.,0.8));
        if let Content::Rect(r)=&mut pill.content {r.radii=[19.;4];}
        row.add(pill);root.add(row);Scene::new(root)
    };
    let expected=include_bytes!("../tests/qt-canvas/right-row-window@2.rgba");
    let crop=|pixels:Vec<u8>| (0..84).flat_map(|y|pixels[(y*6144+6116)*4..][..5*4].to_vec()).collect::<Vec<_>>();
    let ordinary=crop(gpu.paint_scaled(&mut Renderer::new(),&draw(false),6144,116,2.).unwrap());
    assert_ne!(&ordinary[..],expected,"negative control must expose the original coordinate-basis difference");
    let promoted=crop(gpu.paint_scaled(&mut Renderer::new(),&draw(true),6144,116,2.).unwrap());
    assert_eq!(&promoted[..],expected,"promoted Row must match the original window, no mask or layer");
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn retired_window_releases_only_its_native_images_and_canvas_variants() {
    let mut gpu = Gpu::new();
    let mut renderer = Renderer::new();
    let info = sk::ImageInfo::new((54,216),sk::ColorType::Alpha8,sk::AlphaType::Premul,None);
    let image = sk::images::raster_from_data(&info,
        sk::Data::new_copy(include_bytes!("../tests/qt-canvas/corners-a8.bin")),54).unwrap();
    let scene = |size| {
        let mut node = Node::new(Rect::new(0.,0.,27.,27.),Content::Image(image.clone()));
        node.native_materials = Some(true);
        node.image_sampling = crate::image::ImageSampling::QtCanvasAtlas {size,corner:0};
        Scene::new(node)
    };
    let live = scene((1024,1024));
    for owner in [1,2] {
        gpu.native.borrow_mut().set_resource_owner(owner);
        gpu.paint(&mut renderer,&live,32,32).unwrap();
    }
    let expected = gpu.paint(&mut renderer,&live,32,32).unwrap();
    assert_eq!(gpu.native.borrow().review_stats().0,1024*1024);
    gpu.native.borrow_mut().set_resource_owner(3);
    gpu.paint(&mut renderer,&scene((2048,1024)),32,32).unwrap();
    assert_eq!(gpu.native.borrow().review_stats().0,3*1024*1024);
    gpu.native.borrow_mut().release_owner(3);
    gpu.native.borrow_mut().release_owner(1);
    assert_eq!(gpu.native.borrow().review_stats().0,1024*1024);
    gpu.native.borrow_mut().set_resource_owner(2);
    gpu.native.borrow_mut().fail_upload_after(0);
    assert_eq!(gpu.paint(&mut renderer,&live,32,32).unwrap(),expected);
    assert!(gpu.native.borrow().review_upload_budget_untouched());
    gpu.native.borrow_mut().release_owner(2);
    gpu.native.borrow_mut().release_owner(2);
    assert_eq!(gpu.native.borrow().review_stats().0,0);
}

#[test]
#[ignore = "requires surfaceless EGL; run tests/run-renderer-regressions.sh"]
fn retired_window_keeps_shared_glyphs_without_reupload() {
    let mut gpu = Gpu::new();
    let mut renderer = Renderer::new();
    let mut node = Node::new(Rect::new(0.,0.,320.,48.),Content::Text(Text {
        text:"CPU 12% Memory 34%".into(),size:16.,render_type:RenderType::NativeRendering,..Default::default()
    }));
    node.native_text_mode = NativeTextMode::QtGl;
    let scene = Scene::new(node);
    gpu.native.borrow_mut().set_resource_owner(1);
    let expected = gpu.paint(&mut renderer,&scene,320,48).unwrap();
    gpu.native.borrow_mut().set_resource_owner(2);
    assert_eq!(gpu.paint(&mut renderer,&scene,320,48).unwrap(),expected);
    let bytes = gpu.native.borrow().review_stats().0;
    assert_eq!(bytes,512*512,"one R8 page, not a four-channel mask copy");
    gpu.native.borrow_mut().release_owner(1);
    assert_eq!(gpu.native.borrow().review_stats().0,bytes);
    gpu.native.borrow_mut().fail_upload_after(0);
    assert_eq!(gpu.paint(&mut renderer,&scene,320,48).unwrap(),expected);
    assert!(gpu.native.borrow().review_upload_budget_untouched());
    gpu.native.borrow_mut().release_owner(2);
    assert_eq!(gpu.native.borrow().review_stats().0,0);
}
