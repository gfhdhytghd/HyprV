//! SCTK/calloop integration. No timers are installed by the engine.
//! Only dirty or animating surfaces request a frame callback.
use crate::{
    egl::{EglWindow, SharedEgl},
    geometry::Rect,
    render::Renderer,
    scene::{NodeId, Scene},
};
use calloop::{
    channel,
    timer::{TimeoutAction, Timer},
    EventLoop, RegistrationToken,
};
use calloop_wayland_source::WaylandSource;
pub use sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use sctk::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputInfo, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    subcompositor::SubcompositorState,
};
use smithay_client_toolkit as sctk;
use std::{
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_subsurface, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};
pub use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;
use wayland_protocols::wp::{
    cursor_shape::v1::client::{
        wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1},
        wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
    },
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    presentation_time::client::{
        wp_presentation::{self, WpPresentation},
        wp_presentation_feedback::{self, WpPresentationFeedback},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
#[derive(Clone, Debug)]
pub struct LayerConfig {
    pub namespace: String,
    pub layer: Layer,
    pub anchor: Anchor,
    pub size: (u32, u32),
    pub margins: (i32, i32, i32, i32),
    pub exclusive_zone: i32,
    pub keyboard: KeyboardInteractivity,
    /// None selects the compositor's preferred output.
    pub output: Option<wl_output::WlOutput>,
    /// Empty input region for passive overlays.
    pub input_passthrough: bool,
    /// Full logical input surface backed by one transparent pixel (requires viewporter).
    pub input_only: bool,
    /// Paint the complete scene directly into the window on every damaged frame.
    /// Passive full-output artwork avoids an additional retained GPU target.
    pub full_redraw: bool,
    /// Sparse artwork on a large layer (requires viewporter and subcompositor).
    /// The layer surface keeps its role, size, input region and namespace, but its
    /// own buffer is one transparent pixel scaled by the viewport. The scene, still
    /// in layer coordinates, is presented through synchronized subsurfaces covering
    /// Scene::sparse_regions only (full-width bands around the painted children), so
    /// no layer-sized buffer is ever allocated.
    /// Subsurfaces take no input; with full_redraw they are direct windows too.
    pub sparse: bool,
}
impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            namespace: "hyprv-quickshell".into(),
            layer: Layer::Top,
            anchor: Anchor::TOP,
            size: (400, 80),
            margins: (0, 0, 0, 0),
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::None,
            output: None,
            input_passthrough: false,
            input_only: false,
            full_redraw: false,
            sparse: false,
        }
    }
}
impl LayerConfig {
    /// Zero dimensions are legal only on an axis anchored at both ends.
    pub fn legal_size(&self, size: (u32, u32)) -> (u32, u32) {
        (
            if self.anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
                size.0
            } else {
                size.0.max(1)
            },
            if self.anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
                size.1
            } else {
                size.1.max(1)
            },
        )
    }
    /// Popup-like overlay. This is a layer surface, with no xdg_popup implicit grab.
    pub fn popup(size: (u32, u32)) -> Self {
        Self {
            layer: Layer::Overlay,
            size,
            keyboard: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(u64);
/// PNG filename, namespace, logical size and buffer scale of a retained GPU surface.
pub struct SurfaceCapture {
    pub file: String,
    pub config: LayerConfig,
    pub size: (u32, u32),
    pub scale: f64,
    pub output: Option<String>,
}
#[derive(Clone, Copy, Debug)]
pub struct FrameInfo {
    /// Monotonic application time (same clock domain on every surface).
    pub time: Duration,
    /// Last actual presentation timestamp, in the compositor's advertised clock.
    pub presented: Option<Duration>,
    pub refresh: Option<Duration>,
}
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Native seat/device capability loss cancels an implicit pointer grab.
    PointerCancel,
    Scroll {
        position: (f64, f64),
        event: crate::scroll::ScrollEvent,
    },
    Pointer {
        event: PointerEvent,
        target: Option<NodeId>,
    },
    Hover {
        previous: Option<NodeId>,
        current: Option<NodeId>,
    },
    Click {
        target: NodeId,
        button: u32,
    },
    KeyboardEnter {
        seat: wl_seat::WlSeat,
        keys: Vec<Keysym>,
    },
    KeyboardLeave {
        seat: wl_seat::WlSeat,
    },
    Key {
        seat: wl_seat::WlSeat,
        event: KeyEvent,
        pressed: bool,
    },
    Modifiers {
        seat: wl_seat::WlSeat,
        modifiers: Modifiers,
        layout: u32,
    },
    Resize {
        width: u32,
        height: u32,
        scale: f64,
    },
}
#[derive(Default)]
pub struct Response {
    pub animate: bool,
    pub close: bool,
    pub cursor: Option<CursorShape>,
}
pub trait UiHandler {
    fn clock(&mut self, _now: Duration) {}
    fn output_changed(&mut self, _output: &WlOutput, _info: Option<MonitorInfo>) {}
    /// Reconcile application state after events; runs on the UI thread before scheduling.
    fn update(&mut self, _engine: &mut Engine) {}
    /// Whether a completed frame needs host reconciliation. Static hosts can
    /// return false; native buffer scheduling/flushing still runs. Default keeps
    /// existing handlers' frame-driven update behavior.
    fn update_after_frame(&self) -> bool { true }
    /// Opt in to drawing settled data changes without an empty commit/frame
    /// callback round trip. Only used when `update_after_frame` is false and a
    /// surface is not animating or waiting for a callback. Frame-driven handlers
    /// should keep the default; animations always retain compositor pacing.
    fn immediate_static_frames(&self) -> bool { false }
    /// Applications that do not use idle presentation timestamps can avoid a
    /// feedback round trip for data-only frames. The default preserves updates.
    fn presentation_feedback(&self, _surface:SurfaceId)->bool {true}
    /// A host may drive several related surfaces from one animation clock even
    /// when an individual frame() returns false. Keep direct-window drawables
    /// through that transition, then retire them without scheduling another frame.
    fn resource_transition_active(&self) -> bool { false }
    /// Optional app-epoch deadline; one single-shot timer, absent while idle.
    fn next_deadline(&self) -> Option<Duration> {
        None
    }

    /// Return true only while an animation needs another compositor frame.
    fn frame(&mut self, _surface: SurfaceId, _scene: &mut Scene, _time: FrameInfo) -> bool {
        false
    }
    fn input(&mut self, _surface: SurfaceId, _scene: &mut Scene, _event: InputEvent) -> Response {
        Response::default()
    }
    /// Called on initial discovery and hotplug. Returned scenes are output-bound automatically.
    fn output_added(
        &mut self,
        _output: &wl_output::WlOutput,
        _info: Option<OutputInfo>,
    ) -> Vec<(LayerConfig, Scene)> {
        Vec::new()
    }
    fn output_removed(&mut self, _output: &wl_output::WlOutput) {}
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Wayland: {0}")]
    Wayland(String),
    #[error(transparent)]
    Graphics(#[from] crate::egl::EglError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
type Command = Box<dyn FnOnce(&mut Engine) + Send>;
#[derive(Clone)]
pub struct Handle(channel::Sender<Command>);
impl Handle {
    /// Eventfd-backed wakeup for service diffs; no polling. Captured data must be Send.
    pub fn send(&self, command: impl FnOnce(&mut Engine) + Send + 'static) -> Result<(), Error> {
        self.0
            .send(Box::new(command))
            .map_err(|_| Error::Wayland("engine command channel closed".into()))
    }
}
struct View {
    id: SurfaceId,
    window: Option<EglWindow>,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    layer: LayerSurface,
    config: LayerConfig,
    scene: Scene,
    size: (u32, u32),
    scale: f64,
    configured: bool,
    pending: bool,
    animate: bool,
    presented: Option<Duration>,
    refresh: Option<Duration>,
    last_callback: Option<u32>,
    callback_time: Duration,
    feedback: Option<WpPresentationFeedback>,
    viewport_state: Option<((u32,u32),u64,bool)>,
    /// Requested layer size not yet sent; applied with the first buffer drawn at it.
    pending_size: Option<(u32, u32)>,
    /// LayerConfig::sparse subsurfaces, reused by index across region changes.
    tiles: Vec<Tile>,
}
impl View {
    /// Retire idle direct-window swapchains; the compositor keeps the last buffers.
    fn release_idle_drawables(&mut self) -> Result<(), crate::egl::EglError> {
        for window in self.window.iter_mut().chain(self.tiles.iter_mut().filter_map(|t| t.window.as_mut())) {
            window.release_idle_drawable()?;
        }
        Ok(())
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.tiles.clear();
        self.window.take();
        if let Some(f) = self.fractional.take() {
            f.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
        self.feedback.take();
    }
}
/// One synchronized subsurface of a sparse layer presenting a device-aligned scene
/// region. Its buffer, position and viewport apply with the next layer commit.
struct Tile {
    window: Option<EglWindow>,
    viewport: WpViewport,
    subsurface: wl_subsurface::WlSubsurface,
    surface: wl_surface::WlSurface,
    /// Logical region in layer coordinates; None while unmapped.
    rect: Option<Rect>,
    /// Committed position, viewport destination and scale bits.
    state: Option<((i32, i32), (i32, i32), u64)>,
}
impl Drop for Tile {
    fn drop(&mut self) {
        // The EGL surface and wl_egl_window go before their wl_surface.
        self.window.take();
        self.viewport.destroy();
        self.subsurface.destroy();
        self.surface.destroy();
    }
}
struct Seat {
    seat: wl_seat::WlSeat,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_seat: wl_seat::WlSeat,
    scroll: crate::scroll::ScrollState,
    pointer_focus: Option<(SurfaceId, (f64, f64))>,
    scroll_target: Option<SurfaceId>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    shape: Option<WpCursorShapeDeviceV1>,
    serial: u32,
    keyboard_focus: Option<SurfaceId>,
    hover: Option<(SurfaceId, NodeId)>,
    pressed: HashMap<u32, (SurfaceId, NodeId)>,
}
impl Drop for Seat {
    fn drop(&mut self) {
        if let Some(s) = self.shape.take() {
            s.destroy();
        }
        if let Some(p) = self.pointer.take() {
            if p.version() >= 3 {
                p.release();
            }
        }
        if let Some(k) = self.keyboard.take() {
            if k.version() >= 3 {
                k.release();
            }
        }
        if self.pointer_seat != self.seat && self.pointer_seat.version() >= 5 {
            self.pointer_seat.release();
        }
    }
}
pub struct Engine {
    connection: Connection,
    qh: QueueHandle<Self>,
    registry: RegistryState,
    pending_seat_global: Option<u32>,
    compositor: CompositorState,
    subcompositor: Option<SubcompositorState>,
    layer_shell: LayerShell,
    outputs: OutputState,
    seats: SeatState,
    input: Vec<Seat>,
    views: Vec<View>,
    next_id: u64,
    fractional_manager: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    cursor_manager: Option<WpCursorShapeManagerV1>,
    presentation: Option<WpPresentation>,
    pub presentation_clock: Option<u32>,
    graphics: Option<Rc<SharedEgl>>,
    retire_idle_drawables: bool,
    renderer: Renderer,
    handler: Option<Box<dyn UiHandler>>,
    running: bool,
    external_frame_clock: bool,
    error: Option<Error>,
    epoch: Instant,
    loop_handle: Option<calloop::LoopHandle<'static, Engine>>,
    timer: Option<RegistrationToken>,
    deadline: Option<Duration>,
    /// Completed renders and CPU time spent rendering/submitting; inspect without a sampling timer.
    pub frames: u64,
    pub render_time: Duration,
    pub first_render_time: Option<Duration>,
    pub last_render_time: Option<Duration>,
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.views.clear();
        self.renderer.clear_caches();
        self.graphics.take();
        crate::diagnostics::finish();
    }
}
pub struct Application {
    pub engine: Engine,
    event_loop: EventLoop<'static, Engine>,
    handle: Handle,
}
impl Application {
    pub fn connect(handler: impl UiHandler + 'static) -> Result<Self, Error> {
        let connection = Connection::connect_to_env().map_err(|e| Error::Wayland(e.to_string()))?;
        let (globals, mut queue) = registry_queue_init::<Engine>(&connection)
            .map_err(|e| Error::Wayland(e.to_string()))?;
        let qh = queue.handle();
        let compositor =
            CompositorState::bind(&globals, &qh).map_err(|e| Error::Wayland(e.to_string()))?;
        let layer_shell =
            LayerShell::bind(&globals, &qh).map_err(|e| Error::Wayland(e.to_string()))?;
        let subcompositor =
            SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh).ok();
        let viewport: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
        // Fractional scaling is only usable together with viewporter.
        let fractional_manager = if viewport.is_some() {
            globals.bind(&qh, 1..=1, ()).ok()
        } else {
            None
        };
        let mut engine = Engine {
            registry: RegistryState::new(&globals),
            pending_seat_global: None,
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            compositor,
            subcompositor,
            layer_shell,
            fractional_manager,
            viewporter: viewport,
            cursor_manager: globals.bind(&qh, 1..=1, ()).ok(),
            presentation: globals.bind(&qh, 1..=1, ()).ok(),
            presentation_clock: None,
            input: Vec::new(),
            views: Vec::new(),
            next_id: 1,
            graphics: None,
            retire_idle_drawables: true,
            renderer: Renderer::new(),
            handler: Some(Box::new(handler)),
            running: true,
            external_frame_clock: false,
            error: None,
            epoch: Instant::now(),
            loop_handle: None,
            timer: None,
            deadline: None,
            frames: 0,
            render_time: Duration::ZERO,
            first_render_time: None,
            last_render_time: None,
            connection: connection.clone(),
            qh,
        };
        // Prepare before dispatching output/seat/configure events: no initial
        // view can paint or accept input with cold effect/material pipelines.
        let graphics = SharedEgl::new(&connection)?;
        graphics.prepare_renderer(&mut engine.renderer)?;
        engine.graphics = Some(graphics);
        // SeatState binds these in registry order, but emits new_seat only for
        // hotplugged globals. Seed initial seats before capability delivery.
        let names: Vec<_> = engine
            .registry
            .globals_by_interface("wl_seat")
            .map(|g| g.name)
            .collect();
        let initial: Vec<_> = engine.seats.seats().collect();
        for (seat, name) in initial.into_iter().zip(names) {
            engine.pending_seat_global = Some(name);
            engine.new_seat(&connection, &engine.qh.clone(), seat);
        }
        engine.pending_seat_global = None;
        queue
            .roundtrip(&mut engine)
            .map_err(|e| Error::Wayland(e.to_string()))?;
        let event_loop = EventLoop::try_new().map_err(|e| Error::Wayland(e.to_string()))?;
        engine.loop_handle = Some(event_loop.handle());
        WaylandSource::new(connection, queue)
            .insert(event_loop.handle())
            .map_err(|e| Error::Wayland(e.to_string()))?;
        let (sender, receiver) = channel::channel::<Command>();
        event_loop
            .handle()
            .insert_source(receiver, |event, _, state| {
                if let channel::Event::Msg(command) = event {
                    command(state);
                    state.schedule();
                }
            })
            .map_err(|e| Error::Wayland(e.to_string()))?;
        Ok(Self {
            engine,
            event_loop,
            handle: Handle(sender),
        })
    }
    /// Explicit fixture clock: the caller drives update/render barriers. Native
    /// frame callbacks cannot repeatedly repaint a frozen virtual animation.
    pub fn set_external_frame_clock(&mut self, enabled: bool) {
        self.engine.external_frame_clock = enabled;
    }
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }
    pub fn event_loop_handle(&self) -> calloop::LoopHandle<'static, Engine> {
        self.event_loop.handle()
    }
    pub fn run(mut self) -> Result<(), Error> {
        self.engine.schedule();
        while self.engine.running {
            self.event_loop
                .dispatch(None, &mut self.engine)
                .map_err(|e| Error::Wayland(e.to_string()))?;
            crate::diagnostics::count("engine.dispatch");
        }
        self.engine.error.take().map_or(Ok(()), Err)
    }
    /// Dispatch once, blocking indefinitely when timeout is None. Useful for host integration/tests.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), Error> {
        self.engine.schedule();
        self.event_loop
            .dispatch(timeout, &mut self.engine)
            .map_err(|e| Error::Wayland(e.to_string()))?;
        if let Some(e) = self.engine.error.take() {
            Err(e)
        } else {
            Ok(())
        }
    }
}
impl Engine {
    /// Warm an already-built hidden component on the live renderer/context.
    /// Intended for startup/output initialization, never as an idle polling path.
    pub fn prepare_scene(&mut self, scene: &Scene, logical: (f32, f32), scale: f64) -> Result<(), crate::egl::EglError> {
        let graphics = self.graphics.clone().ok_or_else(|| crate::egl::EglError::Init("graphics unavailable".into()))?;
        graphics.prepare_scene(&mut self.renderer, scene, logical, scale)
    }
    /// Release all renderer/GPU caches between frames without destroying surfaces.
    pub fn clear_render_caches(&mut self) -> Result<(), Error> {
        if let Some(graphics) = &self.graphics {
            graphics.clear_caches(&mut self.renderer)?;
        } else {
            self.renderer.clear_caches();
        }
        Ok(())
    }

    /// Explicit capture/replay barrier: render configured retained scenes via the
    /// same EGL path as presentation. Installs no timer and changes no input devices.
    pub fn renderer_name(&self) -> Option<String> {
        self.graphics.as_ref().and_then(|g| g.renderer_name())
    }
    /// Debug native-compositor negative control; production always retires idle
    /// direct-window drawables after their last submitted frame.
    #[cfg(debug_assertions)]
    pub fn retain_idle_drawables_for_test(&mut self) { self.retire_idle_drawables = false; }
    /// Invokes clock/frame callbacks; release host RefCell guards before calling.
    pub fn render_now(&mut self) {
        let ids: Vec<_> = self
            .views
            .iter()
            .filter(|v| v.configured)
            .map(|v| v.id)
            .collect();
        for id in ids {
            if let Some(scene) = self.scene(id) {
                scene.invalidate();
            }
            self.paint(id, None);
        }
    }
    /// Host-local synthetic input for deterministic replay, without compositor injection.
    /// Invokes input/update synchronously; release host RefCell guards first.
    pub fn replay_input(&mut self, id: SurfaceId, event: InputEvent) {
        self.emit(id, event);
        self.schedule();
    }
    pub fn surface_geometry(&self, id: SurfaceId) -> Option<((u32, u32), f64)> {
        self.views
            .iter()
            .find(|v| v.id == id)
            .map(|v| (v.size, v.scale))
    }
    pub fn surface_proxy(&self, id: SurfaceId) -> Option<wl_surface::WlSurface> {
        self.views
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.layer.wl_surface().clone())
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    /// Dump all configured, rendered surface buffers. Uses retained GPU readback,
    /// not a second CPU rendering pass; callers may separately compose the PNGs.
    pub fn dump_surfaces(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<Vec<SurfaceCapture>, Error> {
        std::fs::create_dir_all(directory).map_err(|e| Error::Wayland(e.to_string()))?;
        let mut captures = Vec::new();
        for (index, view) in self.views.iter_mut().enumerate() {
            if let Some(window) = view.window.as_mut() {
                let name = format!("surface-{index:02}.png");
                if view.config.full_redraw || view.config.sparse {
                    let logical=(view.size.0 as f32,view.size.1 as f32);
                    window.capture(&mut self.renderer,
                        &view.scene,logical,view.scale,&directory.join(&name))?;
                } else {
                    window.png(&directory.join(&name))?;
                }
                let output = view
                    .config
                    .output
                    .as_ref()
                    .and_then(|o| self.outputs.info(o))
                    .and_then(|o| o.name);
                captures.push(SurfaceCapture {
                    file: name,
                    config: view.config.clone(),
                    size: view.size,
                    scale: view.scale,
                    output,
                });
            }
        }
        Ok(captures)
    }
    pub fn surfaces(&self) -> Vec<(SurfaceId, LayerConfig)> {
        self.views
            .iter()
            .map(|v| (v.id, v.config.clone()))
            .collect()
    }
    pub fn replace_scene(&mut self, id: SurfaceId, scene: Scene) {
        if let Some(v) = self.views.iter_mut().find(|v| v.id == id) {
            v.scene = scene;
        }
    }
    /// Move a retained scene when rebinding a native surface. The caller must
    /// replace it or close this surface before returning to event dispatch.
    pub fn take_scene(&mut self, id: SurfaceId) -> Option<Scene> {
        self.views.iter_mut().find(|v| v.id == id).map(|v| {
            std::mem::replace(
                &mut v.scene,
                Scene::new(crate::scene::Node::new(
                    Rect::default(),
                    crate::scene::Content::Group,
                )),
            )
        })
    }
    /// Update layer policy. On mapped surfaces, size is published atomically
    /// with the next correctly sized buffer; zero axes retain compositor sizing.
    pub fn configure_surface(
        &mut self,
        id: SurfaceId,
        size: (u32, u32),
        layer: Layer,
        zone: i32,
        keyboard: KeyboardInteractivity,
    ) {
        if let Some(v) = self.views.iter_mut().find(|v| v.id == id) {
            let size = v.config.legal_size(size);
            if v.config.size != size {
                v.config.size = size;
                // Layer size is double-buffered. Committing it alone makes the compositor
                // scale the old-size buffer into the new box until a new buffer arrives
                // (a 820->58 bar shrink shows a squashed, invisible bar for ~3 frames).
                // Defer it to the next paint so it lands in the same commit as a buffer
                // drawn at that size.
                if v.configured {
                    v.pending_size = Some(size);
                    v.scene.invalidate();
                } else {
                    v.layer.set_size(size.0, size.1);
                }
            }
            if v.config.layer != layer {
                v.config.layer = layer;
                v.layer.set_layer(layer);
            }
            if v.config.exclusive_zone != zone {
                v.config.exclusive_zone = zone;
                v.layer.set_exclusive_zone(zone);
            }
            if v.config.keyboard != keyboard {
                v.config.keyboard = keyboard;
                v.layer.set_keyboard_interactivity(keyboard);
            }
            v.layer.commit();
        }
    }
    /// Union in logical surface coordinates; Some(empty) makes an overlay click-through.
    pub fn set_input_region(&mut self, id: SurfaceId, rects: Option<&[Rect]>) {
        if let Some(v) = self.views.iter().find(|v| v.id == id) {
            let region = rects.map(|rects| {
                let region = self.compositor.wl_compositor().create_region(&self.qh, ());
                for r in rects {
                    let (x, y, w, h) = r.physical(1.);
                    if w > 0 && h > 0 {
                        region.add(x, y, w, h);
                    }
                }
                region
            });
            v.layer.wl_surface().set_input_region(region.as_ref());
            if let Some(r) = region {
                r.destroy();
            }
            v.layer.commit();
        }
    }
    pub fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
    pub fn outputs(&self) -> Vec<(wl_output::WlOutput, Option<OutputInfo>)> {
        self.outputs
            .outputs()
            .map(|o| {
                let i = self.outputs.info(&o);
                (o, i)
            })
            .collect()
    }
    pub fn add_surface(&mut self, mut config: LayerConfig, mut scene: Scene) -> SurfaceId {
        let _timing = crate::diagnostics::scope("engine.add_surface");
        crate::diagnostics::mark("surface.created", &config.namespace);
        scene.invalidate();
        config.input_only &= self.viewporter.is_some();
        config.sparse &= self.viewporter.is_some() && self.subcompositor.is_some() && !config.input_only;
        config.size = config.legal_size(config.size);
        let id = SurfaceId(self.next_id);
        self.next_id += 1;
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            config.layer,
            Some(config.namespace.clone()),
            config.output.as_ref(),
        );
        layer.set_anchor(config.anchor);
        layer.set_size(config.size.0, config.size.1);
        layer.set_margin(
            config.margins.0,
            config.margins.1,
            config.margins.2,
            config.margins.3,
        );
        layer.set_exclusive_zone(config.exclusive_zone);
        layer.set_keyboard_interactivity(config.keyboard);
        if config.input_passthrough {
            let region = self.compositor.wl_compositor().create_region(&self.qh, ());
            layer.wl_surface().set_input_region(Some(&region));
            region.destroy();
        }
        let viewport = self
            .viewporter
            .as_ref()
            .map(|v| v.get_viewport(layer.wl_surface(), &self.qh, ()));
        let fractional = self
            .fractional_manager
            .as_ref()
            .map(|m| m.get_fractional_scale(layer.wl_surface(), &self.qh, id));
        layer.commit();
        let size = config.size;
        self.views.push(View {
            id,
            window: None,
            fractional,
            viewport,
            layer,
            config,
            scene,
            size,
            scale: 1.,
            configured: false,
            pending: false,
            animate: false,
            presented: None,
            refresh: None,
            last_callback: None,
            callback_time: Duration::ZERO,
            feedback: None,
            viewport_state: None,
            pending_size: None,
            tiles: Vec::new(),
        });
        id
    }
    /// Advanced layer-shell controls. Commit changes explicitly via WaylandSurface::commit.
    /// For mapped size changes use configure_surface; direct set_size + commit
    /// bypasses its atomic size/buffer publication and can scale stale content.
    pub fn layer_surface(&self, id: SurfaceId) -> Option<&LayerSurface> {
        self.views.iter().find(|v| v.id == id).map(|v| &v.layer)
    }
    /// Flush scene edits made directly on the UI thread. Handle::send does this automatically.
    /// Calls the attached UiHandler::update synchronously: release host RefCell
    /// borrows before calling. During update the handler is temporarily detached,
    /// so scheduling from update itself does not re-enter the handler.
    pub fn request_redraw(&mut self) {
        self.schedule();
    }
    pub fn scene(&mut self, id: SurfaceId) -> Option<&mut Scene> {
        self.views
            .iter_mut()
            .find(|v| v.id == id)
            .map(|v| &mut v.scene)
    }
    /// Schedule animation; shares request_redraw's synchronous handler contract.
    pub fn animate(&mut self, id: SurfaceId) {
        if let Some(v) = self.views.iter_mut().find(|v| v.id == id) {
            v.animate = true;
        }
        self.schedule();
    }
    fn cancel_seat_pointer(&mut self, seat: &wl_seat::WlSeat) {
        let mut ids: Vec<_> = self
            .input
            .iter()
            .filter(|s| s.seat == *seat)
            .flat_map(|s| {
                s.pressed
                    .values()
                    .map(|(id, _)| *id)
                    .chain(s.pointer_focus.map(|f| f.0))
            })
            .collect();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        for id in ids {
            self.emit(id, InputEvent::PointerCancel);
        }
    }
    pub fn close(&mut self, id: SurfaceId) {
        self.views.retain(|v| v.id != id);
        self.renderer.retain_scenes(self.views.iter().map(|v| &v.scene));
        for seat in &mut self.input {
            if seat.scroll_target == Some(id) || seat.pointer_focus.is_some_and(|f| f.0 == id) {
                seat.pointer_focus = None;
                seat.scroll_target = None;
                seat.scroll = Default::default();
            }
            seat.pressed.retain(|_, (s, _)| *s != id);
            if seat.keyboard_focus == Some(id) {
                seat.keyboard_focus = None;
            }
            if seat.hover.is_some_and(|(s, _)| s == id) {
                seat.hover = None;
            }
        }
    }
    pub fn quit(&mut self) {
        self.running = false;
    }
    pub fn surface_count(&self) -> usize {
        self.views.len()
    }
    pub fn set_cursor(&mut self, seat: &wl_seat::WlSeat, shape: CursorShape) {
        if let Some(s) = self.input.iter().find(|s| s.seat == *seat) {
            if let Some(device) = &s.shape {
                device.set_shape(s.serial, shape);
            }
        }
    }
    fn schedule(&mut self) { self.schedule_with_update(true); }
    fn schedule_with_update(&mut self, update: bool) {
        let _timing = crate::diagnostics::scope("engine.schedule");
        if self.handler.is_none() {
            return;
        }
        if update {
            if let Some(mut handler) = self.handler.take() {
                handler.update(self);
                self.handler = Some(handler);
            }
        }
        let deadline = self.handler.as_ref().and_then(|h| h.next_deadline());
        if self.deadline != deadline {
            if let Some(handle) = self.loop_handle.clone() {
                if let Some(token) = self.timer.take() {
                    handle.remove(token);
                }
                self.deadline = deadline;
                if let Some(at) = deadline {
                    match handle.insert_source(
                        Timer::from_duration(at.saturating_sub(self.now())),
                        |_, _, state| {
                            state.deadline = None;
                            state.timer = None;
                            state.schedule();
                            TimeoutAction::Drop
                        },
                    ) {
                        Ok(token) => self.timer = Some(token),
                        Err(e) => {
                            self.error = Some(Error::Wayland(e.to_string()));
                            self.running = false;
                        }
                    }
                }
            }
        }

        if !self.external_frame_clock && self.handler.as_ref().is_some_and(|h| {
            h.immediate_static_frames() && !h.update_after_frame()
        }) {
            let ready: Vec<_> = self.views.iter().filter(|v| {
                v.configured && !v.pending && !v.animate && v.scene.is_dirty()
            }).map(|v| v.id).collect();
            for id in ready {
                crate::diagnostics::count("engine.static_immediate");
                self.paint_inner(id, None, false);
                if !self.running { return; }
            }
        }
        for view in &mut self.views {
            if !self.external_frame_clock
                && view.configured
                && !view.pending
                && (view.animate || view.scene.is_dirty())
            {
                view.layer
                    .wl_surface()
                    .frame(&self.qh, view.layer.wl_surface().clone());
                crate::diagnostics::count("engine.frame_callback_requested");
                view.pending = true;
                view.layer.commit();
            }
        }
        if self.retire_idle_drawables && !self.handler.as_ref().is_some_and(|h|h.resource_transition_active()) {
            for view in &mut self.views {
                if view.config.full_redraw && !view.animate && !view.pending {
                    if let Err(error) = view.release_idle_drawables() {
                        self.error = Some(error.into()); self.running = false; return;
                    }
                }
            }
        }
        if let Err(e) = self.connection.flush() {
            if matches!(&e,wayland_client::backend::WaylandError::Io(err) if err.kind()==std::io::ErrorKind::WouldBlock)
            {
                return;
            }
            self.error = Some(Error::Wayland(e.to_string()));
            self.running = false;
        }
    }
    fn emit(&mut self, id: SurfaceId, event: InputEvent) -> Response {
        let now = self.now();
        if let Some(handler) = self.handler.as_mut() {
            handler.clock(now);
        }
        let mut response = Response::default();
        if let Some(v) = self.views.iter_mut().find(|v| v.id == id) {
            response = self
                .handler
                .as_mut()
                .unwrap()
                .input(id, &mut v.scene, event);
            v.animate |= response.animate;
        }
        if response.close {
            self.close(id);
        }
        response
    }
    fn surface_id(&self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.views
            .iter()
            .find(|v| v.layer.wl_surface() == surface)
            .map(|v| v.id)
    }
    fn paint(&mut self, id: SurfaceId, time: Option<u32>) {
        self.paint_inner(id, time, true);
    }
    fn paint_inner(&mut self, id: SurfaceId, time: Option<u32>, reschedule: bool) {
        let _timing = crate::diagnostics::scope("engine.paint");
        // After a fatal error (e.g. a protocol error) nothing may be drawn or committed again.
        if !self.running {
            return;
        }
        let Some(view) = self.views.iter_mut().find(|v| v.id == id) else {
            return;
        };
        if !view.configured {
            return;
        }
        view.pending = false;
        if let Some(size) = view.pending_size.take() {
            view.layer.set_size(size.0, size.1);
            // Zero means compositor-selected on a stretched axis, not a zero
            // pixel buffer. Retain that axis's last configure until the next
            // compositor configure; explicit axes resize atomically below.
            view.size = (
                if size.0 == 0 { view.size.0 } else { size.0 },
                if size.1 == 0 { view.size.1 } else { size.1 },
            );
            view.scene.invalidate();
        }
        let now = self.epoch.elapsed();
        // wl_callback time wraps at 2^32 milliseconds. Anchor it to the app's monotonic epoch.
        let frame_time = if let Some(t) = time {
            if let Some(last) = view.last_callback {
                view.callback_time += Duration::from_millis(t.wrapping_sub(last) as u64);
            } else {
                view.callback_time = now;
            }
            view.last_callback = Some(t);
            view.callback_time
        } else {
            // Do not move the callback anchor: the next animation callback's
            // delta already includes the intervening idle time.
            now
        };
        view.animate = self.handler.as_mut().unwrap().frame(
            id,
            &mut view.scene,
            FrameInfo {
                time: frame_time,
                presented: view.presented,
                refresh: view.refresh,
            },
        );
        // Input-only and sparse layers own one transparent viewport-scaled pixel.
        let one_pixel = view.config.input_only || view.config.sparse;
        let logical = if one_pixel {
            (1., 1.)
        } else {
            (view.size.0 as f32, view.size.1 as f32)
        };
        let render_scale = if one_pixel {
            1.
        } else {
            view.scale
        };
        let extent = Rect::new(0., 0., logical.0, logical.1);
        let viewport_state=(view.size,render_scale.to_bits(),one_pixel);
        // Sparse scene damage (layer coordinates) belongs to the subsurfaces. The layer's
        // pixel is redrawn only with a new viewport: Hyprland resizes a layer on a buffer
        // commit, not on a viewport-only commit. Other frames commit subsurface state.
        let (damages, tile_damages) = if view.config.sparse {
            let surface = Rect::new(0., 0., view.size.0 as f32, view.size.1 as f32);
            let tiles = view.scene.take_damage_regions(surface);
            let pixel = view.window.is_none() || view.viewport_state != Some(viewport_state);
            (if pixel { vec![extent] } else { Vec::new() }, tiles)
        } else {
            (view.scene.take_damage_regions(extent), Vec::new())
        };
        if !damages.is_empty() || !tile_damages.is_empty() {
            crate::diagnostics::mark("frame.begin", &view.config.namespace);
            let started = Instant::now();
            let blank = (view.config.sparse && !damages.is_empty()).then(|| Scene::new(crate::scene::Node::new(
                Rect::default(), crate::scene::Content::Group)));
            let result: Result<(),crate::egl::EglError> = (|| {
                if self.graphics.is_none() {
                    self.graphics = Some(SharedEgl::new(&self.connection)?);
                }
                // Subsurface commits are cached until the layer commit below.
                if !tile_damages.is_empty() {
                    let (Some(subcompositor), Some(viewporter)) = (&self.subcompositor, &self.viewporter) else {
                        unreachable!("sparse layers require subcompositor and viewporter");
                    };
                    paint_tiles(view, &tile_damages, &Present {
                        graphics: self.graphics.as_ref().unwrap(),
                        subcompositor,
                        compositor: &self.compositor,
                        viewporter,
                        qh: &self.qh,
                    }, &mut self.renderer)?;
                }
                let size = crate::render::physical_size(extent.width, extent.height, render_scale)
                    .map_err(crate::egl::EglError::from)?;
                // The buffer committed below must have exactly the size that the viewport
                // source describes for the latest configure. A wl_egl_window resize only
                // reaches a later buffer (NVIDIA's egl-wayland can already hold this frame's
                // back buffer at the old size), which paired an old buffer with the new
                // source after an output rotation: wp_viewport "Box doesn't fit".
                // A size change (configure or scale) therefore recreates the EGL window,
                // whose first buffer is created at the new size.
                if !damages.is_empty() && view.window.as_ref().is_some_and(|w| w.size() != size) {
                    view.window = None;
                }
                if !damages.is_empty() && view.window.is_none() {
                    let _timing = crate::diagnostics::scope("engine.egl_create");
                    view.window = Some(EglWindow::new(
                        self.graphics.as_ref().unwrap().clone(),
                        view.layer.wl_surface(),
                        size,
                    )?);
                }
                if let Some(window) = view.window.as_mut() {
                    window.set_full_redraw(view.config.full_redraw);
                }
                if view.viewport_state!=Some(viewport_state) {
                    if let Some(v)=&view.viewport {
                        view.layer.wl_surface().set_buffer_scale(1);
                        v.set_destination(view.size.0 as i32,view.size.1 as i32);
                        v.set_source(0.,0.,logical.0 as f64*render_scale,logical.1 as f64*render_scale);
                    }else{view.layer.wl_surface().set_buffer_scale(view.scale as i32);}
                    view.viewport_state=Some(viewport_state);
                }
                for damage in &damages {
                    let (x,y,w,h)=damage.physical(render_scale);
                    view.layer.wl_surface().damage_buffer(x,y,w,h);
                }
                // Request before the swap's implicit commit, never after it.
                if view.animate {
                    view.layer
                        .wl_surface()
                        .frame(&self.qh, view.layer.wl_surface().clone());
                    crate::diagnostics::count("engine.frame_callback_requested");
                    view.pending = true;
                }
                if view.feedback.is_none() && (view.presented.is_none() || self.handler.as_ref().unwrap().presentation_feedback(id)) {
                    if let Some(p) = &self.presentation {
                        view.feedback = Some(p.feedback(view.layer.wl_surface(), &self.qh, id));
                    }
                }
                if damages.is_empty() {
                    // Sparse frame: the layer keeps its pixel; this commit applies the
                    // cached subsurface buffers, positions and viewports atomically.
                    view.layer.commit();
                    return Ok(());
                }
                view.window.as_mut().unwrap().draw_regions(
                    &mut self.renderer,
                    blank.as_ref().unwrap_or(&view.scene),
                    (extent.width, extent.height),
                    render_scale,
                    damages,
                )?;
                Ok(())
            })();
            if let Err(e) = result {
                self.error = Some(e.into());
                self.running = false;
                return;
            }
            crate::diagnostics::mark("frame.submitted", &view.config.namespace);
            self.frames += 1;
            let elapsed = started.elapsed();
            self.render_time += elapsed;
            self.first_render_time.get_or_insert(elapsed);
            self.last_render_time = Some(elapsed);
        }
        if self.retire_idle_drawables && view.config.full_redraw && !view.animate
            && !self.handler.as_ref().is_some_and(|h|h.resource_transition_active()) {
            if let Err(error) = view.release_idle_drawables() {
                self.error = Some(error.into()); self.running = false; return;
            }
        }
        if reschedule {
            let update = self.handler.as_ref().is_some_and(|handler| handler.update_after_frame());
            self.schedule_with_update(update);
        }
    }
}
/// Engine globals a sparse layer presents its subsurfaces with.
struct Present<'a> {
    graphics: &'a Rc<SharedEgl>,
    subcompositor: &'a SubcompositorState,
    compositor: &'a CompositorState,
    viewporter: &'a WpViewporter,
    qh: &'a QueueHandle<Engine>,
}
/// Draw the damaged regions of a sparse layer, each into its own region-sized
/// subsurface buffer. Commits are cached (synchronized subsurfaces); the caller's
/// layer commit publishes them together with positions and viewports.
fn paint_tiles(view: &mut View, damages: &[Rect], present: &Present, renderer: &mut Renderer)
    -> Result<(), crate::egl::EglError>
{
    let _timing = crate::diagnostics::scope("engine.sparse_tiles");
    let scale = view.scale;
    let surface = Rect::new(0., 0., view.size.0 as f32, view.size.1 as f32);
    let regions = view.scene.sparse_regions(surface, scale);
    for (index, rect) in regions.iter().copied().enumerate() {
        if view.tiles.len() == index {
            let (subsurface, wl) = present.subcompositor.create_subsurface(view.layer.wl_surface().clone(), present.qh);
            // Input stays with the layer surface and its own input region.
            let region = present.compositor.wl_compositor().create_region(present.qh, ());
            wl.set_input_region(Some(&region));
            region.destroy();
            let viewport = present.viewporter.get_viewport(&wl, present.qh, ());
            view.tiles.push(Tile { window: None, viewport, subsurface, surface: wl, rect: None, state: None });
        }
        let tile = &mut view.tiles[index];
        let size = crate::render::physical_size(rect.width, rect.height, scale)?;
        let state = ((rect.x as i32, rect.y as i32), (rect.width as i32, rect.height as i32), scale.to_bits());
        let local = Rect::new(0., 0., rect.width, rect.height);
        let regions: Vec<Rect> = if tile.rect != Some(rect) || tile.state != Some(state) {
            vec![local]
        } else {
            damages.iter().filter_map(|d| {
                let x0 = d.x.max(rect.x);
                let y0 = d.y.max(rect.y);
                let x1 = (d.x + d.width).min(rect.x + rect.width);
                let y1 = (d.y + d.height).min(rect.y + rect.height);
                (x1 > x0 && y1 > y0).then(|| Rect::new(x0 - rect.x, y0 - rect.y, x1 - x0, y1 - y0))
            }).collect()
        };
        if regions.is_empty() {
            continue;
        }
        // As for layer windows: a new extent gets a new EGL window whose first buffer
        // has that size. The old swapchain is released before the new one exists.
        if tile.window.as_ref().is_some_and(|w| w.size() != size) {
            tile.window = None;
        }
        if tile.window.is_none() {
            tile.window = Some(EglWindow::new(present.graphics.clone(), &tile.surface, size)?);
        }
        if tile.state != Some(state) {
            if tile.state.is_none_or(|s| s.0 != state.0) {
                tile.subsurface.set_position(state.0 .0, state.0 .1);
            }
            tile.surface.set_buffer_scale(1);
            tile.viewport.set_destination(state.1 .0, state.1 .1);
            tile.viewport.set_source(0., 0., size.0 as f64, size.1 as f64);
            tile.state = Some(state);
        }
        for damage in &regions {
            let (x, y, w, h) = damage.physical(scale);
            tile.surface.damage_buffer(x, y, w, h);
        }
        let window = tile.window.as_mut().unwrap();
        window.set_full_redraw(view.config.full_redraw);
        window.set_origin((rect.x, rect.y));
        window.draw_regions(renderer, &view.scene, (rect.width, rect.height), scale, regions)?;
        tile.rect = Some(rect);
    }
    // Surplus subsurfaces unmap with the same layer commit; kept for reuse.
    for tile in view.tiles.iter_mut().skip(regions.len()) {
        if tile.rect.take().is_some() {
            tile.window = None;
            tile.state = None;
            tile.surface.attach(None, 0, 0);
            tile.surface.commit();
        }
    }
    Ok(())
}
impl CompositorHandler for Engine {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        s: &wl_surface::WlSurface,
        f: i32,
    ) {
        if let Some(v) = self.views.iter_mut().find(|v| v.layer.wl_surface() == s) {
            if v.fractional.is_none() {
                v.scale = f.max(1) as f64;
                v.scene.invalidate();
            }
        }
        self.schedule();
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, s: &wl_surface::WlSurface, t: u32) {
        if let Some(id) = self.surface_id(s) {
            if self.external_frame_clock {
                if let Some(v) = self.views.iter_mut().find(|v| v.id == id) {
                    v.pending = false;
                }
                return;
            }
            self.paint(id, Some(t));
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl OutputHandler for Engine {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let specs = self
            .handler
            .as_mut()
            .unwrap()
            .output_added(&output, self.outputs.info(&output));
        for (mut config, scene) in specs {
            config.output = Some(output.clone());
            self.add_surface(config, scene);
        }
        self.schedule();
    }
    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let info = self.outputs.info(&output);
        self.handler.as_mut().unwrap().output_changed(&output, info);
        self.schedule();
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, o: wl_output::WlOutput) {
        self.handler.as_mut().unwrap().output_removed(&o);
        let ids: Vec<_> = self
            .views
            .iter()
            .filter(|v| v.config.output.as_ref() == Some(&o))
            .map(|v| v.id)
            .collect();
        for id in ids {
            self.close(id);
        }
        self.schedule();
    }
}
impl LayerShellHandler for Engine {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(id) = self.surface_id(layer.wl_surface()) {
            self.close(id);
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(v) = self.views.iter_mut().find(|v| v.layer == *layer) else {
            return;
        };
        let id = v.id;
        let first = !v.configured;
        crate::diagnostics::mark("surface.configure", &v.config.namespace);
        v.size = (
            if c.new_size.0 > 0 {
                c.new_size.0
            } else {
                v.config.size.0.max(1)
            },
            if c.new_size.1 > 0 {
                c.new_size.1
            } else {
                v.config.size.1.max(1)
            },
        );
        v.configured = true;
        v.scene.invalidate();
        let (w, h) = v.size;
        let scale = v.scale;
        self.emit(
            id,
            InputEvent::Resize {
                width: w,
                height: h,
                scale,
            },
        );
        if first {
            self.paint(id, None);
        } else {
            self.schedule();
        }
    }
}
impl SeatHandler for Engine {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        // SCTK 0.19 caps seats at v7. A second binding to the SAME registry
        // global negotiates v9 for value120 and relative_direction; keyboards
        // stay on SCTK's binding. The upgraded seat has no separate devices.
        let pointer_seat = self
            .pending_seat_global
            .take()
            .and_then(|name| self.registry.bind_specific(qh, name, 1..=9, ()).ok())
            .unwrap_or_else(|| seat.clone());
        self.input.push(Seat {
            seat,
            pointer_seat,
            scroll: Default::default(),
            pointer_focus: None,
            scroll_target: None,
            pointer: None,
            keyboard: None,
            shape: None,
            serial: 0,
            keyboard_focus: None,
            hover: None,
            pressed: HashMap::new(),
        });
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        c: Capability,
    ) {
        let Some(s) = self.input.iter_mut().find(|s| s.seat == seat) else {
            return;
        };
        if c == Capability::Pointer && s.pointer.is_none() {
            let pointer = s
                .pointer_seat
                .get_pointer(qh, sctk::seat::pointer::PointerData::new(seat.clone()));
            s.shape = self
                .cursor_manager
                .as_ref()
                .map(|m| m.get_pointer(&pointer, qh, ()));
            s.pointer = Some(pointer);
        }
        if c == Capability::Keyboard && s.keyboard.is_none() {
            s.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        c: Capability,
    ) {
        if c == Capability::Pointer {
            self.cancel_seat_pointer(&seat);
        }
        if let Some(s) = self.input.iter_mut().find(|s| s.seat == seat) {
            if c == Capability::Pointer {
                if let Some(shape) = s.shape.take() {
                    shape.destroy();
                }
                if let Some(p) = s.pointer.take() {
                    if p.version() >= 3 {
                        p.release();
                    }
                }
                s.hover = None;
                s.pressed.clear();
                s.pointer_focus = None;
                s.scroll_target = None;
                s.scroll = Default::default();
            }
            if c == Capability::Keyboard {
                if let Some(k) = s.keyboard.take() {
                    if k.version() >= 3 {
                        k.release();
                    }
                }
                s.keyboard_focus = None;
            }
        }
        self.schedule();
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.cancel_seat_pointer(&seat);
        self.input.retain(|s| s.seat != seat);
        self.schedule();
    }
}
impl PointerHandler for Engine {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(seat_index) = self
            .input
            .iter()
            .position(|s| s.pointer.as_ref() == Some(pointer))
        else {
            return;
        };
        for e in events {
            let Some(id) = self.surface_id(&e.surface) else {
                continue;
            };
            let target = self
                .views
                .iter()
                .find(|v| v.id == id)
                .and_then(|v| v.scene.hit_test(e.position.0 as f32, e.position.1 as f32));
            let mut click = None;
            let mut hover = None;
            {
                let seat = &mut self.input[seat_index];
                if matches!(e.kind, PointerEventKind::Leave { .. }) {
                    seat.pointer_focus = None;
                } else {
                    seat.pointer_focus = Some((id, e.position));
                }
                match &e.kind {
                    PointerEventKind::Enter { serial } => {
                        seat.serial = *serial;
                        if let Some(device) = &seat.shape {
                            device.set_shape(*serial, Shape::Default);
                        }
                    }
                    PointerEventKind::Leave { .. } => {
                        hover = Some((seat.hover.take().map(|(_, n)| n), None));
                    }
                    PointerEventKind::Press { button, .. } => {
                        if let Some(t) = target {
                            seat.pressed.insert(*button, (id, t));
                        }
                    }
                    PointerEventKind::Release { button, .. } => {
                        if let Some((s, t)) = seat.pressed.remove(button) {
                            if s == id && Some(t) == target {
                                click = Some((t, *button));
                            }
                        }
                    }
                    _ => {}
                }
                if !matches!(e.kind, PointerEventKind::Leave { .. }) {
                    let next = target.map(|n| (id, n));
                    if seat.hover != next {
                        hover = Some((seat.hover.map(|(_, n)| n), target));
                        seat.hover = next;
                    }
                }
            }
            if let Some((previous, current)) = hover {
                self.emit(id, InputEvent::Hover { previous, current });
            }
            let response = self.emit(
                id,
                InputEvent::Pointer {
                    event: e.clone(),
                    target,
                },
            );
            if let Some(shape) = response.cursor {
                let seat = self.input[seat_index].seat.clone();
                self.set_cursor(&seat, shape);
            }
            if let Some((target, button)) = click {
                self.emit(id, InputEvent::Click { target, button });
            }
        }
        self.schedule();
    }
}
impl KeyboardHandler for Engine {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keys: &[Keysym],
    ) {
        let Some(id) = self.surface_id(surface) else {
            return;
        };
        if let Some(s) = self
            .input
            .iter_mut()
            .find(|s| s.keyboard.as_ref() == Some(keyboard))
        {
            s.keyboard_focus = Some(id);
            let seat = s.seat.clone();
            self.emit(
                id,
                InputEvent::KeyboardEnter {
                    seat,
                    keys: keys.to_vec(),
                },
            );
        }
        self.schedule();
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(s) = self
            .input
            .iter_mut()
            .find(|s| s.keyboard.as_ref() == Some(keyboard))
        {
            if let Some(id) = s.keyboard_focus.take() {
                let seat = s.seat.clone();
                self.emit(id, InputEvent::KeyboardLeave { seat });
            }
        }
        self.schedule();
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        _: u32,
        e: KeyEvent,
    ) {
        self.key(k, e, true);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        _: u32,
        e: KeyEvent,
    ) {
        self.key(k, e, false);
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        layout: u32,
    ) {
        if let Some(s) = self.input.iter().find(|s| s.keyboard.as_ref() == Some(k)) {
            if let Some(id) = s.keyboard_focus {
                let seat = s.seat.clone();
                self.emit(
                    id,
                    InputEvent::Modifiers {
                        seat,
                        modifiers,
                        layout,
                    },
                );
            }
        }
        self.schedule();
    }
}
impl Engine {
    fn key(&mut self, k: &wl_keyboard::WlKeyboard, event: KeyEvent, pressed: bool) {
        if let Some(s) = self.input.iter().find(|s| s.keyboard.as_ref() == Some(k)) {
            if let Some(id) = s.keyboard_focus {
                let seat = s.seat.clone();
                self.emit(
                    id,
                    InputEvent::Key {
                        seat,
                        event,
                        pressed,
                    },
                );
            }
        }
        self.schedule();
    }
}
impl Dispatch<WpFractionalScaleV1, SurfaceId> for Engine {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            if let Some(v) = state.views.iter_mut().find(|v| v.id == *id) {
                v.scale = scale as f64 / 120.;
                v.scene.invalidate();
                let (w, h) = v.size;
                // Preferred scale may precede the first layer configure; its size is
                // still (0, 0), not a usable surface extent for consumers.
                if v.configured {
                    state.emit(
                        *id,
                        InputEvent::Resize {
                            width: w,
                            height: h,
                            scale: scale as f64 / 120.,
                        },
                    );
                }
            }
            state.schedule();
        }
    }
}
impl Dispatch<WpPresentation, ()> for Engine {
    fn event(
        state: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.presentation_clock = Some(clk_id);
        }
    }
}
impl Dispatch<WpPresentationFeedback, SurfaceId> for Engine {
    fn event(
        state: &mut Self,
        proxy: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(v) = state.views.iter_mut().find(|v| v.id == *id) {
            match event {
                wp_presentation_feedback::Event::Presented {
                    tv_sec_hi,
                    tv_sec_lo,
                    tv_nsec,
                    refresh,
                    ..
                } => {
                    v.presented = Some(Duration::new(
                        ((tv_sec_hi as u64) << 32) | tv_sec_lo as u64,
                        tv_nsec,
                    ));
                    v.refresh = Some(Duration::from_nanos(refresh as u64));
                    if v.feedback.as_ref() == Some(proxy) {
                        v.feedback = None;
                    }
                }
                wp_presentation_feedback::Event::Discarded
                    if v.feedback.as_ref() == Some(proxy) =>
                {
                    v.feedback = None;
                }
                _ => {}
            }
        }
    }
}
sctk::delegate_compositor!(Engine);
sctk::delegate_output!(Engine);
sctk::delegate_seat!(Engine);
sctk::delegate_keyboard!(Engine);
sctk::delegate_pointer!(Engine, pointer: []);
wayland_client::delegate_noop!(Engine:ignore wl_seat::WlSeat);
sctk::delegate_layer!(Engine);
sctk::delegate_subcompositor!(Engine);
sctk::delegate_registry!(Engine);
impl ProvidesRegistryState for Engine {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, ExtendedSeats];
}
wayland_client::delegate_noop!(Engine:ignore WpFractionalScaleManagerV1);
wayland_client::delegate_noop!(Engine:ignore WpViewporter);
wayland_client::delegate_noop!(Engine:ignore WpViewport);
wayland_client::delegate_noop!(Engine:ignore WpCursorShapeManagerV1);
wayland_client::delegate_noop!(Engine:ignore WpCursorShapeDeviceV1);
wayland_client::delegate_noop!(Engine:ignore wayland_client::protocol::wl_region::WlRegion);

pub use sctk::output::OutputInfo as MonitorInfo;
/// Logical output size. It is xdg-output's logical size when known (already rotated and
/// scaled by the compositor); otherwise the current mode divided by the integer scale,
/// with width and height swapped for 90/270-degree transforms, flipped included.
pub fn output_logical_size(info: &MonitorInfo) -> Option<(i32, i32)> {
    if let Some(size) = info.logical_size.filter(|(w, h)| *w > 0 && *h > 0) {
        return Some(size);
    }
    let mode = info.modes.iter().find(|m| m.current)?;
    let scale = info.scale_factor.max(1);
    Some(swap_for_transform((mode.dimensions.0 / scale, mode.dimensions.1 / scale), info.transform))
}
/// (w, h) as seen after applying a wl_output transform: swapped for 90/270, flipped or not.
pub fn swap_for_transform(size: (i32, i32), transform: wl_output::Transform) -> (i32, i32) {
    use wl_output::Transform as T;
    match transform {
        T::_90 | T::_270 | T::Flipped90 | T::Flipped270 => (size.1, size.0),
        _ => size,
    }
}
pub use sctk::seat::keyboard::Keysym as KeySym;
pub use sctk::seat::pointer::{AxisScroll, PointerEventKind as PointerKind};
pub use wayland_client::protocol::wl_output::WlOutput;

// Carry the registry name into SeatHandler::new_seat for the v9 pointer binding.
struct ExtendedSeats;
impl sctk::registry::RegistryHandler<Engine> for ExtendedSeats {
    fn new_global(
        state: &mut Engine,
        conn: &Connection,
        qh: &QueueHandle<Engine>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        if interface == "wl_seat" {
            state.pending_seat_global = Some(name);
        }
        <SeatState as sctk::registry::RegistryHandler<Engine>>::new_global(
            state, conn, qh, name, interface, version,
        );
        state.pending_seat_global = None;
    }
    fn remove_global(
        state: &mut Engine,
        conn: &Connection,
        qh: &QueueHandle<Engine>,
        name: u32,
        interface: &str,
    ) {
        <SeatState as sctk::registry::RegistryHandler<Engine>>::remove_global(
            state, conn, qh, name, interface,
        );
    }
}
impl Dispatch<wl_pointer::WlPointer, sctk::seat::pointer::PointerData> for Engine {
    fn event(
        state: &mut Self,
        proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        data: &sctk::seat::pointer::PointerData,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use crate::scroll::ScrollSource as S;
        use wayland_client::WEnum::Value;
        let Some(index) = state
            .input
            .iter()
            .position(|s| s.pointer.as_ref() == Some(proxy))
        else {
            return;
        };
        let scroll = &mut state.input[index].scroll;
        let axis_index = |axis| match axis {
            Value(wl_pointer::Axis::HorizontalScroll) => Some(0),
            Value(wl_pointer::Axis::VerticalScroll) => Some(1),
            _ => None,
        };
        let axis = match event {
            wl_pointer::Event::Axis { axis, value, .. } => {
                if let Some(i) = axis_index(axis) {
                    scroll.delta[i] += value;
                }
                true
            }
            wl_pointer::Event::AxisDiscrete { axis, discrete } => {
                if let Some(i) = axis_index(axis) {
                    scroll.delta120[i] += discrete * 120;
                }
                true
            }
            wl_pointer::Event::AxisValue120 { axis, value120 } => {
                if let Some(i) = axis_index(axis) {
                    scroll.delta120[i] += value120;
                }
                true
            }
            wl_pointer::Event::AxisStop { axis, .. } => {
                if axis_index(axis).is_some() {
                    scroll.stop = true;
                }
                true
            }
            wl_pointer::Event::AxisSource { axis_source } => {
                scroll.source = match axis_source {
                    Value(wl_pointer::AxisSource::Finger) => S::Finger,
                    Value(wl_pointer::AxisSource::Continuous) => S::Continuous,
                    Value(wl_pointer::AxisSource::WheelTilt) => S::WheelTilt,
                    _ => S::Wheel,
                };
                true
            }
            wl_pointer::Event::AxisRelativeDirection { axis, direction } => {
                if let Some(i) = axis_index(axis) {
                    scroll.inverted[i] =
                        direction == Value(wl_pointer::AxisRelativeDirection::Inverted);
                }
                true
            }
            _ => false,
        };
        let frame = matches!(event, wl_pointer::Event::Frame) || (axis && proxy.version() < 5);
        if !axis {
            <SeatState as Dispatch<
                wl_pointer::WlPointer,
                sctk::seat::pointer::PointerData,
                Engine,
            >>::event(state, proxy, event, data, conn, qh);
        }
        if frame {
            let seat = &mut state.input[index];
            let events = seat.scroll.finish();
            let focus = seat.pointer_focus;
            let target = seat.scroll_target.or(focus.map(|f| f.0));
            if events
                .iter()
                .any(|e| e.phase == crate::scroll::ScrollPhase::Begin)
            {
                seat.scroll_target = target;
            }
            if events
                .iter()
                .any(|e| e.phase == crate::scroll::ScrollPhase::End)
            {
                seat.scroll_target = None;
            }
            if let Some(id) = target {
                let position = focus.map(|f| f.1).unwrap_or((0., 0.));
                for event in events {
                    state.emit(id, InputEvent::Scroll { position, event });
                }
            }
            state.schedule();
        }
    }
}

pub use sctk::seat::pointer::PointerEvent as NativePointerEvent;

