//! The `DynamicIsland` controller: QML state (§2), Behaviors (§0.3, §8), timers and the
//! Qt Quick pointer delivery the island depends on (§4).
//!
//! Time is supplied by the host (monotonic, wall-clock ms like `QSG_RENDER_LOOP=basic`).
//! Bindings are re-evaluated on every input change and every [`Island::frame`]; call it
//! once per presented frame while [`Island::animating`] (Behaviors whose target follows
//! another animated value restart on each evaluation, exactly like Qt).
use crate::model::{Agent, IslandAction, Inputs};
use crate::util::{contains, hc, out_cubic, out_quad, Anim, FrameBehavior};
use hyprv_ui::geometry::Rect;
use hyprv_widgets::controls::{Button, CircleGlyphButton, Control, PillButton, PointerEvent, Tok};
use std::time::Duration;
use hyprv_widgets::flickable::{Flickable, Settings as FlickSettings, Wheel};

// ---------------------------------------------------------------- §2.3 constants
pub const COLLAPSED_HEIGHT: f32 = 38.;
pub const EXPANDED_WIDTH: f32 = 392.;
pub const EXPANDED_HEIGHT: f32 = 176.;
pub const AGENT_EXPANDED_WIDTH: f32 = 448.;
pub const SWIPE_THRESHOLD: f32 = 34.;
pub const HOVER_BRIDGE_MARGIN: f32 = 10.;
pub const AGENT_MINIMAL_WIDTH: f32 = 94.;
pub const AGENT_MINIMAL_CLOCK_CENTER_X: f32 = 61.;
pub const AGENT_COMPACT_PILL_WIDTH: f32 = 132.;
pub const AGENT_COMPACT_CLOCK_CENTER_X: f32 = 68.;
pub const MUSIC_DETACHED_WIDTH: f32 = 38.;
pub const AGENT_DETACHED_GAP: f32 = 4.;
pub const DUAL_COLLAPSED_WIDTH: f32 = 136.;
pub const DUAL_MUSIC_LEFT_X: f32 = 98.;
pub const COMPACT_MUSIC_WIDTH: f32 = 120.;
pub const ROW_HEIGHT: f32 = 42.;
pub const ROW_SPACING: f32 = 8.;
pub const AGENT_BUDGET_HEIGHT: f32 = 490.;
pub const AGENT_TOP_RESERVE: f32 = 104.;
/// Island top edge inside the bar surface (`centerSection` topMargin).
pub const TOP_MARGIN: f32 = 10.;
/// Bar strip that the live input region always covers (`shell.qml:3647`).
pub const BAR_STRIP: f32 = 58.;
/// Qt `QStyleHints::startDragDistance` default.
pub const DRAG_THRESHOLD: f32 = 10.;
/// `musicFrameRelease`, `seekPreviewReset` (DynamicIsland.qml:1429-1441).
pub const MUSIC_FRAME_RELEASE: Duration = Duration::from_millis(230);
pub const SEEK_PREVIEW_RESET: Duration = Duration::from_millis(700);
/// AudioSpectrum idle-wave timer.
pub const WAVE_TICK: Duration = Duration::from_millis(66);

/// Quirk switches (ui-port rule: every reproduced QML quirk is a named constant).
/// §4.3: once hidden by a right swipe the x axis is disabled, so a left swipe cannot
/// restore music until `hasMusicContent` goes false.
pub const QUIRK_SWIPE_CANNOT_RESTORE: bool = true;
/// Measured on Qt 6.12 (references `music-swipe-hide`, `music-swipe-from-side`): the
/// DragHandler's `translation` is already reset when `onActiveChanged` runs on release,
/// so `dx` reads 0 and the swipe never hides music. `false` = the intended §4.3 swipe.
pub const QUIRK_SWIPE_TRANSLATION_RESET: bool = true;
/// §4.2 (resolved by reference `idle-hover-lock`): the z-99 side hotspots take hover, so
/// idle lock/power never show their hover fill while the island is collapsed. `false`
/// would let hover reach the buttons under the hotspots.
pub const QUIRK_HOTSPOTS_BLOCK_BUTTON_HOVER: bool = true;
/// Historical fixture constant; production scrolling uses the shared Flickable's
/// platform-configured wheel easing/velocity rather than a fixed pixel jump.
pub const WHEEL_STEP_PX: f32 = 60.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Target {
    #[default]
    None,
    Agent,
    Music,
}

/// Pointer events in **host surface** coordinates (bar surface / parity canvas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Motion { x: f32, y: f32 },
    Leave,
    /// Grab lost (compositor/native cancel, QML `onCanceled`): ends any press/drag/seek.
    Cancel,
    Press { x: f32, y: f32 },
    Release { x: f32, y: f32 },
    /// Qt angleDelta units (120 = one notch); positive = up.
    Wheel { x: f32, y: f32, angle_y: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Default,
    Pointer,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointerOutcome {
    pub actions: Vec<IslandAction>,
    pub cursor: CursorShape,
}

/// What the bar host needs every frame (`IslandGeometry` in hyprv-ui-bar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Animated root size.
    pub width: f32,
    pub height: f32,
    /// Animated `attachedCenterOffset` (host shifts the container by it).
    pub attached_center_offset: f32,
    /// `expanded` (host: surface 820 tall now; back to 58 230 ms after it clears).
    pub expanded: bool,
    /// Island rect in host-surface coordinates (input region part 2).
    pub rect: Rect,
}

/// All QML derived properties (§2.2, §2.4) for one evaluation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Derived {
    pub has_music: bool,
    pub agent_active: bool,
    pub music_active: bool,
    pub music_primary: bool,
    pub dual: bool,
    pub agent_expanded: bool,
    pub music_expanded: bool,
    pub expanded: bool,
    pub pfc: bool,
    pub target_width: f32,
    pub target_height: f32,
    pub main_island_width: f32,
    pub agent_compact_width: f32,
    pub offset: f32,
    pub agent_list_height: f32,
    pub agent_content_height: f32,
    pub agent_expanded_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Area {
    Clock,
    Left,
    Right,
    Seek,
    Prev,
    Play,
    Next,
    Resolver,
    MusicPill,
    AgentPill,
    Lock,
    Power,
    Row(usize),
}
impl Area {
    fn hover_enabled(self) -> bool {
        !matches!(self, Area::Row(_))
    }
    fn accepts_press(self) -> bool {
        !matches!(self, Area::Left | Area::Right | Area::Resolver)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Handler {
    CompactMusic,
    ExpandedMusic,
    CompactAgent,
    ExpandedAgent,
}

/// One item in Qt delivery order: its passive HoverHandler (rect, enabled) and its
/// MouseAreas, topmost first.
type Layer = (Option<(Handler, Rect, bool)>, Vec<(Area, Rect)>);

#[derive(Clone, Copy, Debug)]
struct Drag {
    press: (f32, f32),
    last: (f32, f32),
    active: bool,
    /// TapHandlers that took a passive grab on press.
    tap_compact: bool,
    tap_expanded: bool,
}

pub(crate) struct Anims {
    pub width: Anim,
    pub height: Anim,
    pub offset: Anim,
    pub frame_w: Anim,
    pub frame_r: Anim,
    pub mef_x: Anim,
    pub mef_w: Anim,
    pub mef_r: Anim,
    pub mef_op: Anim,
    pub idle_op: Anim,
    pub cmusic_op: Anim,
    pub cagent_op: Anim,
    pub emusic_op: Anim,
    pub eagent_op: Anim,
    pub fill_w: Anim,
    pub scroll_op: Anim,
}
impl Anims {
    fn new() -> Self {
        let c = |ms| Anim::new(ms, out_cubic());
        let q = |ms| Anim::new(ms, out_quad());
        Self {
            width: c(210), height: c(210), offset: c(210), frame_w: c(210), frame_r: c(210),
            mef_x: c(210), mef_w: c(210), mef_r: c(210), mef_op: q(120),
            idle_op: q(120), cmusic_op: q(140), cagent_op: q(140), emusic_op: q(150), eagent_op: q(150),
            fill_w: c(180), scroll_op: q(120),
        }
    }
    fn all(&self) -> [&Anim; 16] {
        [&self.width, &self.height, &self.offset, &self.frame_w, &self.frame_r, &self.mef_x, &self.mef_w, &self.mef_r,
         &self.mef_op, &self.idle_op, &self.cmusic_op, &self.cagent_op, &self.emusic_op, &self.eagent_op, &self.fill_w, &self.scroll_op]
    }
}

pub(crate) struct Wave {
    pub phase: f64,
    pub next: Option<Duration>,
}

pub struct Island {
    pub(crate) inputs: Inputs,
    pub(crate) tok: Tok,
    host_width: f32,
    /// Live: hover only exists inside the 58-px input strip and the island (§1.1).
    pub clip_hover_to_input_region: bool,
    // QML state (§2.1)
    pub(crate) manually_hidden: bool,
    /// laptop-island.md §4: the adaptive bar is compact (`reveal == 0`), so
    /// `contentRoot` is invisible. Inputs still update; nothing ticks, draws or takes input.
    bar_hidden: bool,
    /// [`Island::visual_rev`] while `bar_hidden`: the tree the host already has.
    hidden_rev: u64,
    pub(crate) seeking: bool,
    pub(crate) seek_preview_s: f64,
    pub(crate) hover_target: Target,
    pub(crate) app_hover_active: bool,
    pub(crate) keep_primary: bool,
    music_frame_release: Option<Duration>,
    seek_preview_reset: Option<Duration>,
    // change tracking for on*Changed handlers
    prev: Option<Derived>,
    prev_calendar_open: bool,
    pub(crate) a: Anims,
    pub(crate) lock: PillButton,
    pub(crate) clock: PillButton,
    pub(crate) power: PillButton,
    pub(crate) prev_btn: CircleGlyphButton,
    pub(crate) play_btn: CircleGlyphButton,
    pub(crate) next_btn: CircleGlyphButton,
    pub(crate) waves: [Wave; 2],
    presented_at: Duration,
    during_frame: bool,
    pub(crate) media_scales: [FrameBehavior;3],
    // pointer
    pointer: Option<(f32, f32)>,
    buttons_down: bool,
    island_hovered: bool,
    contains: Vec<Area>,
    handlers: Vec<Handler>,
    grab: Option<Area>,
    /// Stable session ID of the pressed agent row (`grab == Some(Area::Row(_))`).
    pressed_row: Option<String>,
    drag: Option<Drag>,
    pub(crate) scroll_y: f32,
    list_flick: Flickable,
    pending: Vec<IslandAction>,
    pub(crate) now: Duration,
    /// Bumped whenever [`Island::build`] output may change other than through running
    /// animations (see [`Island::visual_rev`]).
    rev: u64,
    /// `(root, visual_rev)` of the last [`crate::update`] (skips identical rebuilds).
    pub(crate) reconciled: std::cell::Cell<Option<(hyprv_ui::scene::NodeId, u64)>>,
}

impl Island {
    pub fn new(inputs: Inputs, host_width: f32, now: Duration) -> Self {
        let tok = Tok::new(inputs.dark);
        let mut s = Self {
            tok,
            host_width,
            clip_hover_to_input_region: true,
            manually_hidden: false,
            bar_hidden: false,
            hidden_rev: 0,
            seeking: false,
            seek_preview_s: 0.,
            hover_target: Target::None,
            app_hover_active: false,
            keep_primary: false,
            music_frame_release: None,
            seek_preview_reset: None,
            prev: None,
            prev_calendar_open: inputs.calendar_open,
            a: Anims::new(),
            lock: PillButton::island("\u{f023}", 11., 12.),
            clock: { let mut c = PillButton::island("", 0., 0.); c.interactive = false; c },
            power: PillButton::island("\u{f011}", 10., 13.),
            prev_btn: CircleGlyphButton::new("\u{f04ae}", false),
            play_btn: CircleGlyphButton::new("\u{f040a}", true),
            next_btn: CircleGlyphButton::new("\u{f04ad}", false),
            waves: [Wave { phase: 0., next: None }, Wave { phase: 0., next: None }],
            presented_at: now,
            during_frame: false,
            media_scales: std::array::from_fn(|_|FrameBehavior::new()),
            pointer: None,
            buttons_down: false,
            island_hovered: false,
            contains: Vec::new(),
            handlers: Vec::new(),
            grab: None,
            pressed_row: None,
            drag: None,
            scroll_y: 0.,
            list_flick: Flickable::new(FlickSettings::list_view()),
            pending: Vec::new(),
            now,
            rev: 0,
            reconciled: Default::default(),
            inputs,
        };
        s.evaluate(now);
        s
    }

    pub fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    /// New service/host snapshot. Behaviors retarget from their current values.
    pub fn set_inputs(&mut self, inputs: Inputs, now: Duration) {
        if inputs.dark != self.inputs.dark {
            self.tok = Tok::new(inputs.dark);
        }
        if inputs != self.inputs {
            self.rev += 1;
        }
        self.inputs = inputs;
        self.now = now;
        self.reconcile_grab(now);
        self.evaluate(now);
    }

    /// Host calendar state only (`calendarPanelPresented`, `calendarProgress`): same as
    /// [`Island::set_inputs`] with those two fields changed, without cloning the snapshot.
    pub fn set_calendar(&mut self, open: bool, progress: f32, now: Duration) {
        if open == self.inputs.calendar_open && progress == self.inputs.calendar_progress {
            self.now = now;
            return;
        }
        self.inputs.calendar_open = open;
        self.inputs.calendar_progress = progress;
        self.rev += 1;
        self.now = now;
        self.reconcile_grab(now);
        self.evaluate(now);
    }

    /// Ends any press/drag/seek as a grab cancellation (MouseArea `onCanceled`, no click).
    /// Hosts call this (or deliver [`Pointer::Cancel`]) when the native grab is lost.
    pub fn cancel_pointer(&mut self, now: Duration) {
        self.now = now;
        self.rev += 1;
        self.cancel_grab(now);
        self.pointer = None;
        self.evaluate(now);
    }

    fn cancel_grab(&mut self, now: Duration) {
        self.list_flick.cancel();
        if let Some(g) = self.grab.take() {
            self.cancel_area(g, now);
        }
        self.pressed_row = None;
        self.drag = None;
        self.buttons_down = false;
    }

    /// Keeps the grab consistent with a new snapshot: a pressed row follows its session
    /// (cancelled if it disappears), and hiding the island (host opacity ≤ 0.01 while the
    /// calendar covers it) cancels the press like Qt hiding a grabbing MouseArea.
    fn reconcile_grab(&mut self, now: Duration) {
        if let Some(Area::Row(_)) = self.grab {
            let id = self.pressed_row.as_deref();
            match self.inputs.agents.iter().position(|a| Some(a.id.as_str()) == id) {
                Some(j) => self.grab = Some(Area::Row(j)),
                None => {
                    self.grab = None;
                    self.pressed_row = None;
                }
            }
        }
        let hidden = self.inputs.calendar_progress >= 0.99;
        if hidden && (self.grab.is_some() || self.buttons_down || self.drag.is_some()) {
            self.cancel_grab(now);
        }
    }

    /// Laptop adaptive bar `reveal` (laptop-island.md §4). At `reveal <= 0` the island
    /// is hidden: grabs and hover end, the idle waves stop, [`Island::animating`] is false
    /// and [`Island::visual_rev`] stays at the last revealed tree, while
    /// [`Island::set_inputs`] keeps the model current. The first revealed frame renders
    /// the current state. Desktop never calls this (`reveal` is always 1).
    pub fn set_bar_reveal(&mut self, reveal: f64, now: Duration) {
        let hidden = reveal <= 0.;
        if hidden == self.bar_hidden {
            return;
        }
        self.now = now;
        if hidden {
            self.cancel_grab(now);
            self.pointer = None;
            self.evaluate(now);
            self.rev += 1;
            self.hidden_rev = self.rev;
            self.bar_hidden = true;
            self.update_waves(now);
        } else {
            self.bar_hidden = false;
            self.rev += 1;
            self.evaluate(now);
        }
    }

    pub fn bar_hidden(&self) -> bool {
        self.bar_hidden
    }

    /// Width of the host surface/canvas the island is centred in.
    pub fn set_host_width(&mut self, width: f32, now: Duration) {
        if width != self.host_width {
            self.rev += 1;
        }
        self.host_width = width;
        self.evaluate(now);
    }

    // ------------------------------------------------------------ derived state

    pub fn derived(&self) -> Derived {
        let i = &self.inputs;
        let md = &i.media;
        let has_music = md.available && (md.playing || !md.title.is_empty() || !md.artist.is_empty() || !md.art_url.is_empty());
        let agent_active = !i.agents.is_empty();
        let music_active = has_music && !self.manually_hidden;
        let music_primary = music_active && !agent_active;
        let dual = agent_active && music_active;
        let hov = self.app_hover_active && self.island_hovered && !i.calendar_open;
        let agent_expanded = hov && agent_active && self.hover_target == Target::Agent;
        let music_expanded = hov && music_active && self.hover_target == Target::Music;
        let expanded = agent_expanded || music_expanded;
        let pfc = dual && self.keep_primary;
        let n = i.agents.len() as f32;
        let content = n * ROW_HEIGHT + (n - 1.).max(0.) * ROW_SPACING;
        let list = ROW_HEIGHT.max(content.min(AGENT_BUDGET_HEIGHT - AGENT_TOP_RESERVE));
        let agent_expanded_height = 184f32.max(AGENT_TOP_RESERVE + list);
        let agent_compact_width = if dual { DUAL_COLLAPSED_WIDTH } else { AGENT_COMPACT_PILL_WIDTH };
        let primary_collapsed = if dual { AGENT_MINIMAL_WIDTH } else { AGENT_COMPACT_PILL_WIDTH };
        let target_width = if agent_active {
            if agent_expanded { AGENT_EXPANDED_WIDTH } else if music_expanded { EXPANDED_WIDTH } else { agent_compact_width }
        } else if music_expanded {
            EXPANDED_WIDTH
        } else if music_primary {
            COMPACT_MUSIC_WIDTH
        } else {
            self.idle_row_width()
        };
        let main_island_width = if agent_active { if agent_expanded { AGENT_EXPANDED_WIDTH } else { primary_collapsed } } else { target_width };
        let target_height = if agent_expanded { agent_expanded_height } else if music_expanded { EXPANDED_HEIGHT } else { COLLAPSED_HEIGHT };
        let offset = if agent_active && !expanded {
            target_width / 2. - if dual { AGENT_MINIMAL_CLOCK_CENTER_X } else { AGENT_COMPACT_CLOCK_CENTER_X }
        } else {
            0.
        };
        Derived {
            has_music, agent_active, music_active, music_primary, dual, agent_expanded, music_expanded, expanded, pfc,
            target_width, target_height, main_island_width, agent_compact_width, offset,
            agent_list_height: list, agent_content_height: content, agent_expanded_height,
        }
    }

    /// `idleRow.implicitWidth`: (11 + lock + 12) + clock + (10 + power + 13).
    pub fn idle_row_width(&self) -> f32 {
        let (l, c, p) = self.idle_button_widths();
        l + c + p
    }
    pub(crate) fn idle_button_widths(&self) -> (f32, f32, f32) {
        let w = |s: &str| crate::util::measure(s, 16., crate::util::BOLD).width;
        (11. + w("\u{f023}") + 12., w(&self.inputs.now_hh_mm), 10. + w("\u{f011}") + 13.)
    }

    // ------------------------------------------------------------ animated geometry

    pub(crate) fn w(&self) -> f32 { self.a.width.get(self.now) as f32 }
    pub(crate) fn h(&self) -> f32 { self.a.height.get(self.now) as f32 }
    pub(crate) fn off(&self) -> f32 { self.a.offset.get(self.now) as f32 }

    /// Island origin in host coordinates (`centerSection` + island anchors).
    pub fn origin(&self) -> (f32, f32) {
        let off = if self.inputs.calendar_open { 0. } else { self.a.offset.get(self.now) };
        // shell.qml:3886 centerSection: anchors.horizontalCenter + horizontalCenterOffset
        // (Qt 6: qRound(parent/2) − qRound(child/2) + offset, offset not rounded), the
        // same rule as hyprv-ui-bar's layout.rs; the legacy odd-integer hc() put the
        // animating island up to 0.5 px off.
        let half = hyprv_ui::geometry::qt_center_half;
        ((half(self.host_width) as f64 - half(self.w()) as f64 + off) as f32, TOP_MARGIN)
    }

    pub fn geometry(&self) -> Geometry {
        let d = self.derived();
        let (x, y) = self.origin();
        Geometry {
            width: self.w(),
            height: self.h(),
            attached_center_offset: self.off(),
            expanded: d.expanded,
            rect: Rect::new(x, y, self.w(), self.h()),
        }
    }

    /// `islandFrame` rect (island-local), its height and visibility.
    pub(crate) fn frame_rect(&self, d: &Derived) -> Rect {
        let x = if d.pfc { ((self.a.width.get(self.now) / 2. - self.a.offset.get(self.now)) - AGENT_MINIMAL_CLOCK_CENTER_X as f64) as f32 } else { 0. };
        let h = if d.pfc { COLLAPSED_HEIGHT } else { self.h() };
        Rect::new(x, 0., self.a.frame_w.get(self.now) as f32, h)
    }
    pub(crate) fn mef_rect(&self) -> Rect {
        Rect::new(self.a.mef_x.get(self.now) as f32, 0., self.a.mef_w.get(self.now) as f32, self.h())
    }
    /// `expandedMusic` content box (anchors.fill frame, margins 13).
    pub(crate) fn emusic_rect(&self, d: &Derived) -> Rect {
        let f = if d.dual { self.mef_rect() } else { self.frame_rect(d) };
        Rect::new(f.x + 13., f.y + 13., f.width - 26., f.height - 26.)
    }
    pub(crate) fn eagent_rect(&self, d: &Derived) -> Rect {
        let f = self.frame_rect(d);
        Rect::new(f.x + 13., f.y + 13., f.width - 26., f.height - 26.)
    }
    pub(crate) fn cagent_rect(&self, d: &Derived) -> Rect {
        let x = if d.dual && d.expanded { ((self.a.width.get(self.now) / 2. - self.a.offset.get(self.now)) - AGENT_MINIMAL_CLOCK_CENTER_X as f64) as f32 } else { 0. };
        Rect::new(x, 0., d.agent_compact_width, COLLAPSED_HEIGHT)
    }
    pub(crate) fn clock_hotspot(&self, d: &Derived) -> Rect {
        let cx = if d.agent_active {
            if d.dual { AGENT_MINIMAL_CLOCK_CENTER_X } else { AGENT_COMPACT_CLOCK_CENTER_X }
        } else {
            self.w() / 2.
        };
        Rect::new(cx - 29., 0., 58., COLLAPSED_HEIGHT)
    }
    pub(crate) fn emusic_high(&self, d: &Derived) -> bool {
        d.music_expanded || self.keep_primary
    }
    /// Agent list rect inside the expandedAgent box.
    pub(crate) fn list_rect(&self, d: &Derived) -> Rect {
        let b = self.eagent_rect(d);
        Rect::new(0., b.height - d.agent_list_height, b.width, d.agent_list_height)
    }
    pub(crate) fn max_scroll(&self, d: &Derived) -> f32 {
        (d.agent_content_height - d.agent_list_height).max(0.)
    }
    /// Idle row: (lock, clock, power) rects in islandFrame-local coordinates.
    pub(crate) fn idle_rects(&self, d: &Derived) -> [Rect; 3] {
        let f = self.frame_rect(d);
        let (l, c, p) = self.idle_button_widths();
        let x0 = hc(f.width) - hc(l + c + p);
        let y0 = hc(f.height) - hc(COLLAPSED_HEIGHT);
        [Rect::new(x0, y0, l, 38.), Rect::new(x0 + l, y0, c, 38.), Rect::new(x0 + l + c, y0, p, 38.)]
    }
    /// Media buttons (prev, play, next) in expandedMusic-local coordinates.
    pub(crate) fn media_button_rects(&self, d: &Derived) -> [Rect; 3] {
        let b = self.emusic_rect(d);
        let x0 = hc(b.width) - hc(160.);
        let y0 = b.height - 44.;
        [Rect::new(x0, y0 + 2., 40., 40.), Rect::new(x0 + 58., y0, 44., 44.), Rect::new(x0 + 120., y0 + 2., 40., 40.)]
    }

    // ------------------------------------------------------------ evaluation

    fn evaluate(&mut self, now: Duration) {
        self.now = now;
        if self.clock.label != self.inputs.now_hh_mm {
            self.clock.label.clone_from(&self.inputs.now_hh_mm);
        }
        for _ in 0..6 {
            let before = self.snapshot();
            let d = self.derived();
            self.on_changes(&d, now);
            let d = self.derived();
            self.retarget(&d, now);
            self.resolve_hover(now);
            if self.snapshot() == before {
                break;
            }
        }
        let d = self.derived();
        self.prev = Some(d);
        self.prev_calendar_open = self.inputs.calendar_open;
        // Play glyph tracks mediaPlaying.
        let glyph = if self.inputs.media.playing { "\u{f03e4}" } else { "\u{f040a}" };
        if self.play_btn.glyph != glyph {
            self.play_btn.glyph = glyph.into();
        }
        for b in [&mut self.lock, &mut self.clock, &mut self.power] {
            b.update(&self.tok, now);
        }
        for b in [&mut self.prev_btn, &mut self.play_btn, &mut self.next_btn] {
            b.update(&self.tok, now);
        }
        self.update_waves(now);
    }

    /// Fingerprint of the QML state the fixpoint loop settles (allocation-free).
    fn snapshot(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        (self.manually_hidden, self.app_hover_active, self.hover_target, self.keep_primary, self.island_hovered,
         &self.contains, &self.handlers, self.seeking).hash(&mut h);
        h.finish()
    }

    /// Fingerprint of all non-animated state [`Island::build`] reads besides `inputs`.
    fn visual_state(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        self.snapshot().hash(&mut h);
        (self.scroll_y.to_bits(), self.seek_preview_s.to_bits(), self.waves[0].phase.to_bits(), self.waves[1].phase.to_bits(),
         self.grab, self.buttons_down).hash(&mut h);
        h.finish()
    }

    /// Changes whenever [`Island::build`] may produce a different tree; `None` while
    /// anything animates (rebuild every frame). Hosts rebuild/reconcile only when this
    /// differs from the value seen at their last build, so idle frames cost nothing.
    pub fn visual_rev(&self) -> Option<u64> {
        if self.bar_hidden {
            return Some(self.hidden_rev);
        }
        (!self.animating(self.now)).then_some(self.rev)
    }

    /// `onHasMusicContentChanged`, `onCalendarOpenChanged`, `onMusicExpandedChanged`.
    fn on_changes(&mut self, d: &Derived, now: Duration) {
        if !d.has_music {
            self.manually_hidden = false;
        }
        if self.inputs.calendar_open && !self.prev_calendar_open {
            self.app_hover_active = false;
            self.hover_target = Target::None;
            self.prev_calendar_open = true;
        }
        let prev_me = self.prev.map_or(d.music_expanded, |p| p.music_expanded);
        if prev_me != d.music_expanded {
            if let Some(p) = &mut self.prev {
                p.music_expanded = d.music_expanded;
            }
            if !d.dual {
                self.keep_primary = false;
                self.music_frame_release = None;
            } else {
                self.keep_primary = true;
                self.music_frame_release = if d.music_expanded { None } else { Some(now + MUSIC_FRAME_RELEASE) };
            }
        }
    }

    fn retarget(&mut self, d: &Derived, now: Duration) {
        let a = &mut self.a;
        a.width.set(d.target_width as f64, now);
        a.height.set(d.target_height as f64, now);
        a.offset.set(d.offset as f64, now);
        a.frame_w.set(d.main_island_width as f64, now);
        let h = a.height.get(now);
        a.frame_r.set(if d.pfc { COLLAPSED_HEIGHT as f64 / 2. } else if d.expanded { 30. } else { 24. }, now);
        let dme = d.dual && d.music_expanded;
        a.mef_x.set(if dme { 0. } else { DUAL_MUSIC_LEFT_X as f64 }, now);
        a.mef_w.set(if dme { EXPANDED_WIDTH as f64 } else { MUSIC_DETACHED_WIDTH as f64 }, now);
        let radius = if d.music_expanded { 30. } else { h / 2. };
        // Qt ticks the pre-existing radius job once before the first animated
        // height binding change; later changes restart from its presented value.
        if !d.music_expanded && a.height.had_presented_tick(self.presented_at) {
            a.mef_r.set_from_presented(radius, now, self.presented_at);
        } else {
            a.mef_r.set(radius, now);
        }
        a.mef_op.set(if dme { 1. } else { 0. }, now);
        a.idle_op.set(if d.music_primary || d.agent_active { 0. } else { 1. }, now);
        a.cmusic_op.set(if d.music_primary && !d.expanded { 1. } else { 0. }, now);
        a.cagent_op.set(if d.agent_active && !d.expanded { 1. } else { 0. }, now);
        a.emusic_op.set(if d.music_expanded { 1. } else { 0. }, now);
        a.eagent_op.set(if d.agent_expanded { 1. } else { 0. }, now);
        a.scroll_op.set(if d.agent_content_height > d.agent_list_height { 0.34 } else { 0. }, now);
        // Progress fill: width follows the (animating) track width; disabled while seeking.
        let track_w = self.emusic_rect(d).width;
        let md = &self.inputs.media;
        let shown = if self.seeking { self.seek_preview_s } else { md.position_s };
        let ratio = if md.length_s > 0. { (shown / md.length_s).clamp(0., 1.) } else { 0. };
        self.a.fill_w.enabled = !self.seeking;
        self.a.fill_w.set_from_presented((5f64).max(track_w as f64 * ratio), now, self.presented_at);
        self.list_flick.set_extents(d.agent_list_height as f64, d.agent_content_height as f64);
        if self.inputs.calendar_progress >= 0.99 || self.a.eagent_op.get(now) <= 0.01 {
            self.list_flick.reset(self.scroll_y as f64);
        }
        self.scroll_y = self.list_flick.content_y() as f32;
    }

    // ------------------------------------------------------------ hover delivery

    fn local(&self, p: (f32, f32)) -> (f32, f32) {
        let (ox, oy) = self.origin();
        (p.0 - ox, p.1 - oy)
    }

    /// Items in Qt hover/press traversal order (topmost first) with their MouseAreas
    /// (topmost first) and passive HoverHandler.
    fn traversal(&self, d: &Derived) -> Vec<Layer> {
        let mut out = Vec::new();
        let hotspots = !d.expanded && !self.inputs.calendar_open;
        if hotspots {
            let c = self.clock_hotspot(d);
            out.push((None, vec![(Area::Clock, c)]));
            let right_x = c.x + c.width;
            out.push((None, vec![
                (Area::Left, Rect::new(0., 0., c.x.max(0.), COLLAPSED_HEIGHT)),
                (Area::Right, Rect::new(right_x, 0., (self.w() - right_x).max(0.), COLLAPSED_HEIGHT)),
            ]));
        }
        let emusic_visible = self.a.emusic_op.get(self.now) > 0.01;
        let emusic = |s: &Self| {
            let b = s.emusic_rect(d);
            let mut areas = Vec::new();
            if emusic_visible {
                if s.inputs.media.length_s > 0. {
                    areas.push((Area::Seek, Rect::new(b.x - 8., b.y + 78., b.width + 16., 21.)));
                }
                let r = s.media_button_rects(d);
                for (area, rect) in [(Area::Next, r[2]), (Area::Play, r[1]), (Area::Prev, r[0])] {
                    areas.push((area, Rect::new(b.x + rect.x, b.y + rect.y, rect.width, rect.height)));
                }
            }
            (Some((Handler::ExpandedMusic, b, d.music_expanded && emusic_visible)), areas)
        };
        if self.emusic_high(d) {
            out.push(emusic(self));
        }
        // z = 0 children, reverse declaration order.
        let eagent_visible = self.a.eagent_op.get(self.now) > 0.01;
        let eb = self.eagent_rect(d);
        let mut rows = Vec::new();
        if eagent_visible {
            let l = self.list_rect(d);
            for i in 0..self.inputs.agents.len() {
                let y = l.y + i as f32 * (ROW_HEIGHT + ROW_SPACING) - self.scroll_y;
                let r = Rect::new(eb.x, eb.y + y, l.width, ROW_HEIGHT);
                // Clipped by the ListView.
                let top = r.y.max(eb.y + l.y);
                let bottom = (r.y + r.height).min(eb.y + l.y + l.height);
                if bottom > top {
                    rows.push((Area::Row(i), Rect::new(r.x, top, r.width, bottom - top)));
                }
            }
        }
        out.push((Some((Handler::ExpandedAgent, eb, d.agent_expanded && eagent_visible)), rows));
        let cagent_on = self.a.cagent_op.get(self.now) > 0.01 && d.agent_active && !d.expanded;
        let cb = self.cagent_rect(d);
        let mut areas = Vec::new();
        if cagent_on {
            if d.dual {
                areas.push((Area::Resolver, cb));
                let px = cb.x + AGENT_MINIMAL_WIDTH + AGENT_DETACHED_GAP;
                areas.push((Area::MusicPill, Rect::new(px, 0., MUSIC_DETACHED_WIDTH, COLLAPSED_HEIGHT)));
            }
            let pw = if d.dual { AGENT_MINIMAL_WIDTH } else { AGENT_COMPACT_PILL_WIDTH };
            areas.push((Area::AgentPill, Rect::new(cb.x, 0., pw, COLLAPSED_HEIGHT)));
        }
        out.push((Some((Handler::CompactAgent, cb, cagent_on && !d.dual)), areas));
        if !self.emusic_high(d) {
            out.push(emusic(self));
        }
        let f = self.frame_rect(d);
        let cmusic_visible = self.a.cmusic_op.get(self.now) > 0.01;
        out.push((Some((Handler::CompactMusic, f, cmusic_visible && d.music_primary && !d.expanded)), Vec::new()));
        if self.a.idle_op.get(self.now) > 0.01 {
            let r = self.idle_rects(d);
            let shift = |r: Rect| Rect::new(f.x + r.x, f.y + r.y, r.width, r.height);
            out.push((None, vec![(Area::Power, shift(r[2])), (Area::Lock, shift(r[0]))]));
        }
        out
    }

    fn hit(&self, d: &Derived, x: f32, y: f32, hover: bool) -> (Option<Area>, Vec<Handler>) {
        let mut handlers = Vec::new();
        for (handler, areas) in self.traversal(d) {
            let found = areas.into_iter().find(|(a, r)| {
                let blocked_hotspot = hover && !QUIRK_HOTSPOTS_BLOCK_BUTTON_HOVER && matches!(a, Area::Left | Area::Right);
                !blocked_hotspot && (!hover || a.hover_enabled()) && (hover || a.accepts_press()) && contains(*r, x, y)
            });
            if let Some((h, r, enabled)) = handler {
                if enabled && contains(r, x, y) {
                    handlers.push(h);
                }
            }
            if let Some((a, _)) = found {
                return (Some(a), handlers);
            }
        }
        (None, handlers)
    }

    fn area_rect(&self, d: &Derived, area: Area) -> Option<Rect> {
        self.traversal(d).into_iter().flat_map(|(_, a)| a).find(|(a, _)| *a == area).map(|(_, r)| r)
    }

    /// Frame-synchronous hover (Qt re-delivers hover every frame under a still pointer).
    fn resolve_hover(&mut self, now: Duration) {
        let d = self.derived();
        if self.grab.is_some() || self.buttons_down {
            // Qt Quick delivers no hover while a button is held (verified by the
            // `music-swipe-hide` reference: dragging off the clock keeps the island compact);
            // a Leave during the press is applied on release.
            return;
        }
        let Some(p) = self.pointer else {
            self.set_hover(false, Vec::new(), Vec::new(), None, now);
            return;
        };
        let (x, y) = self.local(p);
        let in_strip = !self.clip_hover_to_input_region || p.1 < BAR_STRIP || contains(self.geometry().rect, p.0, p.1);
        let m = HOVER_BRIDGE_MARGIN;
        let island = in_strip && x >= -m && y >= -m && x < self.w() + m && y < self.h() + m;
        let (top, handlers) = if in_strip { self.hit(&d, x, y, true) } else { (None, Vec::new()) };
        self.set_hover(island, top.into_iter().collect(), handlers, Some((x, y)), now);
    }

    fn set_hover(&mut self, island: bool, contains: Vec<Area>, handlers: Vec<Handler>, local: Option<(f32, f32)>, now: Duration) {
        if self.island_hovered && !island {
            self.app_hover_active = false;
            self.hover_target = Target::None;
        }
        self.island_hovered = island;
        let d = self.derived();
        // MouseArea enter / exit (only hover-enabled areas; one at a time).
        let entered: Vec<Area> = contains.iter().filter(|a| !self.contains.contains(a)).copied().collect();
        let left: Vec<Area> = self.contains.iter().filter(|a| !contains.contains(a)).copied().collect();
        let (lx, ly) = local.unwrap_or((0., 0.));
        for a in &left {
            self.button_event(*a, PointerEvent::Leave, now);
        }
        for a in &entered {
            match a {
                Area::Clock => {
                    self.app_hover_active = false;
                    self.hover_target = Target::None;
                }
                Area::Left => {
                    self.app_hover_active = true;
                    self.hover_target = if d.dual {
                        dual_target(lx)
                    } else if d.agent_active {
                        Target::Agent
                    } else if d.music_active {
                        Target::Music
                    } else {
                        Target::None
                    };
                }
                Area::Right => {
                    self.app_hover_active = true;
                    self.hover_target = if d.dual || (!d.agent_active && d.music_active) {
                        Target::Music
                    } else if d.agent_active {
                        Target::Agent
                    } else {
                        Target::None
                    };
                }
                Area::AgentPill => self.hover_target = Target::Agent,
                Area::MusicPill => self.hover_target = Target::Music,
                _ => {}
            }
        }
        // dualHoverResolver: onEntered + onPositionChanged.
        if contains.contains(&Area::Resolver) {
            let x = lx - self.cagent_rect(&d).x;
            self.hover_target = dual_target(x);
        }
        for a in &contains {
            if let Some(r) = self.area_rect(&d, *a) {
                let ev = if entered.contains(a) { PointerEvent::Enter { x: lx - r.x, y: ly - r.y } } else { PointerEvent::Move { x: lx - r.x, y: ly - r.y } };
                self.button_event(*a, ev, now);
            }
        }
        for h in &handlers {
            if !self.handlers.contains(h) {
                self.hover_target = match h {
                    Handler::CompactMusic | Handler::ExpandedMusic => Target::Music,
                    Handler::CompactAgent | Handler::ExpandedAgent => Target::Agent,
                };
            }
        }
        self.contains = contains;
        self.handlers = handlers;
    }

    fn button_event(&mut self, area: Area, ev: PointerEvent, now: Duration) {
        match area {
            Area::Lock => {
                self.lock.pointer(ev, now);
            }
            Area::Power => {
                self.power.pointer(ev, now);
            }
            Area::Prev => {
                self.prev_btn.pointer(ev, now);
            }
            Area::Play => {
                self.play_btn.pointer(ev, now);
            }
            Area::Next => {
                self.next_btn.pointer(ev, now);
            }
            _ => {}
        }
        let button=match area {Area::Prev=>Some((0,&self.prev_btn)),Area::Play=>Some((1,&self.play_btn)),Area::Next=>Some((2,&self.next_btn)),_=>None};
        if let Some((index,button))=button {
            let target=if button.state.pressed.is_some(){0.92}else if button.state.hovered{1.06}else{1.};
            self.media_scales[index].request(target,now,self.during_frame);
        }
    }

    // ------------------------------------------------------------ pointer input

    pub fn pointer(&mut self, ev: Pointer, now: Duration) -> PointerOutcome {
        if self.bar_hidden {
            return PointerOutcome::default();
        }
        for scale in &mut self.media_scales {scale.begin_frame(now);}
        self.now = now;
        self.rev += 1;
        self.advance_timers(now);
        let d = self.derived();
        let mut out = PointerOutcome::default();
        match ev {
            Pointer::Motion { x, y } => {
                self.pointer = Some((x, y));
                if self.list_flick.drag_to(y as f64, None, now) {
                    if let Some(g) = self.grab.take() { self.cancel_area(g, now); }
                    self.pressed_row = None;
                    self.drag = None;
                }
                self.scroll_y = self.list_flick.content_y() as f32;
                let x_enabled = self.swipe_enabled(&d);
                if let Some(drag) = &mut self.drag {
                    drag.last = (x, y);
                    let dx = drag.last.0 - drag.press.0;
                    if !drag.active && x_enabled && dx.abs() > DRAG_THRESHOLD {
                        drag.active = true;
                        drag.tap_compact = false;
                        drag.tap_expanded = false;
                        // DragHandler takes over from the MouseArea (it is canceled).
                        if let Some(g) = self.grab.take() {
                            self.cancel_area(g, now);
                        }
                    }
                }
                if let Some(Area::Seek) = self.grab {
                    let (lx, _) = self.local((x, y));
                    let b = self.emusic_rect(&d);
                    self.seek_preview_s = self.seconds_for_x(lx - b.x);
                } else if let Some(g) = self.grab {
                    if let Some(r) = self.area_rect(&d, g) {
                        let (lx, ly) = self.local((x, y));
                        self.button_event(g, PointerEvent::Move { x: lx - r.x, y: ly - r.y }, now);
                    }
                }
            }
            Pointer::Leave => {
                self.pointer = None;
            }
            Pointer::Cancel => {
                self.cancel_grab(now);
                self.pointer = None;
            }
            Pointer::Press { x, y } => {
                self.pointer = Some((x, y));
                self.resolve_hover(now);
                self.evaluate(now);
                let d = self.derived();
                let (lx, ly) = self.local((x, y));
                let (area, _) = self.hit(&d, lx, ly, false);
                self.buttons_down = true;
                let agent_box = self.eagent_rect(&d);
                let list = self.list_rect(&d);
                if d.agent_expanded && self.max_scroll(&d) > 0. && contains(Rect::new(agent_box.x + list.x, agent_box.y + list.y, list.width, list.height), lx, ly) {
                    self.list_flick.press(y as f64, now);
                }
                let fr = self.frame_rect(&d);
                let eb = self.emusic_rect(&d);
                let top_row = Rect::new(eb.x, eb.y, eb.width, 72.);
                self.drag = Some(Drag {
                    press: (x, y),
                    last: (x, y),
                    active: false,
                    tap_compact: area.is_none() && d.music_primary && !d.expanded && !self.seeking && contains(fr, lx, ly),
                    tap_expanded: area.is_none() && d.music_expanded && !self.seeking && contains(top_row, lx, ly),
                });
                if let Some(a) = area {
                    self.grab = Some(a);
                    self.pressed_row = match a {
                        Area::Row(i) => self.inputs.agents.get(i).map(|s| s.id.clone()),
                        _ => None,
                    };
                    if let Some(r) = self.area_rect(&d, a) {
                        self.button_event(a, PointerEvent::Press { x: lx - r.x, y: ly - r.y, button: Button::Left }, now);
                    }
                    if a == Area::Seek {
                        self.seek_preview_reset = None;
                        self.seeking = true;
                        self.seek_preview_s = self.seconds_for_x(lx - eb.x);
                    }
                }
            }
            Pointer::Release { x, y } => {
                self.pointer = Some((x, y));
                self.buttons_down = false;
                if self.list_flick.release(y as f64, now) {
                    if let Some(g) = self.grab.take() { self.cancel_area(g, now); }
                    self.pressed_row = None;
                    self.drag = None;
                }
                self.scroll_y = self.list_flick.content_y() as f32;
                let (lx, ly) = self.local((x, y));
                if let Some(a) = self.grab.take() {
                    let inside = self.area_rect(&d, a).is_some_and(|r| contains(r, lx, ly));
                    if let Some(r) = self.area_rect(&d, a) {
                        self.button_event(a, PointerEvent::Release { x: lx - r.x, y: ly - r.y, button: Button::Left }, now);
                    }
                    if a == Area::Seek {
                        let b = self.emusic_rect(&d);
                        let target = self.seconds_for_x(lx - b.x);
                        self.seek_preview_s = target;
                        self.pending.push(IslandAction::Seek(target));
                        self.seek_preview_reset = Some(now + SEEK_PREVIEW_RESET);
                    } else if let Area::Row(_) = a {
                        // Bound to the session pressed, never to whatever now sits at the index.
                        if let Some(id) = self.pressed_row.take().filter(|_| inside) {
                            self.pending.push(IslandAction::FocusAgent(id));
                        }
                    } else if inside {
                        if let Some(action) = self.click_action(a) {
                            self.pending.push(action);
                        }
                    }
                }
                if let Some(drag) = self.drag.take() {
                    if drag.active {
                        self.swipe_released(&d, drag);
                    } else if drag.tap_compact && contains(self.frame_rect(&d), lx, ly) {
                        self.pending.push(IslandAction::FocusMediaApp);
                    } else if drag.tap_expanded {
                        let eb = self.emusic_rect(&d);
                        if contains(Rect::new(eb.x, eb.y, eb.width, 72.), lx, ly) {
                            self.pending.push(IslandAction::FocusMediaApp);
                        }
                    }
                }
            }
            Pointer::Wheel { x, y, angle_y } => {
                self.pointer = Some((x, y));
                let (lx, ly) = self.local((x, y));
                let eb = self.eagent_rect(&d);
                let l = self.list_rect(&d);
                let list = Rect::new(eb.x + l.x, eb.y + l.y, l.width, l.height);
                if d.agent_expanded && d.agent_content_height > d.agent_list_height && contains(list, lx, ly) {
                    self.list_flick.wheel(Wheel::deltas(angle_y as f64, 0.), now);
                    self.scroll_y = self.list_flick.content_y() as f32;
                }
            }
        }
        self.evaluate(now);
        out.actions = std::mem::take(&mut self.pending);
        out.cursor = self.cursor();
        out
    }

    /// Native wheel adapter retaining pixel deltas and begin/update/momentum/end.
    pub fn wheel(&mut self, x: f32, y: f32, event: Wheel, now: Duration) -> PointerOutcome {
        if self.bar_hidden {
            return PointerOutcome::default();
        }
        self.now = now;
        self.rev += 1;
        self.pointer = Some((x, y));
        let d = self.derived();
        let (lx, ly) = self.local((x, y));
        let b = self.eagent_rect(&d);
        let l = self.list_rect(&d);
        if event.phase == hyprv_widgets::flickable::ScrollPhase::End || (d.agent_expanded && self.max_scroll(&d) > 0. && contains(Rect::new(b.x+l.x,b.y+l.y,l.width,l.height),lx,ly)) {
            self.list_flick.wheel(event, now);
            self.scroll_y = self.list_flick.content_y() as f32;
        }
        self.evaluate(now);
        PointerOutcome { actions: Vec::new(), cursor: self.cursor() }
    }

    fn cancel_area(&mut self, area: Area, now: Duration) {
        self.button_event(area, PointerEvent::Cancel, now);
        if area == Area::Seek {
            self.seek_preview_reset = Some(now + SEEK_PREVIEW_RESET);
        }
    }

    fn swipe_enabled(&self, d: &Derived) -> bool {
        !self.inputs.calendar_open && d.music_active && !self.seeking
    }

    fn swipe_released(&mut self, d: &Derived, drag: Drag) {
        if !d.has_music {
            return;
        }
        let dx = if QUIRK_SWIPE_TRANSLATION_RESET { 0. } else { drag.last.0 - drag.press.0 };
        let dy = 0.; // yAxis disabled
        if dx.abs() < SWIPE_THRESHOLD || dx.abs() < dy * 1.4 {
            return;
        }
        if dx > 0. {
            self.manually_hidden = true;
        } else if !QUIRK_SWIPE_CANNOT_RESTORE || d.music_active {
            self.manually_hidden = false;
        }
    }

    fn click_action(&self, area: Area) -> Option<IslandAction> {
        Some(match area {
            Area::Clock => IslandAction::ToggleCalendar,
            Area::Lock => IslandAction::Lock,
            Area::Power => IslandAction::Power,
            Area::Prev => IslandAction::Previous,
            Area::Play => IslandAction::PlayPause,
            Area::Next => IslandAction::Next,
            Area::MusicPill => IslandAction::FocusMediaApp,
            Area::AgentPill => IslandAction::FocusAgent(self.inputs.agents.first().map(|a| a.id.clone()).unwrap_or_default()),
            _ => return None,
        })
    }

    fn cursor(&self) -> CursorShape {
        let area = self.grab.or_else(|| self.contains.first().copied());
        match area {
            Some(Area::Clock | Area::Left | Area::Right) | None => CursorShape::Default,
            Some(Area::Seek) if self.inputs.media.length_s <= 0. => CursorShape::Default,
            _ => CursorShape::Pointer,
        }
    }

    /// `progressTrack.secondsForX` (track-local x).
    fn seconds_for_x(&self, x: f32) -> f64 {
        let d = self.derived();
        let width = self.emusic_rect(&d).width;
        let len = self.inputs.media.length_s;
        if len <= 0. || width <= 0. {
            return 0.;
        }
        let ratio = (x / width).clamp(0., 1.) as f64;
        (ratio * len).clamp(0., len)
    }

    // ------------------------------------------------------------ time

    fn advance_timers(&mut self, now: Duration) {
        if self.music_frame_release.is_some_and(|t| now >= t) {
            self.music_frame_release = None;
            self.keep_primary = false;
        }
        if self.seek_preview_reset.is_some_and(|t| now >= t) {
            self.seek_preview_reset = None;
            self.seeking = false;
        }
        for w in &mut self.waves {
            while let Some(t) = w.next.filter(|t| now >= *t) {
                w.phase += 0.28;
                w.next = Some(t + WAVE_TICK);
            }
        }
    }

    /// Advance to `now` (one presented frame). Returns true while more frames are needed.
    pub fn frame(&mut self, now: Duration) -> bool {
        let before = self.visual_state();
        for scale in &mut self.media_scales {scale.begin_frame(now);}
        self.during_frame=true;
        self.now = now;
        self.advance_timers(now);
        self.list_flick.frame(now);
        self.scroll_y = self.list_flick.content_y() as f32;
        if !self.buttons_down {
            self.resolve_hover(now);
        }
        self.evaluate(now);
        if self.visual_state() != before {
            self.rev += 1;
        }
        self.presented_at = now;
        self.during_frame=false;
        self.animating(now)
    }

    /// Which AudioSpectrum instances run their 66 ms idle wave (§7): visible and
    /// (paused or no spectrum energy).
    pub(crate) fn wave_wanted(&self) -> [bool; 2] {
        let active = self.inputs.media.playing;
        let energy = self.inputs.spectrum.iter().any(|v| *v > 0.);
        let shown = self.inputs.calendar_progress < 0.99 && !self.bar_hidden;
        let idle = !active || !energy;
        [
            shown && idle && self.a.cmusic_op.get(self.now) > 0.01,
            shown && idle && self.a.emusic_op.get(self.now) > 0.01,
        ]
    }
    fn update_waves(&mut self, now: Duration) {
        let wanted = self.wave_wanted();
        for (w, on) in self.waves.iter_mut().zip(wanted) {
            match (on, w.next) {
                (true, None) => w.next = Some(now + WAVE_TICK),
                (false, Some(_)) => w.next = None,
                _ => {}
            }
        }
    }

    pub fn animating(&self, now: Duration) -> bool {
        !self.bar_hidden && (self.a.all().iter().any(|a| a.running(now))
            || self.lock.animating(now)
            || self.power.animating(now)
            || self.media_scales.iter().any(|s|s.running(now))
            || self.list_flick.animating())
    }

    /// Next one-shot deadline (timers and the idle wave) for the host's frame scheduler.
    pub fn next_deadline(&self) -> Option<Duration> {
        [self.music_frame_release, self.seek_preview_reset, self.waves[0].next, self.waves[1].next]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn primary_agent(&self) -> Option<&Agent> {
        self.inputs.agents.first()
    }

    pub fn hover_state(&self) -> (bool, Target, bool) {
        (self.app_hover_active, self.hover_target, self.island_hovered)
    }

    pub(crate) fn displayed_position(&self) -> f64 {
        if self.seeking { self.seek_preview_s } else { self.inputs.media.position_s }
    }
}

/// `dualCompactHoverTarget(localX)`.
fn dual_target(x: f32) -> Target {
    if !x.is_finite() {
        Target::None
    } else if x >= AGENT_MINIMAL_WIDTH {
        Target::Music
    } else {
        Target::Agent
    }
}


#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod laptop_tests;
#[cfg(test)]
mod visual_rev_tests;

#[cfg(test)]
mod exactness_tests;
