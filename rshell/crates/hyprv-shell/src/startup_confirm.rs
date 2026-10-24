//! `hyprv-shell startup-confirm`: the one-shot startup confirmation dialog, the native
//! counterpart of `qs --path quickshell/StartupConfirm.qml`, which
//! `hypr/scripts/restore-session-layout.sh` runs at login.
//!
//! Same contract as the QML: read `HYPRV_STARTUP_APPROVAL`, write `launch` there only on
//! an explicit Launch (`hyprv_svc_desktop::startup::decide`), exit 0 once decided. Exit
//! non-zero (no approval) if the dialog is closed without a decision.
//!
//! It binds no IPC socket, takes no instance lock and starts no services, so it runs
//! next to the main shell.
use hyprv_svc_desktop::startup::{decide, StartupState};
use hyprv_ui::{scene::Scene, wayland::*};
use hyprv_ui_panels as panels;
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

struct Dialog {
    panel: panels::startup::StartupConfirm,
    approval: Option<PathBuf>,
    requested: bool,
    surface: Option<SurfaceId>,
    scale: f32,
    pointer: (f32, f32),
    decided: Option<bool>,
}
impl Dialog {
    fn handle(&mut self, actions: Vec<panels::Action>) {
        for action in actions {
            if let panels::Action::Host(panels::HostAction::StartupDecision(launch)) = action {
                if self.decided.is_none() {
                    let mut state = StartupState {
                        approval_path: self.approval.clone(),
                        ..Default::default()
                    };
                    if let Err(e) = decide(&mut state, launch) {
                        eprintln!("startup-confirm: could not record the decision: {e:?}");
                    }
                    self.decided = Some(launch && state.approved);
                }
            }
        }
    }
}
struct Handler(Rc<RefCell<Dialog>>);
impl UiHandler for Handler {
    fn clock(&mut self, now: Duration) {
        self.0.borrow_mut().panel.tick(now);
    }
    fn next_deadline(&self) -> Option<Duration> {
        self.0.borrow().panel.next_deadline()
    }
    fn output_added(
        &mut self,
        output: &WlOutput,
        info: Option<MonitorInfo>,
    ) -> Vec<(LayerConfig, Scene)> {
        let mut d = self.0.borrow_mut();
        if d.requested {
            return Vec::new(); // one dialog, on the first output (QML: default screen)
        }
        d.requested = true;
        d.scale = info.map_or(1., |i| i.scale_factor.max(1) as f32);
        let spec = d.panel.surface();
        let config = LayerConfig {
            namespace: spec.namespace.into(),
            output: Some(output.clone()),
            size: spec
                .size
                .map_or((0, 0), |(w, h)| (w.ceil() as u32, h.ceil() as u32)),
            layer: Layer::Overlay,
            anchor: Anchor::empty(), // centred
            exclusive_zone: spec.exclusive_zone,
            keyboard: KeyboardInteractivity::Exclusive,
            ..Default::default()
        };
        let scale = d.scale;
        vec![(config, Scene::new(d.panel.build(scale)))]
    }
    fn update(&mut self, engine: &mut Engine) {
        let mut d = self.0.borrow_mut();
        if d.surface.is_none() {
            d.surface = engine.surfaces().first().map(|(id, _)| *id);
        }
        if let Some(id) = d.surface {
            if let Some((_, scale)) = engine.surface_geometry(id) {
                if (scale as f32 - d.scale).abs() > 1e-3 {
                    d.scale = scale as f32;
                    let dpr = d.scale;
                    let scene = Scene::new(d.panel.build(dpr));
                    engine.replace_scene(id, scene);
                }
            }
        }
        if d.decided.is_some() {
            engine.quit();
        }
    }
    fn input(&mut self, _: SurfaceId, scene: &mut Scene, event: InputEvent) -> Response {
        let mut d = self.0.borrow_mut();
        let mut cursor = None;
        if let Some(ev) = crate::mapping::pointer(&event, &mut d.pointer) {
            let position = d.pointer;
            if let Some(ev) = crate::mapping::panel_pointer(ev, position) {
                let reply = d.panel.pointer(ev);
                cursor = reply.cursor.map(|c| match c {
                    panels::ui::Cursor::Pointer => CursorShape::Pointer,
                    _ => CursorShape::Default,
                });
                d.handle(reply.actions);
            }
        }
        if let InputEvent::Key { event, pressed, .. } = &event {
            if let Some(key) = crate::mapping::key(event.keysym, event.utf8.clone()) {
                let reply = d.panel.key(&panels::ui::KeyEvent {
                    key,
                    pressed: *pressed,
                    autorepeat: false,
                });
                d.handle(reply.actions);
            }
        }
        let scale = d.scale;
        d.panel.update(scene, scale);
        Response {
            cursor,
            ..Default::default()
        }
    }
}

/// True once a decision was made (approval written only for Launch); the CLI exits 1 otherwise.
pub fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let dialog = Rc::new(RefCell::new(Dialog {
        panel: panels::startup::StartupConfirm::new(false, Duration::ZERO),
        approval: std::env::var_os("HYPRV_STARTUP_APPROVAL").map(PathBuf::from),
        requested: false,
        surface: None,
        scale: 1.,
        pointer: (0., 0.),
        decided: None,
    }));
    Application::connect(Handler(dialog.clone()))?.run()?;
    let decided = dialog.borrow().decided.is_some();
    Ok(decided)
}
