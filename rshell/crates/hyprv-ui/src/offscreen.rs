//! Offscreen rendering through the production Wayland EGL/RGBA8 path.
//! A role-less surface is never mapped. Callers supply a display; tests use a
//! private headless compositor. No helper process is started by the engine.

/// Offscreen transport, independent of per-node text/material rendering modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    #[default]
    Cpu,
    Gl,
}
impl Backend {
    pub fn from_env() -> Result<Self, crate::render::RenderError> {
        match std::env::var(ENV).as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("") | Ok("cpu") => Ok(Self::Cpu),
            Ok("gl") => Ok(Self::Gl),
            _ => Err(crate::render::RenderError::Offscreen(
                "HYPRV_PARITY_BACKEND must be cpu or gl".into(),
            )),
        }
    }
}
use crate::{
    egl::{EglWindow, SharedEgl},
    geometry::Rect,
    render::{physical_size, Renderer},
    scene::Scene,
};
use std::{cell::RefCell, rc::Rc};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_compositor::WlCompositor, wl_registry::WlRegistry, wl_surface::WlSurface},
    Connection, Dispatch, QueueHandle,
};

pub const ENV: &str = "HYPRV_PARITY_BACKEND";

#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
fn err(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}

struct State;
impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: <WlCompositor as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: <WlSurface as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// One production-path offscreen target (reused across renders; resized as needed).
pub struct GpuTarget {
    _connection: Connection,
    shared: Rc<SharedEgl>,
    surface: WlSurface,
    window: Option<(EglWindow, (i32, i32))>,
    id: u64,
}

impl GpuTarget {
    pub fn new() -> Result<Self, Error> {
        let connection = Connection::connect_to_env().map_err(err)?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection).map_err(err)?;
        let handle = queue.handle();
        let compositor: WlCompositor = globals.bind(&handle, 1..=4, ()).map_err(err)?;
        let surface = compositor.create_surface(&handle, ());
        queue.roundtrip(&mut State).map_err(err)?;
        let shared = SharedEgl::new(&connection).map_err(err)?;
        Ok(Self {
            _connection: connection,
            shared,
            surface,
            window: None,
            id: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
        })
    }

    /// Render `scene` (logical `w`×`h` at `scale`) exactly as `EglWindow::draw` does for a
    /// live surface and return the PNG of the retained GPU target (straight alpha RGBA8).
    pub fn render_png(
        &mut self,
        renderer: &mut Renderer,
        scene: &Scene,
        w: f32,
        h: f32,
        scale: f64,
    ) -> Result<Vec<u8>, Error> {
        self.draw(renderer, scene, w, h, scale)?
            .png_bytes()
            .map_err(err)
    }
    fn draw(
        &mut self,
        renderer: &mut Renderer,
        scene: &Scene,
        w: f32,
        h: f32,
        scale: f64,
    ) -> Result<&mut EglWindow, Error> {
        let size = physical_size(w, h, scale).map_err(err)?;
        renderer.sync_offscreen_target(self.id);
        if self.window.as_ref().map(|(_, s)| *s) != Some(size) {
            self.window = None;
            let window = EglWindow::new(self.shared.clone(), &self.surface, size).map_err(err)?;
            self.window = Some((window, size));
        }
        let window = &mut self.window.as_mut().unwrap().0;
        window
            .draw(renderer, scene, (w, h), scale, Rect::new(0., 0., w, h))
            .map_err(err)?;
        Ok(window)
    }
}

type Endpoint = (
    Option<std::ffi::OsString>,
    Option<std::ffi::OsString>,
    Option<std::ffi::OsString>,
);
fn endpoint() -> Endpoint {
    (
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("WAYLAND_DISPLAY"),
        std::env::var_os("WAYLAND_SOCKET"),
    )
}
thread_local! {
    static TARGET: RefCell<Option<(Endpoint, GpuTarget)>> = const { RefCell::new(None) };
}
fn with_target<T>(f: impl FnOnce(&mut GpuTarget) -> Result<T, Error>) -> Result<T, Error> {
    TARGET.with(|cell| {
        let mut slot = cell.borrow_mut();
        let endpoint = endpoint();
        if slot.as_ref().is_none_or(|(old, _)| *old != endpoint) {
            *slot = Some((endpoint, GpuTarget::new()?));
        }
        f(&mut slot.as_mut().unwrap().1)
    })
}
/// Release this thread's offscreen Wayland/EGL connection and its caches.
pub fn clear_target() {
    TARGET.with(|slot| *slot.borrow_mut() = None);
}

/// `true` when `HYPRV_PARITY_BACKEND=gl`.
pub fn enabled() -> bool {
    std::env::var(ENV).as_deref() == Ok("gl")
}

/// The harness hook: `Some(png)` rendered on the production GPU path when enabled, else `None`.
pub fn render_png_if_enabled(
    renderer: &mut Renderer,
    scene: &Scene,
    w: f32,
    h: f32,
    scale: f64,
) -> Result<Option<Vec<u8>>, Error> {
    if renderer.offscreen_backend().map_err(err)? != Backend::Gl {
        return Ok(None);
    }
    physical_size(w, h, scale).map_err(err)?;
    with_target(|target| target.render_png(renderer, scene, w, h, scale)).map(Some)
}

/// Same as [`render_png_if_enabled`] but decoded to a Skia image (premultiplied as decoded).
pub fn render_image_if_enabled(
    renderer: &mut Renderer,
    scene: &Scene,
    w: f32,
    h: f32,
    scale: f64,
) -> Result<Option<crate::skia::Image>, Error> {
    render_png_if_enabled(renderer, scene, w, h, scale)?
        .map(|png| {
            crate::skia::Image::from_encoded(crate::skia::Data::new_copy(&png))
                .ok_or_else(|| err("GPU PNG decode"))
        })
        .transpose()
}

/// Render an RGBA8 GPU scene without a PNG encode/decode round trip.
pub(crate) fn raster(
    renderer: &mut Renderer,
    scene: &Scene,
    w: f32,
    h: f32,
    scale: f64,
) -> Result<crate::skia::Surface, crate::render::RenderError> {
    let size = physical_size(w, h, scale)?;
    with_target(|target| {
        let image = target
            .draw(renderer, scene, w, h, scale)?
            .snapshot()
            .map_err(err)?;
        let info = crate::skia::ImageInfo::new_n32_premul(size, None);
        let mut pixels = vec![0u8; size.0 as usize * size.1 as usize * 4];
        if !image.read_pixels(
            &info,
            &mut pixels,
            size.0 as usize * 4,
            (0, 0),
            crate::skia::image::CachingHint::Disallow,
        ) {
            return Err(err("GPU readback"));
        }
        let mut surface = crate::skia::surfaces::raster_n32_premul(size)
            .ok_or_else(|| err("raster allocation"))?;
        if !surface
            .canvas()
            .write_pixels(&info, &pixels, size.0 as usize * 4, (0, 0))
        {
            return Err(err("raster copy"));
        }
        Ok(surface)
    })
    .map_err(|error| crate::render::RenderError::Offscreen(error.to_string()))
}

impl Drop for GpuTarget {
    fn drop(&mut self) {
        self.window = None;
        self.surface.destroy();
    }
}

#[cfg(test)]
mod partial_tests {
    use super::*;
    use crate::{animation::Color,scene::{Content,Fill,Node,Rectangle}};
    #[test]
    #[ignore="requires private Weston; run tests/run-renderer-regressions.sh"]
    fn partial_egl_frames_preserve_every_unchanged_pixel(){
        assert_eq!(std::env::var("HYPRV_TEST_PRIVATE_WAYLAND").as_deref(),Ok("1"));
        let mut target=GpuTarget::new().unwrap();
        let mut root=Node::new(Rect::new(0.,0.,160.,40.),Content::Rect(Rectangle{fill:Fill::Solid(Color::rgba(0.2,0.3,0.4,0.6)),..Default::default()}));
        root.native_materials=Some(true);
        let mut ids=Vec::new();
        for x in [5.,130.]{let n=Node::new(Rect::new(x,5.,20.,30.),Content::Rect(Rectangle{radii:[8.;4],fill:Fill::Solid(Color::rgba(0.8,0.2,0.4,0.5)),..Default::default()}));ids.push(n.id());root.add(n);}
        let mut scene=Scene::new(root);let extent=Rect::new(0.,0.,160.,40.);
        let mut renderer=Renderer::new();target.render_png(&mut renderer,&scene,160.,40.,2.).unwrap();scene.take_damage(extent);
        std::env::set_var("HYPRV_GPU_SELF_CHECK","1");
        for step in 0..12{
            scene.edit(ids[step%2],|n|if let Content::Rect(r)=&mut n.content{r.fill=Fill::Solid(Color::rgba(0.1,0.3+step as f32*0.02,0.9,0.5));});
            let damage=scene.take_damage_regions(extent);
            target.window.as_mut().unwrap().0.draw_regions(&mut renderer,&scene,(160.,40.),2.,damage).unwrap();
        }
        std::env::remove_var("HYPRV_GPU_SELF_CHECK");
    }

}
