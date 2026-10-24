//! One façade over every panel for the shell host: surfaces, build/update, input,
//! timers and the IPC entry points (`controlPanel toggle`, `quickAdjust show*`).
use crate::{
    calendar::CalendarPanel,
    control_panel::ControlPanel,
    model::*,
    quick_adjust::{Mode, QuickAdjust},
    stats::StatsPopup,
    ui::{Key, KeyEvent, Pointer, Reply, SurfaceSpec},
    wifi::WifiPanel,
};
use hyprv_ui::scene::{Node, Scene};
use std::time::Duration;

pub struct Panels {
    pub control: ControlPanel,
    pub wifi: WifiPanel,
    pub quick: QuickAdjust,
    pub stats: StatsPopup,
    pub calendar: CalendarPanel,
}

impl Default for Panels {
    fn default() -> Self {
        Self { control: ControlPanel::new(), wifi: WifiPanel::new(), quick: QuickAdjust::new(), stats: StatsPopup::new(), calendar: CalendarPanel::new(520., 500.) }
    }
}

impl Panels {
    pub fn new() -> Self {
        Self::default()
    }
    /// Panels for an explicit runtime profile (`new` uses the latched process profile).
    pub fn with_profile(laptop: bool) -> Self {
        Self { control: ControlPanel::with_profile(laptop), quick: QuickAdjust::with_profile(laptop), ..Self::default() }
    }
    /// Surfaces that must currently exist (mapped popups).
    pub fn surfaces(&self, model: &Model) -> Vec<(PanelKind, SurfaceSpec)> {
        let mut v = Vec::new();
        if let Some(s) = self.control.surface(model) {
            v.push((PanelKind::ControlPanel, s));
        }
        if let Some(s) = self.wifi.surface(model) {
            v.push((PanelKind::Wifi, s));
        }
        if let Some(s) = self.quick.surface(model) {
            v.push((PanelKind::QuickAdjust, s));
        }
        if let Some(s) = self.stats.surface(model) {
            v.push((PanelKind::Stats, s));
        }
        v
    }
    /// Full-output surface tree for a mapped popup.
    pub fn build(&mut self, kind: PanelKind, model: &Model) -> Node {
        // Qt GL rectangle/image/effect materials, as the bar's roots (hyprv-ui native_gl)
        let mut root = self.build_tree(kind, model);
        root.native_materials = Some(true);
        root
    }
    fn build_tree(&mut self, kind: PanelKind, model: &Model) -> Node {
        match kind {
            PanelKind::ControlPanel => self.control.build(model),
            PanelKind::Wifi => self.wifi.build(model),
            PanelKind::QuickAdjust => self.quick.build(model),
            PanelKind::Stats => self.stats.build(model),
            PanelKind::Startup => Node::new(Default::default(), hyprv_ui::scene::Content::Group),
        }
    }
    /// Edit only the changed nodes of a scene built by [`Panels::build`].
    pub fn update(&mut self, kind: PanelKind, scene: &mut Scene, model: &Model) -> bool {
        match kind {
            PanelKind::ControlPanel => self.control.update(scene, model),
            PanelKind::Wifi => self.wifi.update(scene, model),
            PanelKind::QuickAdjust => self.quick.update(scene, model),
            PanelKind::Stats => self.stats.update(scene, model),
            PanelKind::Startup => false,
        }
    }
    pub fn pointer(&mut self, kind: PanelKind, ev: Pointer, model: &Model) -> Reply<Action> {
        match kind {
            PanelKind::ControlPanel => self.control.pointer(ev, model),
            PanelKind::Wifi => self.wifi.pointer(ev, model),
            PanelKind::QuickAdjust => self.quick.pointer(ev, model),
            PanelKind::Stats => self.stats.pointer(ev, model),
            PanelKind::Startup => Reply::default(),
        }
    }
    pub fn key(&mut self, kind: PanelKind, ev: &KeyEvent, model: &Model) -> Reply<Action> {
        match kind {
            PanelKind::ControlPanel => self.control.key(ev, model),
            PanelKind::Wifi => self.wifi.key(ev, model),
            PanelKind::Stats => self.stats.key(ev, model),
            _ => Reply::default(),
        }
    }
    /// Advance timers/animations at `model.now`; returns host actions (unmapped…).
    pub fn tick(&mut self, model: &Model) -> Vec<Action> {
        if self.control.needs_measure() {
            self.control.measure(model);
        }
        if self.wifi.needs_measure() {
            self.wifi.build(model);
        }
        if self.stats.needs_measure() {
            self.stats.build(model);
        }
        let mut out = self.control.tick(model.now);
        if self.control.take_remeasure() {
            self.control.measure(model);
        }
        out.extend(self.wifi.tick(model));
        out.extend(self.quick.tick(model.now));
        out.extend(self.stats.tick(model.now));
        out.extend(self.calendar.tick(model));
        out
    }
    /// True while any animation runs (request frame callbacks).
    pub fn animating(&self, model: &Model) -> bool {
        self.control.animating(model.now, model) || self.wifi.animating(model.now) || self.quick.animating(model.now) || self.stats.animating() || self.calendar.animating(model.now)
    }
    /// The nearest timer deadline (schedule one wakeup; no polling).
    pub fn next_deadline(&self, now: Duration) -> Option<Duration> {
        [self.control.next_deadline(), self.wifi.next_deadline(now), self.quick.next_deadline(), self.stats.next_deadline(), self.calendar.next_deadline()].into_iter().flatten().min()
    }

    // IPC (ui-panels.md §4.1)
    /// `controlPanel toggle` → `toggleCentered(primaryBarWindow)`.
    pub fn ipc_control_panel_toggle(&mut self, primary_output: &str, model: &Model) -> Vec<Action> {
        self.control.toggle_centered(primary_output, model.now)
    }
    /// `quickAdjust showBrightness|showBrightnessLevel|showBrightnessIncrease|Decrease|showVolume`
    pub fn ipc_quick_adjust(&mut self, function: &str, model: &Model) -> Vec<Action> {
        let mode = if function == "showVolume" { Mode::Volume } else { Mode::Brightness };
        self.quick.show(mode, model.now)
    }
    /// Esc for whichever popup has keyboard focus.
    pub fn escape(&mut self, kind: PanelKind, model: &Model) -> Reply<Action> {
        self.key(kind, &KeyEvent { key: Key::Escape, pressed: true, autorepeat: false }, model)
    }
}
