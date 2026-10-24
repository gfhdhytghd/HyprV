//! ControlPanelPopup (ui-panels-control.md): surface, state machine with animated page
//! switches, main page, Bluetooth page, Mihomo page (unreachable from the QML UI; open it
//! with [`ControlPanel::show_page`]).
use crate::{
    model::*,
    popup::{self, GlassIds, GlassStyle, Machine, PopupEvent},
    qtext::{Align, Font, TextBox, BOLD, NORMAL},
    ui::*,
    widgets::{self, Chip, MediaButton, Slider, Split, Toggle, CHIP_HEIGHT},
};
use hyprv_ui::{
    animation::{Color, Direction, Family},
    geometry::Rect,
    scene::{Node, NodeId, Scene},
};
use hyprv_widgets::{containers::RevealTiming, flickable::{Flickable, Settings, Wheel}};
use std::time::Duration;

/// The power-profile toggle is never shown active (`ControlPanelPopup.qml:665-673`).
/// QUnifiedTimer: an animation started while the timer is ticking starts on the next
/// tick (one 60 Hz frame later).
pub const TICK_START_DELAY_S: f64 = 1. / 60.;
pub const QUIRK_PROFILE_TOGGLE_NEVER_ACTIVE: bool = true;
/// The Viewflow toggle is never shown active (`ViewflowTargetControl.qml`).
pub const QUIRK_VIEWFLOW_NEVER_ACTIVE: bool = true;
/// Slider labels ("Chassis 1 Limit · N RPM") are set but never rendered.
pub const QUIRK_SLIDER_LABELS_HIDDEN: bool = true;
/// Optimistic flips are dropped when the service publishes a new value or after this long
/// (the QML status poll overwrote them every 1500 ms).
pub const OPTIMISTIC_HOLD_MS: u64 = 1900;
/// Laptop `brightnessUiStepPercent` (laptop-panels.md §2.1); desktop is 2.
pub const LAPTOP_BRIGHTNESS_STEP: i32 = 1;

/// `Profile::Laptop` from the process-wide runtime profile (laptop-panels.md).
pub fn laptop_profile() -> bool {
    hyprv_core::profile::profile() == hyprv_core::profile::Profile::Laptop
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Bluetooth,
    Mihomo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Z {
    Card,
    WifiIcon,
    WifiText,
    BtIcon,
    BtText,
    Vm,
    Viewflow,
    Record,
    Sleep,
    Media(MediaButton),
    Theme,
    Dnd,
    Demo,
    Profile,
    /// Laptop split tiles' text halves (laptop-panels.md §1.1).
    ProfileText,
    DemoText,
    Brightness,
    Volume,
    Chassis,
    Pump,
    BtPower,
    BtClose,
    BtScan,
    BtUnnamed,
    BtAdvanced,
    BtList,
    BtDevice(usize),
    MhPower,
    MhMode(usize),
    MhRefresh,
    MhDashboard,
    MhBack,
}

const POPUP_SCREEN_MARGIN: f32 = 10.;
const MODULE: f32 = 62.;
const SPACING: f32 = 10.;
const COLUMN_W: f32 = MODULE * 2. + SPACING;

fn timing(page: Page) -> RevealTiming {
    if page == Page::Bluetooth {
        RevealTiming { open_pause: 85, open_reveal: 280, content_delay: 60, open_fade: 180, open_slide: 220, open_offset: -10., close_pause: 35, close_reveal: 210, close_fade: 110, close_slide: 170, close_offset: -8. }
    } else {
        RevealTiming { open_pause: 20, open_reveal: 200, content_delay: 20, open_fade: 140, open_slide: 180, open_offset: -8., close_pause: 30, close_reveal: 180, close_fade: 90, close_slide: 150, close_offset: -8. }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SliderState {
    adjusting: bool,
    preview: f32,
}

#[derive(Clone, Debug, Default)]
struct Overrides {
    // (value, set at, model value when set)
    wifi_radio: Option<(bool, Duration, bool)>,
    dark: Option<(bool, Duration, bool)>,
    dnd: Option<(bool, Duration, bool)>,
    recording: Option<(bool, Duration, bool)>,
    sleep: Option<(bool, Duration, bool)>,
    demo: Option<(bool, Duration, bool)>,
    profile: Option<(Profile, Duration, Profile)>,
    playing: Option<(bool, Duration, bool)>,
    volume: Option<(i32, Duration, i32)>,
    muted: Option<(bool, Duration, bool)>,
    brightness: Option<(i32, Duration, i32)>,
    chassis: Option<(i32, Duration, i32)>,
    pump: Option<(i32, Duration, i32)>,
}

fn apply<T: Clone + PartialEq>(slot: &mut Option<(T, Duration, T)>, model: &mut T, now: Duration) {
    if let Some((v, at, seen)) = slot.clone() {
        if *model != seen || now.saturating_sub(at) > ms(OPTIMISTIC_HOLD_MS) {
            *slot = None;
        } else {
            *model = v;
        }
    }
}

/// `snapBrightnessPercent`: clamp, round to the 2 % UI step.
pub fn snap_brightness(v: f32) -> i32 {
    let c = v.round().clamp(0., 100.);
    ((c / 2.).round() * 2.).clamp(0., 100.) as i32
}

/// `snapBrightnessPercent` for the profile: the laptop snaps to whole percent.
pub fn snap_brightness_for(laptop: bool, v: f32) -> i32 {
    if laptop {
        (v.round().clamp(0., 100.) / LAPTOP_BRIGHTNESS_STEP as f32).round() as i32 * LAPTOP_BRIGHTNESS_STEP
    } else {
        snap_brightness(v)
    }
}

/// `shellRoot.volumeIcon` (shell.qml:369-384).
pub fn volume_icon(a: &Audio) -> &'static str {
    if !a.available {
        "\u{F026}"
    } else if a.muted {
        "\u{F466}"
    } else if a.volume_percent <= 20 {
        "\u{F026}"
    } else if a.volume_percent <= 50 {
        "\u{F027}"
    } else {
        "\u{F028}"
    }
}

struct Ids {
    root: NodeId,
    glass: GlassIds,
    key: String,
}

pub struct ControlPanel {
    /// Laptop profile layout and behaviour (laptop-panels.md §1). Latched from
    /// [`laptop_profile`] at construction; tests and fixture replay may set it.
    pub laptop: bool,
    /// The host draws the glass chrome and owns map/animation: page switches become
    /// requests ([`ControlPanel::take_page_request`]) instead of local animations.
    pub host_owned: bool,
    pub m: Machine,
    pub page: Page,
    pending_page: Option<Page>,
    source: Option<Anchor>,
    output: String,
    pub inter: Interaction<Z>,
    zones: Vec<Zone<Z>>,
    sliders: [SliderState; 4],
    ov: Overrides,
    bt_scroll: f32,
    bt_flick: Flickable,
    sw_fill: Anim<Color>,
    sw_stroke: Anim<Color>,
    sw_knob: Anim<f64>,
    sw_init: bool,
    /// When the Mihomo page Loader item was created (its border pulse starts then).
    pulse_start: Option<Duration>,
    /// The page changed inside `tick`: [`ControlPanel::measure`] before the open timer fires.
    remeasure: bool,
    ids: Option<Ids>,
    last_card: Rect,
}

impl Default for ControlPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlPanel {
    pub fn new() -> Self {
        Self::with_profile(laptop_profile())
    }
    pub fn with_profile(laptop: bool) -> Self {
        Self {
            laptop,
            host_owned: false,
            m: Machine::new(timing(Page::Main)),
            page: Page::Main,
            pending_page: None,
            source: None,
            output: String::new(),
            inter: Interaction::default(),
            zones: Vec::new(),
            sliders: [SliderState::default(); 4],
            ov: Overrides::default(),
            bt_scroll: 0.,
            bt_flick: Flickable::new(Settings::list_view()),
            sw_fill: Anim::new(Color::default(), 140, hyprv_ui::animation::Easing::Linear),
            sw_stroke: Anim::new(Color::default(), 140, hyprv_ui::animation::Easing::Linear),
            sw_knob: Anim::new(3., 160, ease(Family::Quad, Direction::InOut)),
            sw_init: false,
            pulse_start: None,
            remeasure: false,
            ids: None,
            last_card: Rect::default(),
        }
    }

    /// Mapped but the content height is not known yet (no build since mapping); the
    /// QML binding has it immediately, so [`crate::Panels::tick`] measures first.
    pub fn needs_measure(&self) -> bool {
        self.m.mapped && self.m.reveal.full_height <= 0.
    }
    /// Recompute the content height (`fullPanelHeight`) without rebuilding the scene.
    /// QML has it as soon as `currentPage` changes; call after `tick` switched pages.
    pub fn measure(&mut self, model: &Model) {
        self.remeasure = false;
        let e = self.effective(model);
        self.sync_switch(&e);
        let (_, h, _) = self.layout(model, &e);
        self.m.set_full_height(h);
    }
    pub fn take_remeasure(&mut self) -> bool {
        std::mem::take(&mut self.remeasure)
    }
    pub fn page(&self) -> Page {
        self.page
    }
    pub fn is_mapped(&self) -> bool {
        self.m.mapped
    }
    pub fn popup_requested(&self) -> bool {
        self.m.requested
    }

    fn reset_expanded(&mut self) {
        self.page = Page::Main;
        self.pending_page = None;
        self.bt_scroll = 0.;
        self.bt_flick.reset(0.);
    }
    fn set_timing(&mut self) {
        self.m.reveal.timing = timing(self.page);
    }

    /// `openFor(source, window)` from the bar icon.
    pub fn open_for(&mut self, anchor: Anchor, output: &str, now: Duration) -> Vec<Action> {
        self.source = Some(anchor);
        self.output = output.to_string();
        self.reset_expanded();
        self.set_timing();
        let map = self.m.open(now);
        self.on_open(map)
    }
    fn on_open(&mut self, newly_mapped: bool) -> Vec<Action> {
        let mut a = vec![Action::Host(HostAction::RefreshPanelData)];
        if newly_mapped {
            a.insert(0, Action::Host(HostAction::Mapped(PanelKind::ControlPanel)));
        }
        a
    }
    /// `toggleFor(source, window)`
    pub fn toggle_for(&mut self, anchor: Anchor, output: &str, now: Duration) -> Vec<Action> {
        if self.m.mapped && self.source == Some(anchor) && self.output == output {
            self.close(now);
            vec![]
        } else {
            self.open_for(anchor, output, now)
        }
    }
    /// `toggleCentered(window)` (IPC `controlPanel toggle`): no source item.
    pub fn toggle_centered(&mut self, output: &str, now: Duration) -> Vec<Action> {
        if self.m.mapped {
            self.close(now);
            return vec![];
        }
        self.source = None;
        self.output = output.to_string();
        self.reset_expanded();
        self.set_timing();
        self.m.requested = true;
        self.m.closing = false;
        self.m.map(now);
        self.on_open(true)
    }
    pub fn close(&mut self, now: Duration) {
        self.pending_page = None;
        self.set_timing();
        self.m.close(now);
    }
    fn unmap_now(&mut self) -> Vec<Action> {
        self.m.unmap_now();
        self.after_unmap()
    }
    fn after_unmap(&mut self) -> Vec<Action> {
        self.reset_expanded();
        self.source = None;
        self.inter = Interaction::default();
        self.sliders = [SliderState::default(); 4];
        self.set_timing();
        vec![Action::Host(HostAction::Unmapped(PanelKind::ControlPanel))]
    }

    /// `switchPageWithAnimation(target)`
    pub fn show_page(&mut self, target: Page, now: Duration) {
        if target == self.page {
            return;
        }
        if self.host_owned {
            self.pending_page = Some(target);
            return;
        }
        if !self.m.mapped || self.m.closing {
            self.page = target;
            self.pulse_start = Some(now);
            self.set_timing();
            return;
        }
        self.pending_page = Some(target);
        self.set_timing(); // old page's close timing
        self.m.reveal.play_close(now);
    }

    /// Advance timers/animations. Returns host actions (unmapped, …).
    pub fn tick(&mut self, now: Duration) -> Vec<Action> {
        if self.m.mapped && self.page == Page::Bluetooth {
            self.bt_flick.frame(now);
            self.bt_scroll = self.bt_flick.content_y() as f32;
        }
        let mut out = Vec::new();
        for ev in self.m.tick(now) {
            if ev == PopupEvent::CloseFinished {
                if let Some(p) = self.pending_page.take() {
                    if self.m.requested && self.m.mapped && !self.m.closing {
                        // animateCurrentPageOpen
                        self.page = p;
                        self.bt_scroll = 0.;
                        self.bt_flick.reset(0.);
                        // The Loader item (and its pulse) is created from
                        // onCloseAnimationFinished, i.e. inside an animation tick: Qt starts
                        // animations registered during a tick on the next one.
                        self.pulse_start = Some(now + Duration::from_secs_f64(TICK_START_DELAY_S));
                        self.set_timing();
                        self.m.reopen_after_page_switch(now);
                        self.remeasure = true;
                        continue;
                    }
                }
                if self.m.finish_close() {
                    out.extend(self.after_unmap());
                }
            }
        }
        out
    }

    pub fn animating(&self, now: Duration, model: &Model) -> bool {
        self.m.mapped
            && (self.m.animating()
                || (self.page == Page::Bluetooth && self.bt_flick.animating())
                || self.sw_fill.running(now)
                || self.sw_stroke.running(now)
                || self.sw_knob.running(now)
                || (self.page == Page::Mihomo && model.controls.mihomo.online && (model.controls.mihomo.download > 1024 || model.controls.mihomo.upload > 1024)))
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.m.next_deadline()
    }

    pub fn surface(&self, model: &Model) -> Option<SurfaceSpec> {
        self.m.mapped.then(|| SurfaceSpec {
            namespace: "shell:hyprv-control-panel",
            layer: Layer::Overlay,
            anchors: [true; 4],
            exclusive_zone: -1,
            keyboard: KeyboardMode::OnDemand,
            size: None,
            input_region: None,
            output: if self.output.is_empty() { model.output.name.clone() } else { self.output.clone() },
        })
    }

    /// The model with optimistic overrides applied.
    pub fn effective(&mut self, model: &Model) -> Model {
        let mut e = model.clone();
        let now = model.now;
        apply(&mut self.ov.wifi_radio, &mut e.network.wifi_radio_enabled, now);
        apply(&mut self.ov.dark, &mut e.theme.dark, now);
        apply(&mut self.ov.dnd, &mut e.controls.dnd, now);
        apply(&mut self.ov.recording, &mut e.controls.screen_recording, now);
        apply(&mut self.ov.sleep, &mut e.controls.prevent_sleep, now);
        apply(&mut self.ov.demo, &mut e.controls.demo_mode, now);
        apply(&mut self.ov.profile, &mut e.controls.profile, now);
        apply(&mut self.ov.playing, &mut e.media.playing, now);
        apply(&mut self.ov.volume, &mut e.audio.volume_percent, now);
        apply(&mut self.ov.muted, &mut e.audio.muted, now);
        apply(&mut self.ov.brightness, &mut e.controls.brightness_percent, now);
        apply(&mut self.ov.chassis, &mut e.controls.chassis_fan_percent, now);
        apply(&mut self.ov.pump, &mut e.controls.pump_fan_percent, now);
        if self.ov.volume.is_some() || self.ov.muted.is_some() {
            e.audio.available = true;
        }
        e
    }

    fn popup_width(&self) -> f32 {
        if self.page == Page::Main {
            COLUMN_W * 2. + SPACING
        } else {
            384.
        }
    }

    fn card_rect(&self, model: &Model, full_h: f32) -> Rect {
        let w = self.popup_width();
        let (ow, oh) = (model.output.width, model.output.height);
        if self.laptop {
            // laptop-panels.md §1.3: IPC opens centre the card; anchored opens keep the
            // right alignment. Both clamp y inside the output.
            let m = POPUP_SCREEN_MARGIN;
            let (x, target_y) = match self.source {
                Some(a) => (m.max(ow - w - m), a.bottom().round() + 10.),
                None => (m.max(((ow - w) / 2.).round()), ((oh - full_h) / 2.).round()),
            };
            return Rect::new(x, m.max(target_y.min(oh - full_h - m)), w, full_h);
        }
        let bar_bottom = match self.source {
            Some(a) => a.bottom().round(),
            None => model.shell.bar_exclusive_zone.max(0.),
        };
        let x = POPUP_SCREEN_MARGIN.max(ow - w - POPUP_SCREEN_MARGIN);
        Rect::new(x, bar_bottom + 10., w, full_h)
    }

    /// Page content laid out at its final card position. The card position depends on
    /// the content height on the laptop, so a first pass at the last known height is
    /// redone when the height moved the card (never on the desktop).
    fn layout(&mut self, model: &Model, e: &Model) -> (Vec<Node>, f32, Rect) {
        let card0 = self.card_rect(model, self.m.reveal.full_height.max(0.));
        let (mut nodes, mut h) = self.page_nodes(e, (card0.x, card0.y));
        let mut card = self.card_rect(model, h);
        if (card.x, card.y) != (card0.x, card0.y) {
            (nodes, h) = self.page_nodes(e, (card.x, card.y));
            card = self.card_rect(model, h);
        }
        (nodes, h, card)
    }

    // ------------------------------------------------------------------ building

    fn style(&self, pal: &Palette) -> GlassStyle {
        if self.page == Page::Main {
            GlassStyle { radius: 19., fill: Color::default(), stroke: Color::default(), surface_opacity: 0., shadow: Color::default() }
        } else {
            GlassStyle { radius: 19., fill: pal.glass_fill, stroke: pal.glass_stroke, surface_opacity: 0.82, shadow: Color::default() }
        }
    }

    /// Page content (card-local) and its height.
    fn page_nodes(&mut self, e: &Model, card_abs: (f32, f32)) -> (Vec<Node>, f32) {
        self.sync_bt_scroll(e);
        let pal = Palette::new(e.theme.dark);
        let mut zones = Vec::new();
        let dpr = e.output.scale;
        let inter = self.inter.clone();
        let sliders = self.sliders;
        if self.page == Page::Mihomo && self.pulse_start.is_none() {
            self.pulse_start = Some(e.now);
        }
        let (nodes, h) = {
            let mut c = Ctx { dpr, pal: &pal, zones: &mut zones };
            match self.page {
                Page::Main => main_page(&mut c, e, card_abs, &inter, &sliders, self.laptop),
                Page::Bluetooth => {
                    let knob = self.sw_knob.get(e.now) as f32;
                    let fill = self.sw_fill.get(e.now);
                    let stroke = self.sw_stroke.get(e.now);
                    bluetooth_page(&mut c, e, card_abs, &inter, self.bt_scroll, (fill, stroke, knob))
                }
                Page::Mihomo => mihomo_page(&mut c, e, card_abs, &inter, e.now.saturating_sub(self.pulse_start.unwrap_or(e.now))),
            }
        };
        self.zones = zones;
        (nodes, h)
    }

    fn sync_switch(&mut self, e: &Model) {
        let pal = Palette::new(e.theme.dark);
        let (fill, stroke) = switch_colors(&pal, e.bluetooth.enabled);
        let knob = if e.bluetooth.enabled { 48. - 22. - 3. } else { 3. };
        if !self.sw_init {
            self.sw_fill.snap(fill);
            self.sw_stroke.snap(stroke);
            self.sw_knob.snap(knob);
            self.sw_init = true;
        } else {
            self.sw_fill.set(fill, e.now);
            self.sw_stroke.set(stroke, e.now);
            self.sw_knob.set(knob, e.now);
        }
    }

    /// Content of the current page only (no glass chrome), for hosts that draw the
    /// AnimatedGlassPanel themselves. `card_abs` = the card's surface position (text
    /// snapping and input zones use it). Returns (content, card width, card height).
    pub fn content(&mut self, model: &Model, card_abs: (f32, f32)) -> (Node, f32, f32) {
        let mut r = self.content_raw(model, card_abs);
        crate::qrect::convert(&mut r.0, card_abs, model.output.scale);
        r
    }
    fn content_raw(&mut self, model: &Model, card_abs: (f32, f32)) -> (Node, f32, f32) {
        let e = self.effective(model);
        self.sync_switch(&e);
        let (nodes, h) = self.page_nodes(&e, card_abs);
        let card = Rect::new(card_abs.0, card_abs.1, self.popup_width(), h);
        self.last_card = card;
        let mut zones = vec![Zone { id: Z::Card, rect: card, cursor: Cursor::Default, enabled: true }];
        zones.append(&mut self.zones);
        self.zones = zones;
        (group_with(Rect::new(0., 0., card.width, h), nodes), card.width, h)
    }
    /// Page switch requested by the content (Bluetooth tile, Close/Back) while the host
    /// owns the glass animation: run the close animation with `timing(current)`, then
    /// call [`ControlPanel::set_page`] and open with `timing(new)`.
    pub fn take_page_request(&mut self) -> Option<Page> {
        if self.host_owned {
            self.pending_page.take()
        } else {
            None
        }
    }
    pub fn set_page(&mut self, page: Page) {
        self.page = page;
        self.pulse_start = None; // starts at the next build
        self.bt_scroll = 0.;
        self.bt_flick.reset(0.);
        self.set_timing();
    }
    /// Glass timing per page (ui-panels-control.md §1.2).
    pub fn timing(page: Page) -> RevealTiming {
        timing(page)
    }

    /// The card alone, fully revealed, at the origin (offscreen tests and parity scenes).
    pub fn page_preview(&mut self, model: &Model, page: Page) -> (Node, f32, f32) {
        let mut r = self.page_preview_raw(model, page);
        crate::qrect::convert(&mut r.0, (0., 0.), model.output.scale);
        r
    }
    fn page_preview_raw(&mut self, model: &Model, page: Page) -> (Node, f32, f32) {
        self.page = page;
        let e = self.effective(model);
        self.sync_switch(&e);
        let (nodes, h) = self.page_nodes(&e, (0., 0.));
        let pal = Palette::new(e.theme.dark);
        let w = self.popup_width();
        let (card, _) = popup::glass(Rect::new(0., 0., w, h), h, h, 1., 0., &self.style(&pal), nodes, model.output.scale);
        (card, w, h)
    }

    /// Full surface tree (output-sized transparent root with the card).
    pub fn build(&mut self, model: &Model) -> Node {
        let mut n = self.build_raw(model);
        crate::qrect::convert(&mut n, (0., 0.), model.output.scale);
        n
    }
    fn build_raw(&mut self, model: &Model) -> Node {
        let mut root = group(Rect::new(0., 0., model.output.width, model.output.height));
        let rid = root.id();
        let e = self.effective(model);
        self.sync_switch(&e);
        let (nodes, h, card) = self.layout(model, &e);
        self.m.set_full_height(h);
        self.last_card = card;
        let pal = Palette::new(e.theme.dark);
        let v = self.m.reveal.values();
        let mut zones = vec![Zone { id: Z::Card, rect: card, cursor: Cursor::Default, enabled: true }];
        zones.append(&mut self.zones);
        self.zones = zones;
        let (card_node, gids) = popup::glass(card, h, v.height, v.opacity, v.offset, &self.style(&pal), nodes, model.output.scale);
        root.children.push(card_node);
        let key = self.page_key(&e);
        self.ids = Some(Ids { root: rid, glass: gids, key });
        root
    }

    fn page_key(&self, e: &Model) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.page,
            e.theme,
            e.network,
            e.bluetooth,
            e.media,
            e.audio,
            e.controls,
            self.inter.hovered,
            self.inter.pressed,
            self.sliders.map(|s| (s.adjusting, s.preview.to_bits())),
            e.output,
            self.bt_scroll,
            (self.sw_fill.get(e.now), self.sw_knob.get(e.now)),
            e.bluetooth_show_unnamed,
            e.shell,
            self.source,
            if self.page == Page::Mihomo { e.now.as_millis() / 16 } else { 0 },
        )
    }

    /// Edit only what changed: the page subtree when its inputs changed, otherwise the
    /// reveal clip, content opacity and card position. Returns true if anything changed.
    pub fn update(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let changed = self.update_raw(scene, model);
        crate::qrect::finalize(scene, model.output.scale);
        changed
    }
    fn update_raw(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let Some(ids) = &self.ids else { return false };
        if scene.root().find(ids.root).is_none() {
            return false;
        }
        let e = self.effective(model);
        self.sync_switch(&e);
        let key = self.page_key(&e);
        let rev = scene.revision();
        let pal = Palette::new(e.theme.dark);
        let style = self.style(&pal);
        if key != self.ids.as_ref().unwrap().key {
            let (nodes, h, card) = self.layout(model, &e);
            self.m.set_full_height(h);
            self.last_card = card;
            let mut zones = vec![Zone { id: Z::Card, rect: card, cursor: Cursor::Default, enabled: true }];
            zones.append(&mut self.zones);
            self.zones = zones;
            let v = self.m.reveal.values();
            let (card_node, gids) = popup::glass(card, h, v.height, v.opacity, v.offset, &style, nodes, model.output.scale);
            let rid = self.ids.as_ref().unwrap().root;
            scene.edit(rid, |n| n.children = vec![card_node]);
            self.ids = Some(Ids { root: rid, glass: gids, key });
        } else {
            let card = self.card_rect(model, self.m.reveal.full_height);
            self.last_card = card;
            if let Some(z) = self.zones.first_mut() {
                z.rect = card;
            }
            let v = self.m.reveal.values();
            let g = self.ids.as_ref().unwrap().glass;
            popup::glass_update(scene, &g, (card.x, card.y), v.height, v.opacity);
        }
        scene.revision() != rev
    }

    // ------------------------------------------------------------------ input

    pub fn key(&mut self, ev: &KeyEvent, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if ev.pressed && ev.key == Key::Escape && self.m.mapped {
            self.close(model.now);
            r.redraw = true;
        }
        r
    }

    pub fn pointer(&mut self, ev: Pointer, model: &Model) -> Reply<Action> {
        let now = model.now;
        let mut r = Reply::default();
        if !self.m.mapped {
            return r;
        }
        let e = self.effective(model);
        self.sync_bt_scroll(&e);
        if let Pointer::Wheel { x, y, angle_y, pixel_y, .. } = ev {
            return self.bluetooth_wheel(x, y, Wheel::deltas(angle_y as f64, pixel_y as f64), model);
        }
        match ev {
            Pointer::Motion { y, .. } if self.bt_flick.drag_to(y as f64, None, now) => {
                self.bt_scroll = self.bt_flick.content_y() as f32;
                self.inter.pressed = None;
                self.inter.hovered = None;
                r.redraw = true;
                return r;
            }
            Pointer::Release { y, button: Button::Left, .. } if self.bt_flick.release(y as f64, now) => {
                self.inter.pressed = None;
                self.inter.hovered = None;
                r.redraw = true;
                return r;
            }
            // lost grab: Flickable::mouseUngrabEvent → cancelInteraction (fixup to bounds)
            Pointer::Leave | Pointer::Cancel => {
                self.bt_flick.cancel_interaction();
                self.bt_scroll = self.bt_flick.content_y() as f32;
            }
            _ => {}
        }
        let zones = self.zones.clone();
        match self.inter.feed(ev, &zones) {
            Hit::None => {}
            Hit::Hover => r.redraw = true,
            Hit::Pressed(id, button, x, y) => {
                r.redraw = true;
                if button == Button::Left {
                    if let Some(i) = slider_index(id) {
                        let zr = zones.iter().find(|z| z.id == id).unwrap().rect;
                        let s = Slider { icon: String::new(), value: 0., icon_left_inset: -1. };
                        if id == Z::Volume && widgets::slider_icon_hit(&s, zr.width, zr.height, x - zr.x, y - zr.y) {
                            r.actions.extend(self.toggle_mute(&e, now));
                            self.inter.pressed = None;
                        } else {
                            let v = widgets::slider_value(x - zr.x, zr.width);
                            self.sliders[i] = SliderState { adjusting: true, preview: v };
                            r.actions.extend(self.slider_change(id, v, &e, now));
                        }
                    }
                    if (id == Z::BtList || matches!(id, Z::BtDevice(_))) && self.bt_flick.maximum_y() > 0. {
                        self.bt_flick.press(y as f64, now);
                    }
                }
            }
            Hit::Dragged(id, x, _y) => {
                if let Some(i) = slider_index(id) {
                    if self.sliders[i].adjusting {
                        let zr = zones.iter().find(|z| z.id == id).unwrap().rect;
                        let v = widgets::slider_value(x - zr.x, zr.width);
                        self.sliders[i].preview = v;
                        r.actions.extend(self.slider_change(id, v, &e, now));
                        r.redraw = true;
                    }
                }
            }
            Hit::Released { id, button, x, y: _, inside } => {
                r.redraw = true;
                if let Some(i) = slider_index(id) {
                    if self.sliders[i].adjusting {
                        let zr = zones.iter().find(|z| z.id == id).unwrap().rect;
                        let v = widgets::slider_value(x - zr.x, zr.width);
                        self.sliders[i] = SliderState { adjusting: false, preview: v };
                        r.actions.extend(self.slider_change(id, v, &e, now));
                        r.actions.extend(self.slider_finish(id, v));
                    }
                } else if inside && button == Button::Left {
                    r.actions.extend(self.click(id, &e, now));
                }
            }
            // ControlPanelSlider onCanceled: adjusting = false; adjustmentFinished(preview)
            Hit::Canceled { id, .. } => {
                r.redraw = true;
                if let Some(i) = slider_index(id) {
                    if self.sliders[i].adjusting {
                        let v = self.sliders[i].preview;
                        self.sliders[i].adjusting = false;
                        r.actions.extend(self.slider_finish(id, v));
                    }
                }
            }
        }
        // click outside the card (any button) closes
        if let Pointer::Press { x, y, .. } = ev {
            if !self.last_card.contains(x, y) {
                self.close(now);
                r.redraw = true;
            }
        }
        r.cursor = Some(self.inter.cursor(&self.zones));
        r
    }

    /// Native wheel entry point: preserve the device's pixel deltas and scroll phase.
    /// `Pointer::Wheel` remains the compatibility adapter for hosts without phases.
    pub fn bluetooth_wheel(&mut self, x: f32, y: f32, wheel: Wheel, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if !self.m.mapped || self.page != Page::Bluetooth { return r; }
        let e = self.effective(model);
        self.sync_bt_scroll(&e);
        if self.zones.iter().any(|z| z.id == Z::BtList && z.rect.contains(x, y)) && self.bt_flick.maximum_y() > 0. {
            r.redraw = self.bt_flick.wheel(wheel, model.now);
            self.bt_scroll = self.bt_flick.content_y() as f32;
        }
        r
    }

    fn sync_bt_scroll(&mut self, e: &Model) {
        let (content, view) = bt_list_extent(e);
        self.bt_flick.set_extents(view as f64, content as f64);
        if self.page != Page::Bluetooth {
            self.bt_flick.reset(0.);
        } else if content <= view {
            self.bt_flick.cancel_interaction();
        }
        self.bt_scroll = self.bt_flick.content_y() as f32;
    }

    fn slider_change(&mut self, id: Z, v: f32, e: &Model, now: Duration) -> Vec<Action> {
        match id {
            Z::Brightness => {
                let s = snap_brightness_for(self.laptop, v);
                self.ov.brightness = Some((s, now, e.controls.brightness_percent));
                vec![Action::BrightnessSet(s)]
            }
            Z::Volume => {
                let s = v.round().clamp(0., 100.) as i32;
                self.ov.volume = Some((s, now, e.audio.volume_percent));
                vec![Action::SetVolume(s)]
            }
            Z::Chassis | Z::Pump if e.controls.fan_available && !self.laptop => {
                let s = v.round().clamp(0., 100.) as i32;
                let ch = if id == Z::Chassis { FanChannel::Chassis } else { FanChannel::Pump };
                if id == Z::Chassis {
                    self.ov.chassis = Some((s, now, e.controls.chassis_fan_percent));
                } else {
                    self.ov.pump = Some((s, now, e.controls.pump_fan_percent));
                }
                vec![Action::FanLimit { channel: ch, percent: s }]
            }
            _ => vec![],
        }
    }
    fn slider_finish(&mut self, id: Z, v: f32) -> Vec<Action> {
        match id {
            // The laptop ControlPanelSlider has no adjustmentFinished (laptop-panels.md §1.4).
            Z::Brightness if self.laptop => vec![],
            Z::Brightness => vec![Action::BrightnessFinish(snap_brightness(v))],
            _ => vec![],
        }
    }
    fn toggle_mute(&mut self, e: &Model, now: Duration) -> Vec<Action> {
        self.ov.muted = Some((!e.audio.muted, now, e.audio.muted));
        vec![Action::ToggleMute]
    }

    fn click(&mut self, id: Z, e: &Model, now: Duration) -> Vec<Action> {
        let c = &e.controls;
        match id {
            Z::WifiIcon => {
                let v = !e.network.wifi_radio_enabled;
                self.ov.wifi_radio = Some((v, now, e.network.wifi_radio_enabled));
                vec![Action::WifiSetRadio(v)]
            }
            Z::WifiText => {
                let mut a = self.unmap_now();
                a.push(Action::Host(HostAction::OpenWifiPanel));
                a
            }
            Z::BtIcon => vec![Action::BluetoothSetPower(!e.bluetooth.enabled)],
            Z::BtText => {
                if self.page == Page::Bluetooth {
                    self.show_page(Page::Main, now);
                } else {
                    self.show_page(Page::Bluetooth, now);
                }
                vec![]
            }
            Z::Vm | Z::Viewflow if self.laptop => vec![],
            Z::Vm => {
                if c.vm_busy {
                    return vec![];
                }
                // actionStarting → popup unmapped instantly so the auth dialog gets input
                let mut a = self.unmap_now();
                a.push(Action::VmToggle);
                a
            }
            Z::Viewflow => {
                if c.viewflow_busy {
                    return vec![];
                }
                vec![Action::ViewflowSet(if c.viewflow_target == Target::MacOs { Target::Windows } else { Target::MacOs })]
            }
            Z::Record => {
                self.ov.recording = Some((!c.screen_recording, now, c.screen_recording));
                vec![Action::ToggleRecording]
            }
            Z::Sleep => {
                self.ov.sleep = Some((!c.prevent_sleep, now, c.prevent_sleep));
                vec![Action::TogglePreventSleep]
            }
            Z::Theme => {
                self.ov.dark = Some((!e.theme.dark, now, e.theme.dark));
                vec![Action::ToggleTheme]
            }
            Z::Dnd => {
                self.ov.dnd = Some((!c.dnd, now, c.dnd));
                vec![Action::ToggleDnd]
            }
            Z::Demo | Z::DemoText => {
                self.ov.demo = Some((!c.demo_mode, now, c.demo_mode));
                vec![Action::ToggleDemo]
            }
            Z::Profile | Z::ProfileText => {
                let next = match c.profile {
                    Profile::PowerSaver => Profile::Balanced,
                    Profile::Balanced => Profile::Performance,
                    Profile::Performance => Profile::PowerSaver,
                };
                self.ov.profile = Some((next, now, c.profile));
                vec![Action::ProfileSet(next)]
            }
            Z::Media(b) => {
                if !e.media.available {
                    return vec![];
                }
                match b {
                    MediaButton::Previous => vec![Action::MediaPrevious],
                    MediaButton::Next => vec![Action::MediaNext],
                    MediaButton::PlayPause => {
                        self.ov.playing = Some((!e.media.playing, now, e.media.playing));
                        vec![Action::MediaPlayPause]
                    }
                }
            }
            Z::BtPower => vec![Action::BluetoothSetPower(!e.bluetooth.enabled)],
            Z::BtClose | Z::MhBack => {
                self.show_page(Page::Main, now);
                vec![]
            }
            Z::BtScan => vec![Action::BluetoothScan],
            Z::BtUnnamed => vec![Action::Host(HostAction::SetBluetoothShowUnnamed(!e.bluetooth_show_unnamed))],
            Z::BtAdvanced => vec![Action::Host(HostAction::OpenBluetoothManager)],
            Z::BtDevice(i) => {
                if e.bluetooth.busy {
                    return vec![];
                }
                let devs = visible_devices(e);
                let Some(d) = devs.get(i) else { return vec![] };
                if d.connected {
                    vec![Action::BluetoothDisconnect { address: d.address.clone() }]
                } else {
                    vec![Action::BluetoothConnect { address: d.address.clone(), paired: d.paired }]
                }
            }
            Z::MhPower => {
                let m = &c.mihomo;
                if m.service == "user" || m.service == "system" {
                    vec![if m.online { Action::MihomoStop } else { Action::MihomoStart }]
                } else {
                    vec![]
                }
            }
            Z::MhMode(i) => {
                if c.mihomo.online && !c.mihomo.busy {
                    vec![Action::MihomoMode(MIHOMO_MODES[i].to_string())]
                } else {
                    vec![]
                }
            }
            Z::MhRefresh => {
                if c.mihomo.busy {
                    vec![]
                } else {
                    vec![Action::MihomoRefresh]
                }
            }
            Z::MhDashboard => {
                if c.mihomo.online && !c.mihomo.dashboard.is_empty() {
                    vec![Action::MihomoOpenDashboard]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

const MIHOMO_MODES: [&str; 3] = ["rule", "global", "direct"];

fn slider_index(id: Z) -> Option<usize> {
    match id {
        Z::Brightness => Some(0),
        Z::Volume => Some(1),
        Z::Chassis => Some(2),
        Z::Pump => Some(3),
        _ => None,
    }
}

fn switch_colors(pal: &Palette, on: bool) -> (Color, Color) {
    let d = pal.dark;
    if on {
        (with_alpha(rgb(if d { "#7f99bd" } else { "#8faad1" }), if d { 0.94 } else { 0.90 }), with_alpha(rgb(if d { "#98b2d8" } else { "#7f9ec8" }), if d { 0.84 } else { 0.62 }))
    } else {
        (with_alpha(pal.primary_text, if d { 0.16 } else { 0.14 }), with_alpha(pal.primary_text, if d { 0.18 } else { 0.12 }))
    }
}

// ------------------------------------------------------------------ main page

fn main_page(c: &mut Ctx<Z>, e: &Model, pabs: (f32, f32), inter: &Interaction<Z>, sliders: &[SliderState; 4], laptop: bool) -> (Vec<Node>, f32) {
    let mut out = Vec::new();
    let n = &e.network;
    let bt = &e.bluetooth;
    let ct = &e.controls;
    // Wi-Fi split tile
    let wifi_sub = if !n.wifi_radio_enabled {
        "Turned off".to_string()
    } else if n.wifi_connected {
        if n.wifi_ssid.is_empty() { "Connected".into() } else { n.wifi_ssid.clone() }
    } else {
        "Not connected".into()
    };
    let wifi = Split {
        icon: if n.wifi_radio_enabled { "\u{F0928}".into() } else { "\u{F092D}".into() },
        title: "WIFI".into(),
        subtitle: wifi_sub,
        active: n.wifi_radio_enabled,
        destructive: false,
        left_enabled: true,
        right_enabled: true,
        chevron: false,
        expanded: false,
    };
    out.push(widgets::split_tile(c, Rect::new(0., 0., COLUMN_W, MODULE), pabs, &wifi, Z::WifiIcon, Z::WifiText, inter));
    // Bluetooth split tile
    let connected: Vec<&BtDevice> = bt.devices.iter().filter(|d| d.connected).collect();
    let paired = bt.devices.iter().filter(|d| d.paired).count();
    // currentBluetoothTitle(): first connected device with a non-empty `name`
    let title = connected.iter().find(|d| !d.name.is_empty()).map(|d| d.name.clone()).unwrap_or_else(|| "Bluetooth".into());
    let sub = if !bt.present {
        "No adapter".to_string()
    } else if !bt.enabled {
        "Turned off".into()
    } else if connected.len() > 1 {
        format!("{} connected", connected.len())
    } else if connected.len() == 1 {
        "Connected".into()
    } else if bt.discovering {
        "Scanning".into()
    } else if paired > 0 {
        format!("{paired} paired")
    } else {
        "Ready".into()
    };
    let bts = Split { icon: if bt.enabled { "\u{F00AF}".into() } else { "\u{F00B2}".into() }, title, subtitle: sub, active: bt.enabled, destructive: false, left_enabled: true, right_enabled: true, chevron: false, expanded: false };
    out.push(widgets::split_tile(c, Rect::new(0., 72., COLUMN_W, MODULE), pabs, &bts, Z::BtIcon, Z::BtText, inter));
    let assets = e.assets.clone();
    let pg = match ct.profile {
        Profile::PowerSaver => "\u{F0F86}",
        Profile::Performance => "\u{F04C5}",
        Profile::Balanced => "\u{F0F85}",
    };
    let split = |icon: &str, title: &str, subtitle: &str, active: bool| Split {
        icon: icon.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        active,
        destructive: false,
        left_enabled: true,
        right_enabled: true,
        chevron: false,
        expanded: false,
    };
    if laptop {
        // Power split tile replaces VM │ Viewflow (laptop-panels.md §1.1)
        let sub = match ct.profile {
            Profile::PowerSaver => "Battery",
            Profile::Performance => "Performance",
            Profile::Balanced => "Balanced",
        };
        let power = split(pg, "Power", sub, ct.profile == Profile::PowerSaver);
        out.push(widgets::split_tile(c, Rect::new(0., 144., COLUMN_W, MODULE), pabs, &power, Z::Profile, Z::ProfileText, inter));
    } else {
        // VM + Viewflow
        let mut vm = Toggle::icon("\u{F05B3}", ct.vm_running);
        vm.enabled = !ct.vm_busy;
        vm.label = if ct.vm_busy { if ct.vm_running { "Sleeping" } else { "Starting" } } else if ct.vm_running { "Windows on" } else { "Windows off" }.into();
        out.push(widgets::toggle(c, Rect::new(0., 144., MODULE, MODULE), pabs, &vm, &assets, Z::Vm, inter));
        let mut vf = Toggle::icon(if ct.viewflow_target == Target::MacOs { "\u{F179}" } else { "\u{F05B3}" }, if QUIRK_VIEWFLOW_NEVER_ACTIVE { false } else { ct.viewflow_target == Target::MacOs });
        vf.enabled = !ct.viewflow_busy;
        out.push(widgets::toggle(c, Rect::new(72., 144., MODULE, MODULE), pabs, &vf, &assets, Z::Viewflow, inter));
    }
    out.push(widgets::toggle(c, Rect::new(0., 216., MODULE, MODULE), pabs, &Toggle::icon("\u{F0EC3}", ct.screen_recording), &assets, Z::Record, inter));
    out.push(widgets::toggle(c, Rect::new(72., 216., MODULE, MODULE), pabs, &Toggle::icon(if ct.prevent_sleep { "\u{F0F4}" } else { "\u{F236}" }, ct.prevent_sleep), &assets, Z::Sleep, inter));
    // right column
    out.push(widgets::media_card(c, Rect::new(144., 0., COLUMN_W, COLUMN_W), pabs, &e.media, [Z::Media(MediaButton::Previous), Z::Media(MediaButton::PlayPause), Z::Media(MediaButton::Next)]));
    let mut theme = Toggle::icon("", e.theme.dark);
    theme.svg = Some("assets/control-panel/light-dark.svg".into());
    out.push(widgets::toggle(c, Rect::new(144., 144., MODULE, MODULE), pabs, &theme, &assets, Z::Theme, inter));
    out.push(widgets::toggle(c, Rect::new(216., 144., MODULE, MODULE), pabs, &Toggle::icon("\u{F009B}", ct.dnd), &assets, Z::Dnd, inter));
    if laptop {
        // Display split tile replaces Demo │ Power-profile (laptop-panels.md §1.1)
        let display = split("\u{F0379}", "Display", if ct.demo_mode { "Demo mode" } else { "Normal" }, ct.demo_mode);
        out.push(widgets::split_tile(c, Rect::new(144., 216., COLUMN_W, MODULE), pabs, &display, Z::Demo, Z::DemoText, inter));
    } else {
        out.push(widgets::toggle(c, Rect::new(144., 216., MODULE, MODULE), pabs, &Toggle::icon("\u{F0379}", ct.demo_mode), &assets, Z::Demo, inter));
        out.push(widgets::toggle(c, Rect::new(216., 216., MODULE, MODULE), pabs, &Toggle::icon(pg, !QUIRK_PROFILE_TOGGLE_NEVER_ACTIVE), &assets, Z::Profile, inter));
    }
    // sliders
    let shown = |i: usize, v: i32| if sliders[i].adjusting { sliders[i].preview } else { v as f32 };
    let sl = |icon: &str, v: f32| Slider { icon: icon.into(), value: v, icon_left_inset: -1. };
    out.push(widgets::slider(c, Rect::new(0., 288., COLUMN_W, MODULE), pabs, &sl("\u{F00DF}", shown(0, ct.brightness_percent)), Z::Brightness, true));
    out.push(widgets::slider(c, Rect::new(144., 288., COLUMN_W, MODULE), pabs, &sl(volume_icon(&e.audio), shown(1, e.audio.volume_percent)), Z::Volume, true));
    if laptop {
        // no chassis fan row: one slider row, 278 + 10 + 62
        return (out, 278. + 10. + MODULE);
    }
    let fan = |c: &mut Ctx<Z>, x: f32, v: f32, id: Z| {
        let mut n = widgets::slider(c, Rect::new(x, 360., COLUMN_W, MODULE), pabs, &sl("\u{F0210}", v), id, ct.fan_available);
        if !ct.fan_available {
            n.opacity = 0.48;
        }
        n
    };
    out.push(fan(c, 0., shown(2, ct.chassis_fan_percent), Z::Chassis));
    out.push(fan(c, 144., shown(3, ct.pump_fan_percent), Z::Pump));
    (out, 278. + 10. + 134.)
}

// ------------------------------------------------------------------ bluetooth page

pub fn visible_devices(e: &Model) -> Vec<&BtDevice> {
    e.bluetooth.devices.iter().filter(|d| e.bluetooth_show_unnamed || d.has_name).collect()
}

fn device_glyph(d: &BtDevice, enabled: bool) -> &'static str {
    let i = d.icon.to_lowercase();
    if ["audio-headset", "audio-headphones", "audio-card", "audio-speakers"].iter().any(|s| i.contains(s)) {
        "\u{F02CB}"
    } else if i.contains("input-mouse") {
        "\u{F037D}"
    } else if i.contains("input-keyboard") {
        "\u{F030C}"
    } else if i.contains("phone") {
        "\u{F03F2}"
    } else if i.contains("camera") {
        "\u{F0100}"
    } else if i.contains("printer") {
        "\u{F042A}"
    } else if i.contains("computer") {
        "\u{F0322}"
    } else if enabled {
        "\u{F00AF}"
    } else {
        "\u{F00B2}"
    }
}

fn device_meta(d: &BtDevice) -> String {
    let mut p = vec![if d.connected { "Connected".to_string() } else if d.paired { "Paired".into() } else { "Available".into() }];
    if !d.has_name {
        p.push("Unnamed".into());
    }
    if d.trusted {
        p.push("Trusted".into());
    }
    if d.blocked {
        p.push("Blocked".into());
    }
    if let Some(r) = d.rssi {
        p.push(format!("{r} dBm"));
    }
    p.join("  •  ")
}

const DEVICE_ROW_H: f32 = 38.; // max(19 + 4 + 15, 32)

/// (content height, viewport height) of the device list.
fn bt_list_extent(e: &Model) -> (f32, f32) {
    let n = visible_devices(e).len() as f32;
    let content = if n > 0. { n * (DEVICE_ROW_H + 20.) + (n - 1.) * 10. } else { 0. };
    (content, content.min(420.))
}

fn bluetooth_page(c: &mut Ctx<Z>, e: &Model, pabs: (f32, f32), inter: &Interaction<Z>, scroll: f32, sw: (Color, Color, f32)) -> (Vec<Node>, f32) {
    let pal = c.pal;
    let bt = &e.bluetooth;
    let devices = &bt.devices;
    let connected: Vec<&BtDevice> = devices.iter().filter(|d| d.connected).collect();
    let paired = devices.iter().filter(|d| d.paired).count();
    let visible = visible_devices(e);
    let hidden_unnamed = devices.len() - visible.len();
    let col_x = 10.;
    let w = 364.;
    let cabs = (pabs.0 + col_x, pabs.1 + 10.);
    let mut col: Vec<Node> = Vec::new();
    let mut y = 0.;
    // 1. header
    {
        let hdr = Rect::new(0., y, w, 40.);
        let habs = at(cabs, hdr);
        let mut h = group(hdr);
        let title = widgets::label("Bluetooth", 17., BOLD, pal.primary_text);
        let (tw, th) = text_size(&title);
        h.children.push(text(&title, Rect::new(0., center(40., th), tw, th), habs, c.dpr));
        let row_h = th.max(28.);
        let sx = tw + 12.;
        let sframe = Rect::new(sx, center(40., row_h) + center(row_h, 28.), 48., 28.);
        h.children.push(widgets::power_switch(c, sframe, habs, widgets::PowerSwitch { fill: sw.0, stroke: sw.1, knob_x: sw.2, enabled: bt.present && !bt.busy, dim: !bt.present || bt.busy }, Z::BtPower));
        let close = Chip::new(pal, "Close").min(76.).r(9.).colors(with_alpha(pal.primary_text, if pal.dark { 0.08 } else { 0.12 }), pal.primary_text, pal.card_stroke);
        let cw = close.width();
        let (n, _) = widgets::chip(c, w - cw, center(40., CHIP_HEIGHT), habs, &close, Z::BtClose, inter);
        h.children.push(n);
        col.push(h);
        y += 40. + 10.;
    }
    // 2. status card
    {
        let summary = if !bt.present {
            "No Bluetooth adapter detected".to_string()
        } else if !bt.enabled {
            "Bluetooth is turned off".into()
        } else if connected.len() > 1 {
            format!("{} Bluetooth devices connected", connected.len())
        } else if connected.len() == 1 {
            let d = connected[0];
            if !d.display_name.is_empty() { d.display_name.clone() } else if !d.address.is_empty() { d.address.clone() } else { "Bluetooth connected".into() }
        } else if bt.discovering {
            "Scanning nearby Bluetooth devices".into()
        } else if paired > 0 {
            format!("{paired} paired Bluetooth devices")
        } else {
            "Bluetooth ready".into()
        };
        let detail = if !bt.present {
            "No Bluetooth controller was detected".to_string()
        } else if !bt.enabled {
            "Turn Bluetooth on to discover and connect devices".into()
        } else if !connected.is_empty() {
            if connected.len() == 1 { "Connected and ready for audio or input".into() } else { format!("{} active Bluetooth connections", connected.len()) }
        } else if bt.discovering {
            "Searching for nearby devices".into()
        } else if paired > 0 {
            format!("{paired} saved devices available")
        } else if bt.pairable {
            "Pairable and ready for nearby devices".into()
        } else {
            "Bluetooth adapter available".into()
        };
        let status = if !bt.present { "No BT" } else if !bt.enabled { "Off" } else if !connected.is_empty() { "Online" } else if bt.discovering { "Scan" } else { "Ready" };
        let on = !connected.is_empty();
        let (node, h) = status_card(c, Rect::new(0., y, w, 0.), cabs, StatusCard { glyph: if bt.enabled { "\u{F00AF}" } else { "\u{F00B2}" }, summary: &summary, detail: &detail, status, on }, None);
        col.push(node);
        y += h + 10.;
    }
    // 3. action chips (Flow)
    {
        let scan = Chip::new(pal, if bt.discovering { "Scanning" } else { "Scan" }).min(88.).r(9.).colors(pal.card_fill, pal.primary_text, pal.card_stroke).disabled(!bt.present || !bt.enabled || bt.busy);
        let show = e.bluetooth_show_unnamed;
        let unnamed = Chip::new(pal, if show { "Hide Unnamed" } else { "Show Unnamed" }).min(132.).r(9.).colors(if show { pal.card_strong_fill } else { pal.card_fill }, if show { pal.accent } else { pal.primary_text }, if show { with_alpha(pal.accent, 0.18) } else { pal.card_stroke });
        let adv = Chip::new(pal, "Advanced").min(98.).r(9.).colors(pal.card_fill, pal.primary_text, pal.card_stroke);
        let fl = flow(c, Rect::new(0., y, w, 0.), cabs, vec![(scan, Z::BtScan), (unnamed, Z::BtUnnamed), (adv, Z::BtAdvanced)], inter);
        y += fl.1 + 10.;
        col.push(fl.0);
    }
    // 4. message
    if !bt.action_message.is_empty() {
        let (n, h) = message_card(c, Rect::new(0., y, w, 0.), cabs, &bt.action_message);
        col.push(n);
        y += h + 10.;
    }
    // 5/6. empty state or list
    if visible.is_empty() {
        let txt = if !bt.present {
            "No Bluetooth adapter detected."
        } else if !bt.enabled {
            "Turn Bluetooth on to scan for nearby devices."
        } else if hidden_unnamed > 0 && !e.bluetooth_show_unnamed {
            "Only unnamed Bluetooth devices are hidden right now. Tap \"Show Unnamed\" to reveal them."
        } else if bt.discovering {
            "Searching for nearby devices..."
        } else {
            "No nearby or paired Bluetooth devices right now."
        };
        let (n, h) = empty_card(c, Rect::new(0., y, w, 0.), cabs, txt);
        col.push(n);
        y += h;
    } else {
        let (content, view) = bt_list_extent(e);
        let lframe = Rect::new(0., y, w, view);
        let labs = at(cabs, lframe);
        c.zone(Z::BtList, abs(cabs, lframe), Cursor::Default);
        let mut items = Vec::new();
        let mut iy = -scroll;
        for (i, d) in visible.iter().enumerate() {
            let card_h = DEVICE_ROW_H + 20.;
            let cframe = Rect::new(0., iy, w, card_h);
            let ca = at(labs, cframe);
            let mut card = rect(cframe, 9., panel_color(if d.connected { pal.accent_fill } else { pal.card_fill }), Some((1., if d.connected { pal.accent_stroke } else { pal.card_stroke })));
            let row = Rect::new(10., 10., w - 20., DEVICE_ROW_H);
            let ra = at(ca, row);
            let mut rn = group(row);
            let g = widgets::label(device_glyph(d, bt.enabled), 24., BOLD, pal.primary_text);
            let (gw, gh) = text_size(&g);
            rn.children.push(text(&g, Rect::new(0., center(DEVICE_ROW_H, gh), gw, gh), ra, c.dpr));
            let chip = Chip::icon(pal, if d.connected { "\u{F08B}" } else { "\u{F090}" })
                .min(50.)
                .r(9.)
                .colors(if d.connected { pal.card_strong_fill } else { pal.card_fill }, if d.connected { pal.critical } else { pal.accent }, if d.connected { with_alpha(pal.critical, 0.20) } else { with_alpha(pal.accent, 0.18) })
                .disabled(bt.busy || (!bt.enabled && !d.connected));
            let chw = chip.width();
            // zone only inside the visible list viewport
            let visible_row = iy + card_h > 0. && iy < view;
            let mut zs = Vec::new();
            let (cn, _) = {
                let mut cc = Ctx { dpr: c.dpr, pal, zones: &mut zs };
                widgets::chip(&mut cc, row.width - chw, center(DEVICE_ROW_H, CHIP_HEIGHT), ra, &chip, Z::BtDevice(i), inter)
            };
            if visible_row {
                for mut z in zs {
                    let top = labs.1.max(z.rect.y);
                    let bottom = (labs.1 + view).min(z.rect.y + z.rect.height);
                    if bottom > top {
                        z.rect = Rect::new(z.rect.x, top, z.rect.width, bottom - top);
                        c.zones.push(z);
                    }
                }
            }
            rn.children.push(cn);
            let info_x = gw + 10.;
            let info_w = (row.width - chw - 10. - info_x).max(0.);
            let name = if d.display_name.is_empty() { d.address.clone() } else { d.display_name.clone() };
            let nt = widgets::label(&name, 14., BOLD, pal.primary_text).width(info_w).elide();
            let mt = widgets::label_n(&device_meta(d), 11., pal.muted_text).width(info_w).elide();
            let nh = text_size(&nt).1;
            let mh = text_size(&mt).1;
            let ih = nh + 4. + mh;
            let iframe = Rect::new(info_x, center(DEVICE_ROW_H, ih), info_w, ih);
            let ia = at(ra, iframe);
            let mut info = group(iframe);
            info.children.push(text(&nt, Rect::new(0., 0., info_w, nh), ia, c.dpr));
            info.children.push(text(&mt, Rect::new(0., nh + 4., info_w, mh), ia, c.dpr));
            rn.children.push(info);
            card.children.push(rn);
            items.push(card);
            iy += card_h + 10.;
        }
        let _ = content;
        col.push(clip(lframe, items));
        y += view;
    }
    let page_h = y + 20.;
    let column = group_with(Rect::new(col_x, 10., w, y), col);
    (vec![column], page_h)
}

/// The existing text and active state shared by Bluetooth and Wi-Fi status cards.
pub struct StatusCard<'a> {
    pub glyph: &'a str,
    pub summary: &'a str,
    pub detail: &'a str,
    pub status: &'a str,
    pub on: bool,
}

/// The status card shared by the Bluetooth page and the Wi-Fi popup: icon, info column
/// (summary 14 Bold, detail 12), disabled status pill. Returns (node, height).
pub fn status_card<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), content: StatusCard<'_>, icon: Option<Node>) -> (Node, f32) {
    let StatusCard { glyph, summary, detail, status, on } = content;
    let pal = c.pal;
    let w = frame.width;
    let pill = Chip::new(pal, status).min(72.).r(9.).colors(pal.card_fill, if on { pal.accent } else { pal.primary_text }, if on { with_alpha(pal.accent, 0.18) } else { pal.card_stroke }).disabled(true);
    let pw = pill.width();
    let g = widgets::label(glyph, 24., BOLD, pal.primary_text);
    let (gw, gh) = if icon.is_some() { (24., 24.) } else { text_size(&g) };
    let info_x = gw + 10.;
    let body_w = w - 20.;
    let info_w = (body_w - pw - 10. - info_x).max(0.);
    let st = widgets::label(summary, 14., BOLD, pal.primary_text).width(info_w).elide();
    let dt = widgets::label_n(detail, 12., pal.muted_text).width(info_w).elide();
    let sh = text_size(&st).1;
    let dh = text_size(&dt).1;
    let info_h = sh + 4. + dh;
    let body_h = info_h.max(gh);
    let h = body_h + 24.;
    let f = Rect::new(frame.x, frame.y, w, h);
    let fa = at(pabs, f);
    let mut n = rect(f, 9., panel_color(if on { pal.accent_fill } else { pal.card_strong_fill }), Some((1., if on { pal.accent_stroke } else { pal.card_stroke })));
    let body = Rect::new(10., 10., body_w, body_h);
    let ba = at(fa, body);
    let mut b = group(body);
    match icon {
        Some(mut i) => {
            i.frame.x = 0.;
            i.frame.y = center(body_h, 24.);
            b.children.push(i);
        }
        None => b.children.push(text(&g, Rect::new(0., center(body_h, gh), gw, gh), ba, c.dpr)),
    }
    let (pn, _) = widgets::chip(c, body_w - pw, center(body_h, CHIP_HEIGHT), ba, &pill, None, &Interaction::default());
    b.children.push(pn);
    let iframe = Rect::new(info_x, center(body_h, info_h), info_w, info_h);
    let ia = at(ba, iframe);
    let mut info = group(iframe);
    info.children.push(text(&st, Rect::new(0., 0., info_w, sh), ia, c.dpr));
    info.children.push(text(&dt, Rect::new(0., sh + 4., info_w, dh), ia, c.dpr));
    b.children.push(info);
    n.children.push(b);
    (n, h)
}


/// Message card: text 12 px primaryText α.82 wrapped, top 10 / bottom 8.
pub fn message_card<Z>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), msg: &str) -> (Node, f32) {
    let pal = c.pal;
    let w = frame.width;
    let t = widgets::label_n(msg, 12., with_alpha(pal.primary_text, 0.82)).width(w - 20.).wrap();
    let th = text_size(&t).1;
    let f = Rect::new(frame.x, frame.y, w, th + 18.);
    let mut n = rect(f, 9., panel_color(pal.card_fill), Some((1., pal.card_stroke)));
    n.children.push(text(&t, Rect::new(10., 10., w - 20., th), at(pabs, f), c.dpr));
    (n, th + 18.)
}

/// Empty-state card: centred text (width w−28, 13 px mutedText, wrap), height text + 26.
pub fn empty_card<Z>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), msg: &str) -> (Node, f32) {
    let pal = c.pal;
    let w = frame.width;
    let t = widgets::label_n(msg, 13., pal.muted_text).width(w - 28.).wrap().align(Align::Center);
    let th = text_size(&t).1;
    let h = th + 26.;
    let f = Rect::new(frame.x, frame.y, w, h);
    let mut n = rect(f, 9., panel_color(pal.card_fill), Some((1., pal.card_stroke)));
    n.children.push(text(&t, Rect::new(center(w, w - 28.), center(h, th), w - 28., th), at(pabs, f), c.dpr));
    (n, h)
}

/// Qt Flow of chips (spacing 10). Returns (node, height).
pub fn flow<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), chips: Vec<(Chip, Z)>, inter: &Interaction<Z>) -> (Node, f32) {
    let fa = at(pabs, Rect::new(frame.x, frame.y, 0., 0.));
    let mut g = group(Rect::new(frame.x, frame.y, frame.width, 0.));
    let (mut x, mut y) = (0., 0.);
    for (chip, id) in chips {
        let w = chip.width();
        if x > 0. && x + w > frame.width {
            x = 0.;
            y += CHIP_HEIGHT + 10.;
        }
        let (n, _) = widgets::chip(c, x, y, fa, &chip, id, inter);
        g.children.push(n);
        x += w + 10.;
    }
    let h = y + CHIP_HEIGHT;
    g.frame.height = h;
    (g, h)
}

// ------------------------------------------------------------------ mihomo page

/// `formatRate(b)` (MihomoControlPanel.qml)
pub fn format_rate(b: u64) -> String {
    let b = b as f64;
    if b >= 1048576. {
        format!("{:.*} MB/s", if b >= 10. * 1048576. { 0 } else { 1 }, b / 1048576.)
    } else if b >= 1024. {
        format!("{:.*} KB/s", if b >= 10. * 1024. { 0 } else { 1 }, b / 1024.)
    } else {
        format!("{} B/s", b.round())
    }
}

fn mihomo_page(c: &mut Ctx<Z>, e: &Model, pabs: (f32, f32), _inter: &Interaction<Z>, pulse_t: Duration) -> (Vec<Node>, f32) {
    let pal = c.pal;
    let m = &e.controls.mihomo;
    let w = 364.;
    let cabs = (pabs.0 + 10., pabs.1 + 10.);
    let mut col = Vec::new();
    let mut y = 0.;
    let down = pal.usage_low;
    let up = pal.microphone;
    let qt = |s: &str, px: f32, weight: i32, color: Color| TextBox::new(s, Font::qt(px, weight), color);
    let dflt = |s: &str, px: f32, weight: i32, color: Color| TextBox::new(s, Font::qt(px, weight).family(crate::qtext::DEFAULT_FONT), color);
    // 1. header row
    {
        let r = Rect::new(0., y, w, 40.);
        let ra = at(cabs, r);
        let mut row = group(r);
        let mut ib = rect(Rect::new(0., 0., 40., 40.), 12., if m.online { with_alpha(pal.accent, 0.24) } else { pal.card_strong_fill }, None);
        ib.children.push(text_center(&qt(if m.online { "\u{F0582}" } else { "\u{F05AA}" }, 20., BOLD, if m.online { pal.accent } else { pal.muted_text }), 40., 40., ra, c.dpr));
        row.children.push(ib);
        let tw = w - 100.;
        let title = qt(&if m.online { format!("Mihomo · {}", if m.mode.is_empty() { "Connected" } else { &m.mode }) } else { "Mihomo".into() }, 13., BOLD, pal.primary_text).width(tw).elide();
        let sub = qt(&if m.online { format!("{} active connections", m.connections) } else { "Core is not reachable".into() }, 9., NORMAL, pal.muted_text).width(tw).elide();
        let (th, sh) = (text_size(&title).1, text_size(&sub).1);
        let ch = th + 2. + sh;
        let cf = Rect::new(50., center(40., ch), tw, ch);
        let ca = at(ra, cf);
        row.children.push(group_with(cf, vec![text(&title, Rect::new(0., 0., tw, th), ca, c.dpr), text(&sub, Rect::new(0., th + 2., tw, sh), ca, c.dpr)]));
        let pf = Rect::new(50. + tw + 10., center(40., 24.), 40., 24.);
        let mut pill = rect(pf, 12., if m.online { with_alpha(down, 0.22) } else { pal.card_strong_fill }, None);
        pill.children.push(text_center(&dflt(if m.online { "ON" } else { "OFF" }, 9., BOLD, if m.online { down } else { pal.muted_text }), 40., 24., at(ra, pf), c.dpr));
        row.children.push(pill);
        c.zone_if(m.service == "user" || m.service == "system", Z::MhPower, abs(ra, pf), Cursor::Pointer);
        col.push(row);
        y += 40. + 10.;
    }
    // 2. rates
    {
        let cw = (w - 8.) / 2.;
        for (i, (glyph, label, rate, tone)) in [("\u{F01DA}", "Download", m.download, down), ("\u{F0552}", "Upload", m.upload, up)].into_iter().enumerate() {
            let f = Rect::new(i as f32 * (cw + 8.), y, cw, 54.);
            let fa = at(cabs, f);
            // border pulse: tone (650 ms) → cardStroke (650 ms), linear, infinite
            let border = if m.online && rate > 1024 {
                let t = (pulse_t.as_millis() % 1300) as f32;
                let p = if t < 650. { t / 650. } else { 1. - (t - 650.) / 650. };
                mix(pal.card_stroke, tone, p)
            } else {
                pal.card_stroke
            };
            let mut card = rect(f, 14., pal.card_fill, Some((1., border)));
            let g = qt(glyph, 16., NORMAL, tone);
            let (gw, gh) = text_size(&g);
            let v = qt(&format_rate(rate), 11., BOLD, pal.primary_text);
            let l = qt(label, 8., NORMAL, pal.muted_text);
            let (vw, vh) = text_size(&v);
            let (lw, lh) = text_size(&l);
            let colh = vh + 1. + lh;
            let inner = Rect::new(9., 9., cw - 18., 54. - 18.);
            let ia = at(fa, inner);
            let mut row = group(inner);
            // Row { anchors.fill; anchors.margins: 9 }: children centre in the full 36 px
            row.children.push(text(&g, Rect::new(0., center(inner.height, gh), gw, gh), ia, c.dpr));
            let cf = Rect::new(gw + 8., center(inner.height, colh), vw.max(lw), colh);
            let ca = at(ia, cf);
            row.children.push(group_with(cf, vec![text(&v, Rect::new(0., 0., vw, vh), ca, c.dpr), text(&l, Rect::new(0., vh + 1., lw, lh), ca, c.dpr)]));
            card.children.push(row);
            col.push(card);
        }
        y += 54. + 10.;
    }
    // 3. modes
    {
        let bw = (w - 14.) / 3.;
        for (i, mode) in MIHOMO_MODES.iter().enumerate() {
            let f = Rect::new(i as f32 * (bw + 7.), y, bw, 34.);
            let sel = m.mode == *mode;
            let mut b = rect(f, 11., if sel { with_alpha(pal.accent, 0.25) } else { pal.card_fill }, Some((1., if sel { pal.accent } else { pal.card_stroke })));
            let name: String = mode[..1].to_uppercase() + &mode[1..];
            b.children.push(text_center(&dflt(&name, 10., if sel { BOLD } else { NORMAL }, if sel { pal.accent } else { pal.muted_text }), bw, 34., at(cabs, f), c.dpr));
            col.push(b);
            c.zone_if(m.online && !m.busy, Z::MhMode(i), abs(cabs, f), Cursor::Pointer);
        }
        y += 34. + 10.;
    }
    // 4. refresh / dashboard
    {
        let bw = (w - 8.) / 2.;
        for (i, (label, color, enabled, id)) in [
            (if m.busy { "Working…" } else { "Refresh" }, pal.muted_text, !m.busy, Z::MhRefresh),
            ("Dashboard  ↗", if m.online { pal.accent } else { pal.muted_text }, m.online && !m.dashboard.is_empty(), Z::MhDashboard),
        ]
        .into_iter()
        .enumerate()
        {
            let f = Rect::new(i as f32 * (bw + 8.), y, bw, 34.);
            let mut b = rect(f, 11., pal.card_fill, Some((1., pal.card_stroke)));
            b.children.push(text_center(&dflt(label, 10., BOLD, color), bw, 34., at(cabs, f), c.dpr));
            col.push(b);
            c.zone_if(enabled, id, abs(cabs, f), Cursor::Pointer);
        }
        y += 34. + 10.;
    }
    // 5. message
    if !m.message.is_empty() || m.service == "verge" {
        let msg = if m.message.is_empty() { "Managed by Clash Verge · controls stay read-only until migration" } else { &m.message };
        let t = qt(msg, 8., NORMAL, if m.message.is_empty() { pal.muted_text } else { pal.critical }).width(w).wrap();
        let th = text_size(&t).1;
        col.push(text(&t, Rect::new(0., y, w, th), cabs, c.dpr));
        y += th + 10.;
    }
    // 6. divider, 7. back
    col.push(rect(Rect::new(0., y, w, 1.), 0., pal.card_stroke, None));
    y += 1. + 10.;
    let back = dflt("‹  Back", 10., BOLD, pal.muted_text);
    let (bw, bh) = text_size(&back);
    col.push(text(&back, Rect::new(0., y, bw, bh), cabs, c.dpr));
    c.zone(Z::MhBack, abs(cabs, Rect::new(0., y, bw, bh)), Cursor::Pointer);
    y += bh;
    (vec![group_with(Rect::new(10., 10., w, y), col)], y + 20.)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

#[cfg(test)]
#[path = "scroll_tests/bluetooth.rs"]
mod scroll_tests;

#[cfg(test)]
mod grab_tests {
    use super::*;

    /// PANEL-2: a Leave/Cancel mid-drag ends the slider grab like Qt's ungrab
    /// (MouseArea onCanceled → adjustmentFinished(preview)); later hovering changes nothing.
    #[test]
    fn leave_or_cancel_ends_slider_grab() {
        for lost in [Pointer::Leave, Pointer::Cancel] {
            let mut m = crate::fixtures::control_idle(false);
            m.now = ms(1000);
            let mut c = ControlPanel::with_profile(false);
            c.open_for(Anchor { x: 1400., y: 10., width: 40., height: 38. }, &m.output.name.clone(), m.now);
            let _ = c.build(&m);
            let z = c.zones.iter().find(|z| z.id == Z::Brightness).expect("brightness zone").rect;
            let (x, y) = (z.x + z.width * 0.5, z.y + z.height * 0.5);
            let r = c.pointer(Pointer::Press { x, y, button: Button::Left }, &m);
            assert!(r.actions.iter().any(|a| matches!(a, Action::BrightnessSet(_))));
            let r = c.pointer(lost, &m);
            assert!(r.actions.iter().any(|a| matches!(a, Action::BrightnessFinish(_))), "{lost:?}: {:?}", r.actions);
            let r = c.pointer(Pointer::Motion { x: z.x + 10., y }, &m);
            assert!(!r.actions.iter().any(|a| matches!(a, Action::BrightnessSet(_))), "{lost:?}: hover still drags");
            // a fresh press is accepted again
            let r = c.pointer(Pointer::Press { x, y, button: Button::Left }, &m);
            assert!(r.actions.iter().any(|a| matches!(a, Action::BrightnessSet(_))));
        }
    }
}

#[cfg(test)]
mod laptop_tests {
    use super::*;

    fn laptop_model() -> Model {
        let mut m = crate::fixtures::control_idle(false);
        m.output = Output { name: "eDP-1".into(), width: 1440., height: 900., scale: 2. };
        m.shell.bar_exclusive_zone = 25.;
        m.now = ms(1000);
        m
    }
    fn zone(c: &ControlPanel, id: Z) -> Option<Rect> {
        c.zones.iter().find(|z| z.id == id).map(|z| z.rect)
    }
    fn click(c: &mut ControlPanel, m: &Model, id: Z) -> Vec<Action> {
        let z = zone(c, id).unwrap();
        let (x, y) = (z.x + z.width * 0.5, z.y + z.height * 0.5);
        let _ = c.pointer(Pointer::Press { x, y, button: Button::Left }, m);
        c.pointer(Pointer::Release { x, y, button: Button::Left }, m).actions
    }

    /// laptop-panels.md §1.1/§1.3: 278×350, centred by IPC, no fan/VM/Viewflow zones.
    #[test]
    fn main_page_layout_and_centring() {
        let m = laptop_model();
        let mut c = ControlPanel::with_profile(true);
        c.toggle_centered("eDP-1", m.now);
        let _ = c.build(&m);
        assert_eq!((c.last_card.x, c.last_card.y, c.last_card.width, c.last_card.height), (581., 275., 278., 350.));
        for gone in [Z::Vm, Z::Viewflow, Z::Chassis, Z::Pump] {
            assert!(zone(&c, gone).is_none(), "{gone:?}");
        }
        let p = zone(&c, Z::Profile).unwrap();
        assert_eq!((p.x - 581., p.y - 275.), (0., 144.));
        let d = zone(&c, Z::DemoText).unwrap();
        assert!(d.x - 581. > 144. && d.y - 275. == 216.);
    }

    #[test]
    fn anchored_open_clamps_inside_output() {
        let mut m = laptop_model();
        let mut c = ControlPanel::with_profile(true);
        c.open_for(Anchor { x: 1400., y: 0., width: 24., height: 25. }, "eDP-1", m.now);
        let _ = c.build(&m);
        assert_eq!((c.last_card.x, c.last_card.y), (1152., 35.));
        m.output.height = 300.;
        let mut c = ControlPanel::with_profile(true);
        c.open_for(Anchor { x: 1400., y: 0., width: 24., height: 25. }, "eDP-1", m.now);
        let _ = c.build(&m);
        assert_eq!(c.last_card.y, 10.);
    }

    /// Both halves of the Power and Display tiles act; brightness snaps to 1 % and
    /// sends nothing on release (laptop-panels.md §1.2, §1.4).
    #[test]
    fn tiles_and_brightness() {
        let m = laptop_model();
        let mut c = ControlPanel::with_profile(true);
        c.toggle_centered("eDP-1", m.now);
        let _ = c.build(&m);
        for id in [Z::Profile, Z::ProfileText] {
            assert!(click(&mut c, &m, id).iter().any(|a| matches!(a, Action::ProfileSet(_))), "{id:?}");
        }
        for id in [Z::Demo, Z::DemoText] {
            assert!(click(&mut c, &m, id).iter().any(|a| matches!(a, Action::ToggleDemo)), "{id:?}");
        }
        let z = zone(&c, Z::Brightness).unwrap();
        let x = z.x + z.width * 0.37;
        let y = z.y + z.height * 0.5;
        let r = c.pointer(Pointer::Press { x, y, button: Button::Left }, &m);
        let want = snap_brightness_for(true, widgets::slider_value(x - z.x, z.width));
        assert!(r.actions.iter().any(|a| matches!(a, Action::BrightnessSet(v) if *v == want)), "{:?}", r.actions);
        let r = c.pointer(Pointer::Release { x, y, button: Button::Left }, &m);
        assert!(!r.actions.iter().any(|a| matches!(a, Action::BrightnessFinish(_))), "{:?}", r.actions);
        let _ = c.pointer(Pointer::Press { x, y, button: Button::Left }, &m);
        let r = c.pointer(Pointer::Cancel, &m);
        assert!(!r.actions.iter().any(|a| matches!(a, Action::BrightnessFinish(_))), "{:?}", r.actions);
        assert_eq!(snap_brightness_for(true, 37.4), 37);
        assert_eq!(snap_brightness_for(false, 37.4), 38);
    }
}
