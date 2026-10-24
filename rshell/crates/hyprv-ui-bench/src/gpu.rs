//! Surfaceless EGL + Skia GL, configured like `hyprv_ui::egl::SharedEgl` (GLES2 context,
//! RGBA8 + stencil config), plus GL_EXT_disjoint_timer_query and NVX memory queries.
use hyprv_ui::skia::{self as sk, gpu};
use khronos_egl as egl;
use std::ffi::c_void;

type Api = egl::DynamicInstance<egl::EGL1_5>;
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;
const TIME_ELAPSED: u32 = 0x88BF;
const QUERY_RESULT: u32 = 0x8866;
const GPU_MEM_TOTAL_NVX: u32 = 0x9048;
const GPU_MEM_AVAIL_NVX: u32 = 0x9049;

type GenQueries = unsafe extern "system" fn(i32, *mut u32);
type BeginQuery = unsafe extern "system" fn(u32, u32);
type EndQuery = unsafe extern "system" fn(u32);
type GetQueryU64 = unsafe extern "system" fn(u32, u32, *mut u64);
type GetIntegerv = unsafe extern "system" fn(u32, *mut i32);
type GetString = unsafe extern "system" fn(u32) -> *const u8;

pub struct Gpu {
    api: Api,
    display: egl::Display,
    context: egl::Context,
    pub skia: gpu::DirectContext,
    pub platform: &'static str,
    pub renderer: String,
    timer: Option<(GenQueries, BeginQuery, EndQuery, GetQueryU64)>,
    get_integer: Option<GetIntegerv>,
    query: u32,
}

impl Gpu {
    pub fn new() -> Result<Self, String> {
        // SAFETY: loads the system libEGL.
        let api = unsafe { Api::load_required() }.map_err(|e| e.to_string())?;
        let mut last = String::new();
        for (platform, name) in [(PLATFORM_SURFACELESS_MESA, "surfaceless"), (PLATFORM_DEVICE_EXT, "device")] {
            let display = if platform == PLATFORM_DEVICE_EXT {
                // EGL_EXT_device_enumeration: first device.
                let query: Option<unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32> =
                    api.get_proc_address("eglQueryDevicesEXT").map(|f| unsafe { std::mem::transmute(f) });
                let Some(query) = query else { continue };
                let mut dev = std::ptr::null_mut();
                let mut n = 0;
                // SAFETY: one output slot.
                if unsafe { query(1, &mut dev, &mut n) } == 0 || n < 1 {
                    continue;
                }
                unsafe { api.get_platform_display(platform, dev, &[egl::ATTRIB_NONE]) }
            } else {
                unsafe { api.get_platform_display(platform, egl::DEFAULT_DISPLAY, &[egl::ATTRIB_NONE]) }
            };
            let display = match display {
                Ok(d) => d,
                Err(e) => {
                    last += &format!("[{name}: {e}] ");
                    continue;
                }
            };
            if let Err(e) = api.initialize(display) {
                last += &format!("[{name}: init {e}] ");
                continue;
            }
            match Self::context(&api, display) {
                Ok(context) => {
                    return Self::finish(api, display, context, name);
                }
                Err(e) => last += &format!("[{name}: {e}] "),
            }
        }
        Err(format!("no surfaceless EGL context ({last})"))
    }

    fn context(api: &Api, display: egl::Display) -> Result<egl::Context, String> {
        api.bind_api(egl::OPENGL_ES_API).map_err(|e| e.to_string())?;
        let config = api
            .choose_first_config(display, &[
                egl::SURFACE_TYPE, egl::PBUFFER_BIT,
                egl::RENDERABLE_TYPE, egl::OPENGL_ES2_BIT,
                egl::RED_SIZE, 8, egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8, egl::ALPHA_SIZE, 8,
                egl::STENCIL_SIZE, 8, egl::NONE,
            ])
            .map_err(|e| e.to_string())?
            .ok_or("no RGBA8 config")?;
        let context = api.create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE]).map_err(|e| e.to_string())?;
        // EGL_KHR_surfaceless_context: no draw/read surface; Skia renders into FBOs.
        api.make_current(display, None, None, Some(context)).map_err(|e| e.to_string())?;
        Ok(context)
    }

    fn finish(api: Api, display: egl::Display, context: egl::Context, platform: &'static str) -> Result<Self, String> {
        let load = |name: &str| api.get_proc_address(name).map_or(std::ptr::null(), |p| p as *const c_void);
        let interface = gpu::gl::Interface::new_load_with(load).ok_or("Skia GL interface")?;
        let skia = gpu::direct_contexts::make_gl(interface, None).ok_or("Skia GL context")?;
        let f = |name: &str| api.get_proc_address(name);
        // SAFETY: GL entry points with their specified signatures.
        let timer = unsafe {
            match (f("glGenQueriesEXT"), f("glBeginQueryEXT"), f("glEndQueryEXT"), f("glGetQueryObjectui64vEXT")) {
                (Some(g), Some(b), Some(e), Some(r)) => Some((std::mem::transmute::<extern "system" fn(), GenQueries>(g), std::mem::transmute::<extern "system" fn(), BeginQuery>(b), std::mem::transmute::<extern "system" fn(), EndQuery>(e), std::mem::transmute::<extern "system" fn(), GetQueryU64>(r))),
                _ => None,
            }
        };
        let get_integer = f("glGetIntegerv").map(|p| unsafe { std::mem::transmute::<extern "system" fn(), GetIntegerv>(p) });
        let renderer = f("glGetString")
            .map(|p| unsafe { std::mem::transmute::<extern "system" fn(), GetString>(p) })
            .and_then(|gs| {
                // SAFETY: GL_RENDERER returns a static NUL-terminated string.
                let s = unsafe { gs(0x1F01) };
                (!s.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(s.cast()) }.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        let mut query = 0;
        if let Some((gen, ..)) = timer {
            // SAFETY: one query name.
            unsafe { gen(1, &mut query) };
        }
        Ok(Self { api, display, context, skia, platform, renderer, timer, get_integer, query })
    }

    pub fn has_timer(&self) -> bool {
        self.timer.is_some()
    }

    /// GPU-side duration (µs) of the work submitted by `f`.
    pub fn timed<R>(&mut self, f: impl FnOnce(&mut gpu::DirectContext) -> R) -> (R, Option<f64>) {
        match self.timer {
            Some((_, begin, end, get)) => {
                // SAFETY: valid query object; queries do not nest.
                unsafe { begin(TIME_ELAPSED, self.query) };
                let r = f(&mut self.skia);
                unsafe { end(TIME_ELAPSED) };
                let mut ns = 0u64;
                unsafe { get(self.query, QUERY_RESULT, &mut ns) };
                (r, Some(ns as f64 / 1000.))
            }
            None => (f(&mut self.skia), None),
        }
    }

    /// NVX_gpu_memory_info (KiB): (total, available) when exposed.
    pub fn vidmem_kib(&self) -> Option<(i64, i64)> {
        let gi = self.get_integer?;
        let (mut total, mut avail) = (0i32, 0i32);
        // SAFETY: scalar queries.
        unsafe {
            gi(GPU_MEM_TOTAL_NVX, &mut total);
            gi(GPU_MEM_AVAIL_NVX, &mut avail);
        }
        (total > 0).then_some((total as i64, avail as i64))
    }

    pub fn render_target(&mut self, w: i32, h: i32) -> Option<sk::Surface> {
        gpu::surfaces::render_target(&mut self.skia, gpu::Budgeted::Yes, &sk::ImageInfo::new_n32_premul((w, h), None), None, None, None, None, None)
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        self.skia.abandon();
        let _ = self.api.make_current(self.display, None, None, None);
        let _ = self.api.destroy_context(self.display, self.context);
        let _ = self.api.terminate(self.display);
    }
}
