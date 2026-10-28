//! Surface/lifecycle backbone. Composite contents are attached through Component slots.
use hyprv_core::{
    ipc::{Request, Response as IpcResponse},
    store::StateStore,
};
use hyprv_ui::{
    animation::{Behavior, Color, CubicBezier, Direction, Easing, Family},
    geometry::Rect,
    scene::{Border, Content, Node, NodeId, Scene},
    skia::Matrix,
    wayland::{
        Anchor, Engine, FrameInfo, InputEvent, KeySym, KeyboardInteractivity, Layer, LayerConfig,
        MonitorInfo, PointerKind, Response, SurfaceId, UiHandler, WlOutput,
    },
};
use hyprv_widgets::{
    containers::{
        glass_panel, popup_position, GlassStyle, PopupPlacement, Reveal, RevealTiming, Transition,
    },
    foundations::{hcenter, with_alpha, Palette},
    primitives::{rounded_rect, FontSpec, Label},
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
const MS210: Duration = Duration::from_millis(210);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Slot {
    Bar,
    Island,
    Calendar,
    ControlPanel,
    Wifi,
    Stats,
    QuickAdjust,
    TrayMenu,
    TrayOverflow,
    Battery,
}
impl Slot {
    pub fn namespace(self) -> &'static str {
        match self {
            Self::Bar | Self::Island | Self::Calendar => "hyprv-quickshell",
            Self::ControlPanel => "shell:hyprv-control-panel",
            Self::Wifi => "shell:hyprv-wifi",
            Self::Stats => "shell:hyprv-system-stats",
            Self::QuickAdjust => "shell:hyprv-quick-adjust",
            Self::TrayMenu => "shell:hyprv-tray-menu",
            Self::TrayOverflow => "shell:hyprv-tray-overflow",
            Self::Battery => "shell:hyprv-battery-info",
        }
    }
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::ControlPanel => (278., 422.),
            Self::Wifi => (384., 400.),
            Self::Stats => (548., 708.),
            Self::QuickAdjust => (278., 62.),
            Self::TrayMenu => (300., 100.),
            Self::TrayOverflow => (134., 54.),
            Self::Battery => (324., 200.),
            _ => (0., 0.),
        }
    }
    pub fn swallows_card(self) -> bool {
        !matches!(self, Self::TrayMenu | Self::TrayOverflow)
    }
}
#[derive(Clone, Debug)]
pub enum HostAction {
    Open {
        slot: Slot,
        output: String,
        anchor: Option<Rect>,
    },
    Close(Slot),
    CloseOn {
        slot: Slot,
        output: String,
    },
    Calendar {
        output: String,
    },
    CloseCalendar {
        force: bool,
    },
    CalendarChild {
        opening: bool,
        active: bool,
    },
    Island {
        output: String,
        expanded: bool,
        bounds: Rect,
        offset: f32,
    },
    Fullscreen {
        output: String,
        value: bool,
    },
    PopupSize {
        slot: Slot,
        width: f32,
        height: f32,
    },
    PopupSizeOn {
        slot: Slot,
        output: String,
        width: f32,
        height: f32,
    },
    TraySubmenuDepth(usize),
    QuickValue(u8),
}
pub struct ViewContext<'a> {
    pub slot: Slot,
    pub output: &'a str,
    pub bounds: Rect,
    pub palette: &'a Palette,
    pub minute: i64,
    pub store: &'a StateStore,
    pub data: &'a BTreeMap<String, Value>,
    pub quick_mode: &'a str,
    pub brightness: Option<u8>,
    pub volume: Option<u8>,
}
/// Components retain their own control state; the host owns surfaces and focus/masks.
pub trait Component {
    fn build(&mut self, context: &ViewContext<'_>) -> Node;
    fn input(&mut self, _context: &ViewContext<'_>, _event: &InputEvent) -> Vec<HostAction> {
        Vec::new()
    }
}
#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub width: f32,
    pub height: f32,
}
struct Bar {
    output: Output,
    fullscreen: bool,
    expanded: bool,
    reveal: Behavior<f64>,
    hide: Behavior<f64>,
    island_expanded: bool,
    island_hold: Option<Duration>,
    island_bounds: Rect,
    island_offset: f32,
    calendar_visible: bool,
    calendar_presented: bool,
    calendar_progress: Behavior<f64>,
    calendar_width: Behavior<f64>,
    calendar_height: Behavior<f64>,
    calendar_radius: Behavior<f64>,
    calendar_close: Option<Duration>,
    calendar_arm: Option<Duration>,
    calendar_leave: Option<Duration>,
    calendar_inside: bool,
}
impl Bar {
    fn new(output: Output) -> Self {
        Self {
            output,
            fullscreen: false,
            expanded: !crate::laptop::active(),
            reveal: Behavior::new(if crate::laptop::active() { 0. } else { 1. }, Duration::from_millis(700), bar_curve()),
            hide: Behavior::new(0., Duration::from_millis(700), bar_curve()),
            island_expanded: false,
            island_hold: None,
            island_bounds: Rect::new(0., 0., 78., 38.),
            island_offset: 0.,
            calendar_visible: false,
            calendar_presented: false,
            calendar_progress: Behavior::new(0., MS210, Easing::new(Family::Cubic, Direction::Out)),
            calendar_width: Behavior::new(78., MS210, Easing::new(Family::Cubic, Direction::Out)),
            calendar_height: Behavior::new(38., MS210, Easing::new(Family::Cubic, Direction::Out)),
            calendar_radius: Behavior::new(24., MS210, Easing::new(Family::Cubic, Direction::Out)),
            calendar_close: None,
            calendar_arm: None,
            calendar_leave: None,
            calendar_inside: false,
        }
    }
    fn height(&self) -> f32 {
        self.output.height.min(
            if self.calendar_presented || self.island_expanded || self.island_hold.is_some() {
                820.
            } else {
                58.
            },
        )
    }
    fn center(&self, now: Duration) -> Rect {
        let (w, h) = if self.calendar_presented {
            (
                self.calendar_width.value(now) as f32,
                self.calendar_height.value(now) as f32,
            )
        } else {
            (self.island_bounds.width, self.island_bounds.height)
        };
        Rect::new(
            hcenter(self.output.width) - hcenter(w)
                + if self.calendar_presented {
                    0.
                } else {
                    self.island_offset
                },
            10.,
            w,
            h,
        )
    }
    fn y(&self, now: Duration) -> f32 {
        -(self.center(now).height + 10.).max(68.) * (1. - self.reveal.value(now) + self.hide.value(now)*self.reveal.value(now)) as f32
    }
    fn running(&self) -> bool {
        self.hide.is_running() || self.reveal.is_running()
            || self.calendar_progress.is_running()
            || self.calendar_width.is_running()
            || self.calendar_height.is_running()
            || self.calendar_radius.is_running()
    }
    fn calendar(&mut self, open: bool, now: Duration) {
        self.calendar_visible = open;
        self.calendar_leave = None;
        if open {
            self.calendar_presented = true;
            self.calendar_arm = Some(now + Duration::from_millis(260));
            self.calendar_close = None;
            self.calendar_progress.retarget(1., now);
            self.calendar_width
                .retarget(self.output.width.min(568.) as f64 - 48., now);
            self.calendar_height
                .retarget((self.height() - 78.).min(500.) as f64, now);
            self.calendar_radius.retarget(30., now);
        } else {
            self.calendar_arm = None;
            self.calendar_progress.retarget(0., now);
            self.calendar_radius.retarget(24., now);
            self.calendar_close = Some(now + MS210);
        }
    }
}
fn bar_curve() -> Easing {
    Easing::BezierSpline(vec![CubicBezier {
        control1: (0.1, 0.9),
        control2: (0.1, 1.05),
        end: (1., 1.),
    }])
}
struct Popup {
    slot: Slot,
    output: String,
    anchor: Option<Rect>,
    size: (f32, f32),
    requested: bool,
    mapped: bool,
    closing: bool,
    open_at: Option<Duration>,
    reveal: Reveal,
    quick_opacity: Behavior<f64>,
    quick_scale: Behavior<f64>,
    hide_at: Option<Duration>,
    unmap_at: Option<Duration>,
}
impl Popup {
    fn new(slot: Slot, output: String, anchor: Option<Rect>, now: Duration) -> Self {
        let size = slot.size();
        let mut reveal = Reveal::new(size.1);
        reveal.timing.open_pause = 20;
        reveal.timing.close_pause = 30;
        if slot == Slot::Wifi {
            reveal.timing.open_pause = 85;
            reveal.timing.open_reveal = 280;
            reveal.timing.content_delay = 60;
            reveal.timing.open_fade = 180;
            reveal.timing.open_slide = 220;
            reveal.timing.open_offset = -10.;
            reveal.timing.close_pause = 35;
            reveal.timing.close_reveal = 210;
            reveal.timing.close_fade = 110;
            reveal.timing.close_slide = 170;
        }
        if slot == Slot::TrayMenu {
            reveal.timing = RevealTiming {
                close_reveal: 200,
                ..Default::default()
            };
        }
        reveal.prepare_open();
        let mut q = Self {
            slot,
            output,
            anchor,
            size,
            requested: true,
            mapped: true,
            closing: false,
            open_at: Some(now + Duration::from_millis(16)),
            reveal,
            quick_opacity: Behavior::new(
                0.,
                Duration::from_millis(120),
                Easing::new(Family::Quad, Direction::Out),
            ),
            quick_scale: Behavior::new(
                0.96,
                Duration::from_millis(160),
                Easing::new(Family::Cubic, Direction::Out),
            ),
            hide_at: None,
            unmap_at: None,
        };
        if slot == Slot::QuickAdjust {
            q.quick_opacity.retarget(1., now);
            q.quick_scale.retarget(1., now);
            q.hide_at = Some(now + Duration::from_secs(3));
            q.open_at = None;
        }
        if slot == Slot::TrayOverflow {
            q.open_at = None;
        }
        q
    }
    fn close(&mut self, now: Duration) {
        if self.closing {
            return;
        }
        self.requested = false;
        self.closing = true;
        self.open_at = None;
        if self.slot == Slot::TrayOverflow {
            self.mapped = false;
        } else if self.slot == Slot::QuickAdjust {
            self.hide_at = None;
            self.quick_opacity.retarget(0., now);
            self.quick_scale.retarget(0.96, now);
            self.unmap_at = Some(now + Duration::from_millis(140));
        } else {
            self.reveal.play_close(now);
        }
    }
    fn position(&self, bar: &Bar, now: Duration) -> Rect {
        let anchor = self
            .anchor
            .unwrap_or(Rect::new(bar.output.width - 10., 0., 0., 58.));
        let output = (bar.output.width, bar.output.height);
        let place = match self.slot {
            Slot::ControlPanel | Slot::Wifi => PopupPlacement::RightAligned { margin: 10. },
            Slot::Stats => PopupPlacement::FixedLeft { x: 10. },
            Slot::QuickAdjust => PopupPlacement::AnchorRight { margin: 10. },
            _ => PopupPlacement::AnchorCentered,
        };
        if self.slot == Slot::QuickAdjust {
            let mut r = popup_position(output, self.size, anchor, place, 0.);
            r.y = 58.;
            r
        } else if self.slot == Slot::ControlPanel {
            let mut r = popup_position(output, self.size, anchor, place, 10.);
            r.y = anchor.y + anchor.height + 10.;
            r
        } else {
            let _ = now;
            popup_position(output, self.size, anchor, place, 10.)
        }
    }
    fn running(&self) -> bool {
        self.reveal.is_running() || self.quick_opacity.is_running() || self.quick_scale.is_running()
    }
}
/// Pure host model plus UI attachment state. All methods run on the UI thread.
pub struct Host {
    bars: Vec<Bar>,
    bar_expansion: crate::laptop::BarExpansion,
    popups: BTreeMap<(Slot, String), Popup>,
    pub palette: Palette,
    pub matugen: hyprv_core::config::Theme,
    pub minute: i64,
    clock_topic: Option<hyprv_core::store::Topic<i64>>,
    clock_demand: Option<hyprv_core::store::Subscription<i64>>,

    pub store: Arc<StateStore>,
    pub data: BTreeMap<String, Value>,
    pub brightness: Option<u8>,
    pub volume: Option<u8>,
    pub quick_mode: String,
    pub quit: bool,
    data_time: Duration,
    child_opening: bool,
    child_active: bool,
    child_timeout: Option<Duration>,
    submenu_depth: usize,
    revision: u64,
    components: HashMap<Slot, Box<dyn Component>>,
    outputs: HashMap<String, WlOutput>,
    output_scales: HashMap<String,f32>,
    mounts: HashMap<SurfaceId, Mounted>,
    no_exclusive: bool,
    demo: bool,
}
impl Default for Host {
    fn default() -> Self {
        Self::new(false, false)
    }
}
impl Host {
    pub fn new(no_exclusive: bool, demo: bool) -> Self {
        Self {
            bars: Vec::new(),
            bar_expansion: Default::default(),
            popups: BTreeMap::new(),
            palette: Palette::default(),
            matugen: hyprv_core::config::Theme::default(),
            minute: hyprv_core::clock::unix_seconds() / 60,
            clock_topic: None,
            clock_demand: None,
            store: Arc::new(StateStore::default()),
            data: BTreeMap::new(),
            brightness: None,
            volume: None,
            quick_mode: "brightness".into(),
            quit: false,
            data_time: Duration::ZERO,
            child_opening: false,
            child_active: false,
            child_timeout: None,
            submenu_depth: 0,
            revision: 1,
            components: HashMap::new(),
            outputs: HashMap::new(),
            output_scales: HashMap::new(),
            mounts: HashMap::new(),
            no_exclusive,
            demo,
        }
    }
    pub fn register(&mut self, slot: Slot, component: impl Component + 'static) {
        self.components.insert(slot, Box::new(component));
        self.invalidate();
    }
    pub fn attach_clock(&mut self, topic: hyprv_core::store::Topic<i64>) {
        self.clock_topic = Some(topic);
        self.sync_clock_demand();
    }
    fn sync_clock_demand(&mut self) {
        if self.bars.is_empty() {
            self.clock_demand = None;
        } else if self.clock_demand.is_none() {
            self.clock_demand = self.clock_topic.as_ref().map(|t| t.subscribe());
        }
    }
    pub fn invalidate(&mut self) {
        self.revision += 1;
    }
    pub fn set_dark(&mut self, dark: bool) {
        if self.palette.dark != dark {
            self.palette = Palette::new(dark);
            self.invalidate();
        }
    }
    pub fn add_output(&mut self, output: Output) {
        if let Some(b) = self.bars.iter_mut().find(|b| b.output.name == output.name) {
            b.output = output;
        } else {
            self.bars.push(Bar::new(output));
        }
        self.sync_clock_demand();
        self.invalidate();
    }
    pub fn remove_output(&mut self, name: &str) {
        self.bars.retain(|b| b.output.name != name);
        self.bar_expansion.remove(name);
        self.outputs.remove(name);
        self.output_scales.remove(name);
        self.popups.retain(|_, p| p.output != name);
        self.sync_clock_demand();
        self.invalidate();
    }
    fn last_time(&self) -> Duration {
        self.data_time
    }
    pub fn primary(&self) -> Option<&str> {
        self.bars.first().map(|b| b.output.name.as_str())
    }
    fn popup(&self, slot: Slot) -> Option<&Popup> {
        self.popups.values().find(|p| p.slot == slot)
    }
    fn popup_mut(&mut self, slot: Slot) -> Option<&mut Popup> {
        self.popups.values_mut().find(|p| p.slot == slot)
    }
    fn popup_on(&self, slot: Slot, output: &str) -> Option<&Popup> {
        self.popups.get(&(slot, output.to_owned()))
    }
    pub fn apply(&mut self, action: HostAction, now: Duration) {
        match action {
            HostAction::Open {
                slot,
                output,
                anchor,
            } => {
                if !self.bars.iter().any(|b| b.output.name == output) {
                    return;
                }
                if matches!(slot, Slot::Bar | Slot::Island | Slot::Calendar) {
                    return;
                }
                // Wi-Fi controllers belong to each bar; other popup slots are singletons.
                if slot != Slot::Wifi {
                    self.popups.retain(|(s, o), _| *s != slot || *o == output);
                }
                let key = (slot, output.clone());
                if let Some(p) = self.popups.get_mut(&key) {
                    if p.output == output {
                        let reversing = p.closing;
                        p.requested = true;
                        p.closing = false;
                        p.unmap_at = None;
                        if slot == Slot::QuickAdjust {
                            p.hide_at = Some(now + Duration::from_secs(3));
                            p.quick_opacity.retarget(1., now);
                            p.quick_scale.retarget(1., now);
                        } else {
                            p.anchor = anchor;
                            if slot == Slot::Wifi && reversing {
                                p.reveal.play_open(now);
                                p.open_at = None;
                            } else if slot != Slot::TrayOverflow {
                                p.reveal.prepare_open();
                                p.open_at = Some(now + Duration::from_millis(16));
                            }
                        }
                    } else {
                        self.popups
                            .insert(key, Popup::new(slot, output, anchor, now));
                    }
                } else {
                    self.popups
                        .insert(key, Popup::new(slot, output, anchor, now));
                }
            }
            HostAction::Close(slot) => {
                for p in self.popups.values_mut().filter(|p| p.slot == slot) {
                    p.close(now);
                }
            }
            HostAction::CloseOn { slot, output } => {
                if let Some(p) = self.popups.get_mut(&(slot, output)) {
                    p.close(now);
                }
            }
            HostAction::Calendar { output } => {
                let already = self
                    .bars
                    .iter()
                    .any(|b| b.output.name == output && b.calendar_visible);
                if already && (self.child_opening || self.child_active) {
                    return;
                }
                for b in &mut self.bars {
                    let open = b.output.name == output && !already;
                    if b.calendar_visible != open {
                        b.calendar(open, now);
                    }
                }
            }
            HostAction::CloseCalendar { force } => {
                if !force && (self.child_opening || self.child_active) {
                    return;
                }
                for b in &mut self.bars {
                    if b.calendar_visible {
                        b.calendar(false, now);
                    }
                }
            }
            HostAction::CalendarChild { opening, active } => {
                let released = (self.child_opening || self.child_active) && !opening && !active;
                self.child_opening = opening;
                self.child_active = active;
                self.child_timeout = if opening {
                    Some(now + Duration::from_millis(2600))
                } else {
                    None
                };
                if opening {
                    self.apply(HostAction::CloseCalendar { force: true }, now);
                } else if released {
                    for b in &mut self.bars {
                        if b.calendar_visible && !b.calendar_inside {
                            b.calendar_leave = Some(now + Duration::from_millis(220));
                        }
                    }
                }
            }
            HostAction::Island {
                output,
                expanded,
                bounds,
                offset,
            } => {
                if let Some(b) = self.bars.iter_mut().find(|b| b.output.name == output) {
                    if expanded {
                        b.island_expanded = true;
                        b.island_hold = None;
                    } else if b.island_expanded {
                        b.island_expanded = false;
                        b.island_hold = Some(now + Duration::from_millis(230));
                    }
                    b.island_bounds = bounds;
                    b.island_offset = offset;
                }
            }
            HostAction::Fullscreen { output, value } => {
                if let Some(b) = self.bars.iter_mut().find(|b| b.output.name == output) {
                    b.fullscreen = value;
                    b.hide.retarget(if value { 1. } else { 0. }, now);
                }
            }
            HostAction::PopupSize {
                slot,
                width,
                height,
            } => {
                if let Some(p) = self.popup_mut(slot) {
                    p.size = (width, height);
                    p.reveal.set_full_height(height);
                }
            }
            HostAction::PopupSizeOn {
                slot,
                output,
                width,
                height,
            } => {
                if let Some(p) = self.popups.get_mut(&(slot, output)) {
                    p.size = (width, height);
                    p.reveal.set_full_height(height);
                }
            }
            HostAction::TraySubmenuDepth(depth) => self.submenu_depth = depth,
            HostAction::QuickValue(value) => {
                if self.quick_mode == "volume" {
                    self.volume = Some(value.min(100));
                } else {
                    self.brightness = Some(value.min(100));
                }
                if let Some(p) = self.popup_mut(Slot::QuickAdjust) {
                    p.hide_at = Some(now + Duration::from_secs(3));
                }
            }
        }
        self.invalidate();
    }
    fn data_status(&self) -> Value {
        let mut status = serde_json::Map::new();
        for name in [
            "detailedStats",
            "wifiConnected",
            "wifiNetworkCount",
            "networkInterface",
            "dnd",
            "cpu",
            "memory",
            "networkRxRate",
            "networkTxRate",
            "mediaPlayer",
            "mediaPosition",
            "mediaAvailable",
            "mediaPlaying",
            "spectrum",
            "powerProfile",
            "batteryAvailable",
            "batteryPercent",
        ] {
            status.insert(
                name.into(),
                self.data.get(name).cloned().unwrap_or(Value::Null),
            );
        }
        for (name, value) in [
            ("expanded", json!(self.bars.iter().any(|b|b.expanded))),
            ("systemInterval", json!(1000)),
            ("darkMode", json!(self.palette.dark)),
            (
                "panelPolling",
                json!(self.popup(Slot::ControlPanel).is_some_and(|p| p.mapped)),
            ),
            (
                "batteryDetailPolling",
                json!(self.popup(Slot::Battery).is_some_and(|p| p.mapped)),
            ),
            ("nativeAudio", json!(self.volume.is_some())),
            ("volume", json!(self.volume)),
            ("nativeWifi", json!(false)),
            ("wifiScanning", json!(false)),
            ("notifications", json!(false)),
            ("agentWatcher", json!(false)),
        ] {
            status.insert(name.into(), value);
        }
        Value::Object(status)
    }
    pub fn dispatch(&mut self, request: Request, now: Duration) -> IpcResponse {
        if let Err(e) = request.validate() {
            return IpcResponse::error(e);
        }
        match(request.target.as_str(),request.function.as_str()){
            ("shell","kill")=>{self.quit=true;},("bar","toggle")=>{if crate::laptop::active() && self.bar_expansion.toggle(&request.args[0],crate::laptop::wall_ms()) { if let Some(b)=self.bars.iter_mut().find(|b|b.output.name==request.args[0]) { b.expanded=self.bar_expansion.expanded(&request.args[0]); b.reveal.retarget(if b.expanded {1.} else {0.},now); } }},
            ("bar","layoutStatus")=>return IpcResponse::success(self.bars.iter().find(|b|b.output.name==request.args[0]).map_or(Value::Null,|b|json!({"expanded":b.expanded,"fullscreen":b.fullscreen,"reveal":b.reveal.value(now),"fullscreenHide":b.hide.value(now),"compactOffset":-15.*b.hide.value(now),"topCornerOffset":0,"bottomCornerOffset":0,"presented":b.hide.value(now)<1.,"duration":700,"curve":[0.1,0.9,0.1,1.05,1,1]})).to_string()),
            ("bar","inputStatus")=>return IpcResponse::success(self.bars.first().map_or(Value::Null,|b|{let c=b.center(now);json!({"monitor":b.output.name,"reveal":b.reveal.value(now),"contentY":b.y(now),"inputX":0,"inputY":b.y(now).floor(),"inputWidth":b.output.width.ceil(),"inputHeight":if b.calendar_visible{b.height()}else{58f32.max(c.y+c.height)}})}).to_string()),
            ("bar","dataStatus")=>return IpcResponse::success(self.data_status().to_string()),
            ("controlPanel","toggle")=>{if self.popup(Slot::ControlPanel).is_some_and(|p|p.mapped){self.apply(HostAction::Close(Slot::ControlPanel),now);}else if let Some(output)=self.primary().map(str::to_owned){self.apply(HostAction::Open{slot:Slot::ControlPanel,output,anchor:None},now);}},
            ("calendar","toggle")=>{if let Some(output)=self.primary().map(str::to_owned){self.apply(HostAction::Calendar{output},now);}},
            ("quickAdjust",function)=>{self.quick_mode=if function=="showVolume"{"volume"}else{"brightness"}.into();match function{"showBrightnessLevel"=>{let v=request.args[0].parse::<i32>().unwrap();let step=if crate::laptop::active(){1.}else{2.};self.brightness=Some(((v as f64/step).round()*step).clamp(0.,100.) as u8);},"showBrightnessIncrease"=>self.brightness=Some(self.brightness.unwrap_or(50).saturating_add(if crate::laptop::active(){1}else{2}).min(100)),"showBrightnessDecrease"=>self.brightness=Some(self.brightness.unwrap_or(50).saturating_sub(if crate::laptop::active(){1}else{2})),_=>{}}if let Some(output)=self.primary().map(str::to_owned){self.apply(HostAction::Open{slot:Slot::QuickAdjust,output,anchor:None},now);}},_=>return IpcResponse::error("unhandled IPC")
        }
        self.invalidate();
        IpcResponse::success("")
    }
    pub fn tick(&mut self, now: Duration) {
        let mut changed = false;
        for b in &mut self.bars {
            b.hide.advance(now);
            b.reveal.advance(now);
            b.calendar_progress.advance(now);
            b.calendar_width.advance(now);
            b.calendar_height.advance(now);
            b.calendar_radius.advance(now);
            if b.island_hold.is_some_and(|d| now >= d) {
                b.island_hold = None;
                changed = true;
            }
            if b.calendar_close.is_some_and(|d| now >= d) {
                b.calendar_close = None;
                b.calendar_presented = false;
                changed = true;
            }
            if b.calendar_leave.is_some_and(|d| now >= d) {
                b.calendar_leave = None;
                if b.calendar_visible
                    && !b.calendar_inside
                    && !self.child_opening
                    && !self.child_active
                {
                    b.calendar(false, now);
                    changed = true;
                }
            }
        }
        for p in self.popups.values_mut() {
            if p.open_at.is_some_and(|d| now >= d) && p.size.1 > 0. {
                p.open_at = None;
                p.reveal.play_open(now);
                changed = true;
            }
            if p.reveal.advance(now) == Some(Transition::Close) && p.closing {
                p.mapped = false;
                changed = true;
            }
            p.quick_opacity.advance(now);
            p.quick_scale.advance(now);
            if p.hide_at.is_some_and(|d| now >= d) {
                p.close(now);
                changed = true;
            }
            if p.unmap_at.is_some_and(|d| now >= d) {
                p.mapped = false;
                changed = true;
            }
        }
        if self.child_timeout.is_some_and(|d| now >= d) {
            self.child_timeout = None;
            self.child_opening = false;
            changed = true;
        }
        self.popups.retain(|_, p| p.mapped);
        if changed {
            self.invalidate();
        }
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.bars
            .iter()
            .flat_map(|b| [b.island_hold, b.calendar_close, b.calendar_leave, None])
            .chain(
                self.popups
                    .values()
                    .flat_map(|p| [p.open_at.filter(|_| p.size.1 > 0.), p.hide_at, p.unmap_at]),
            )
            .chain([self.child_timeout])
            .flatten()
            .min()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Role {
    Bar,
    Corner(u8),
    Popup(Slot),
}
struct Mounted {
    role: Role,
    output: String,
    parts: HashMap<&'static str, NodeId>,
    revision: u64,
    signature: Vec<f32>,
}
#[derive(Clone)]
pub struct HostAdapter(pub Rc<RefCell<Host>>);
impl Host {
    fn port(&mut self, slot: Slot, output: &str, bounds: Rect) -> Node {
        let context = ViewContext {
            slot,
            output,
            bounds,
            palette: &self.palette,
            minute: self.minute,
            store: &self.store,
            data: &self.data,
            quick_mode: &self.quick_mode,
            brightness: self.brightness,
            volume: self.volume,
        };
        if let Some(component) = self.components.get_mut(&slot) {
            return component.build(&context);
        }
        if self.demo || slot == Slot::Island {
            let mut n = rounded_rect(bounds, 24., self.palette.module_background, None);
            let text = if slot == Slot::Island {
                chrono::DateTime::from_timestamp(self.minute * 60, 0)
                    .map(|time| {
                        time.with_timezone(&chrono::Local)
                            .format("%H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "--:--".into())
            } else {
                format!("{slot:?}")
            };
            n.children.push(
                Label::new(
                    text,
                    FontSpec {
                        pixel_size: if slot == Slot::Island { 16. } else { 14. },
                        ..Default::default()
                    },
                    self.palette.primary_text,
                )
                .centered_node(Rect::new(0., 0., bounds.width, bounds.height)),
            );
            n
        } else {
            Node::new(bounds, Content::Group)
        }
    }
    fn build(
        &mut self,
        role: Role,
        output: &str,
        now: Duration,
    ) -> (Scene, HashMap<&'static str, NodeId>) {
        let b = self.bars.iter().find(|b| b.output.name == output).unwrap();
        let full = (b.output.width, b.output.height);
        let bar_h = b.height();
        let center = b.center(now);
        let mut parts = HashMap::new();
        let mut root = Node::new(
            Rect::new(
                0.,
                0.,
                full.0,
                if role == Role::Bar { bar_h } else { full.1 },
            ),
            Content::Group,
        );
        match role {
            Role::Corner(_) => {
                root=hyprv_ui_bar::surfaces::corner_window(full.0,full.1,
                    self.output_scales.get(output).copied().unwrap_or(1.),0.,0.);
            }
            Role::Bar => {
                let mut body = Node::new(Rect::new(0., 0., full.0, bar_h), Content::Group);
                parts.insert("body", body.id());
                body.children
                    .push(self.port(Slot::Bar, output, Rect::new(0., 0., full.0, 58.)));
                let mut dismiss = Node::new(Rect::new(0., 0., full.0, 0.), Content::Group);
                dismiss.interactive = true;
                parts.insert("outside", dismiss.id());
                body.children.push(dismiss);
                let mut host = rounded_rect(center, 24., Color::default(), None);
                host.interactive = true;
                host.clip = Some([0.; 4]);
                parts.insert("center", host.id());
                let island = self.port(Slot::Island, output, Rect::new(0., 0., center.width, 38.));
                parts.insert("island", island.id());
                host.children.push(island);
                let calendar = self.port(
                    Slot::Calendar,
                    output,
                    Rect::new(
                        0.,
                        0.,
                        full.0.min(568.) - 48.,
                        (bar_h - 78.).clamp(0., 500.),
                    ),
                );
                parts.insert("calendar", calendar.id());
                host.children.push(calendar);
                body.children.push(host);
                root.children.push(body);
            }
            Role::Popup(slot) => {
                let p = self.popup_on(slot, output).unwrap();
                let size = p.size;
                let style = GlassStyle {
                    fill: if slot == Slot::ControlPanel {
                        Color::default()
                    } else {
                        self.palette.glass_fill
                    },
                    stroke: if slot == Slot::ControlPanel {
                        Color::default()
                    } else {
                        self.palette.glass_stroke
                    },
                    surface_opacity: if slot == Slot::ControlPanel { 0. } else { 0.82 },
                    shadow: if slot == Slot::Stats {
                        with_alpha(
                            hyprv_widgets::foundations::rgb(if self.palette.dark {
                                "#000000"
                            } else {
                                "#111111"
                            }),
                            if self.palette.dark { 0.45 } else { 0.18 },
                        )
                    } else {
                        Color::default()
                    },
                    ..Default::default()
                };
                let reveal = p.reveal.clone();
                let mut outside = Node::new(root.frame, Content::Group);
                outside.interactive = slot != Slot::QuickAdjust;
                parts.insert("outside", outside.id());
                root.children.push(outside);
                let content = self.port(slot, output, Rect::new(0., 0., size.0, size.1));
                let mut card = if slot == Slot::TrayOverflow {
                    let mut n = hyprv_widgets::primitives::rounded_rect(
                        Rect::new(0., 0., size.0, size.1),
                        18.,
                        self.palette.glass_fill,
                        Some(hyprv_ui::scene::Border {
                            width: 1.,
                            color: self.palette.glass_stroke,
                        }),
                    );
                    n.children.push(content);
                    n
                } else if slot == Slot::QuickAdjust {
                    let mut n = Node::new(Rect::new(0., 0., size.0, size.1), Content::Group);
                    n.children.push(content);
                    n
                } else {
                    let n = glass_panel(
                        Rect::new(0., 0., size.0, size.1),
                        &reveal,
                        &style,
                        vec![content],
                    );
                    parts.insert("viewport", n.children[0].id());
                    parts.insert("content", n.children[0].children[0].children[1].id());
                    n
                };
                card.interactive = slot.swallows_card();
                parts.insert("card", card.id());
                root.children.push(card);
            }
        }
        parts.insert("root", root.id());
        (Scene::new(root), parts)
    }
    fn config(&self, role: Role, output: &str, now: Duration) -> LayerConfig {
        let b = self.bars.iter().find(|b| b.output.name == output).unwrap();
        let mut config = LayerConfig {
            output: self.outputs.get(output).cloned(),
            ..Default::default()
        };
        match role {
            Role::Bar => {
                config.namespace = "hyprv-quickshell".into();
                config.size = (0, b.height().max(1.) as u32);
                config.anchor = Anchor::TOP | Anchor::LEFT | Anchor::RIGHT;
                config.exclusive_zone = if self.no_exclusive { 0 } else { (25. + 33. * b.reveal.value(now)).round() as i32 };
            }
            Role::Corner(_) => {
                config.namespace = format!("hyprv-screen-corner-overlay-{output}");
                config.layer = Layer::Overlay;
                config.size = (0,0);
                config.exclusive_zone = -1;
                config.input_passthrough = true;
                config.full_redraw=true;
                config.sparse=true;
                config.anchor = Anchor::TOP|Anchor::BOTTOM|Anchor::LEFT|Anchor::RIGHT;
            }
            Role::Popup(slot) => {
                config.namespace = slot.namespace().into();
                config.layer = Layer::Overlay;
                config.size = (0, 0);
                config.anchor = Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM;
                config.exclusive_zone = -1;
                config.keyboard = if slot == Slot::QuickAdjust {
                    KeyboardInteractivity::None
                } else {
                    KeyboardInteractivity::OnDemand
                };
            }
        }
        config
    }
    fn layout(
        &self,
        mount: &Mounted,
        scene: &mut Scene,
        now: Duration,
    ) -> (Vec<f32>, Vec<Rect>, (u32, u32), Layer) {
        let b = self
            .bars
            .iter()
            .find(|b| b.output.name == mount.output)
            .unwrap();
        let full = (b.output.width, b.output.height);
        match mount.role {
            Role::Corner(_) => {
                let scale=self.output_scales.get(&mount.output).copied().unwrap_or(1.);
                let signature=vec![full.0,full.1,scale];
                if mount.signature!=signature {
                    scene.edit(scene.root().id(),|root|{
                        let node=hyprv_ui_bar::surfaces::corner_window(full.0,full.1,scale,0.,0.);
                        root.frame=node.frame;root.native_materials=node.native_materials;root.children=node.children;
                    });
                }
                (signature,vec![],(0,0),Layer::Overlay)
            },
            Role::Bar => {
                let center = b.center(now);
                let y = b.y(now);
                let progress = b.calendar_progress.value(now) as f32;
                let radius = b.calendar_radius.value(now) as f32;
                let h = b.height();
                let signature = vec![
                    full.0,
                    h,
                    center.x,
                    center.width,
                    center.height,
                    y,
                    progress,
                    radius,
                    if b.calendar_visible { 1. } else { 0. },
                ];
                if mount.signature != signature {
                    scene.edit(mount.parts["root"], |root| {
                        root.frame.width = full.0;
                        root.frame.height = h;
                        if let Some(n) = find_mut(root, mount.parts["body"]) {
                            n.frame.y = y;
                            n.frame.height = h;
                            n.frame.width = full.0;
                        }
                        if let Some(n) = find_mut(root, mount.parts["outside"]) {
                            n.frame =
                                Rect::new(0., 0., full.0, if b.calendar_visible { h } else { 0. });
                        }
                        if let Some(n) = find_mut(root, mount.parts["center"]) {
                            n.frame = center;
                            if let Content::Rect(r) = &mut n.content {
                                r.radii = [radius; 4];
                                r.fill = hyprv_ui::scene::Fill::Solid(if progress > 0.001 {
                                    self.palette.module_background
                                } else {
                                    Color::default()
                                });
                                r.border = if progress > 0.001 {
                                    Some(Border {
                                        width: 1.,
                                        color: with_alpha(
                                            self.palette.primary_text,
                                            if self.palette.dark { 0.11 } else { 0.09 },
                                        ),
                                    })
                                } else {
                                    None
                                };
                            }
                        }
                        if let Some(n) = find_mut(root, mount.parts["island"]) {
                            n.opacity = 1. - progress;
                            n.visible = n.opacity > 0.01;
                        }
                        if let Some(n) = find_mut(root, mount.parts["calendar"]) {
                            n.opacity = progress;
                            n.visible = b.calendar_presented && progress > 0.01;
                        }
                    });
                }
                let mut mask = vec![
                    Rect::new(0., y.floor(), full.0.ceil(), 58.),
                    Rect::new(center.x, center.y + y, center.width, center.height),
                ];
                if b.calendar_visible {
                    mask.push(Rect::new(0., y, full.0, h));
                }
                (
                    signature,
                    mask,
                    (0, h as u32),
                    if b.fullscreen || b.hide.value(now) != 0. {
                        Layer::Overlay
                    } else {
                        Layer::Top
                    },
                )
            }
            Role::Popup(slot) => {
                let p = self.popup_on(slot, &mount.output).unwrap();
                let card = p.position(b, now);
                let v = p.reveal.values();
                let opacity = p.quick_opacity.value(now) as f32;
                let scale = p.quick_scale.value(now) as f32;
                let signature = vec![
                    card.x,
                    card.y,
                    card.width,
                    card.height,
                    v.height,
                    v.opacity,
                    opacity,
                    scale,
                    full.0,
                    full.1,
                ];
                if mount.signature != signature {
                    scene.edit(mount.parts["root"], |root| {
                        root.frame = Rect::new(0., 0., full.0, full.1);
                        if let Some(n) = find_mut(root, mount.parts["outside"]) {
                            n.frame = root_extent(full);
                        }
                        if let Some(n) = find_mut(root, mount.parts["card"]) {
                            n.frame = card;
                            if slot == Slot::QuickAdjust {
                                n.opacity = opacity;
                                let mut m = Matrix::translate((card.width, 0.));
                                m.pre_scale((scale, scale), None);
                                m.pre_translate((-card.width, 0.));
                                n.transform = m;
                            }
                        }
                        if let Some(id) = mount.parts.get("viewport") {
                            if let Some(n) = find_mut(root, *id) {
                                n.frame.height = v.height;
                            }
                        }
                        if let Some(id) = mount.parts.get("content") {
                            if let Some(n) = find_mut(root, *id) {
                                n.opacity = v.opacity;
                            }
                        }
                    });
                }
                let masks = if slot == Slot::QuickAdjust {
                    vec![card]
                } else {
                    vec![root_extent(full)]
                };
                (signature, masks, (0, 0), Layer::Overlay)
            }
        }
    }
    fn sync(&mut self, engine: &mut Engine) {
        let now = engine.now();
        self.data_time = now;
        self.tick(now);
        if self.quit {
            engine.quit();
            return;
        }
        let mut desired = Vec::new();
        for b in &self.bars {
            desired.push((b.output.name.clone(), Role::Bar));
            desired.push((b.output.name.clone(),Role::Corner(0)));
        }
        for p in self.popups.values() {
            desired.push((p.output.clone(), Role::Popup(p.slot)));
        }
        let live: Vec<_> = engine.surfaces().into_iter().map(|(id, _)| id).collect();
        let stale: Vec<_> = self
            .mounts
            .iter()
            .filter(|(id, m)| !live.contains(id) || !desired.contains(&(m.output.clone(), m.role)))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            engine.close(id);
            self.mounts.remove(&id);
        }
        for (output, role) in desired {
            if self
                .mounts
                .values()
                .any(|m| m.role == role && m.output == output)
            {
                continue;
            }
            let (scene, parts) = self.build(role, &output, now);
            let id = engine.add_surface(self.config(role, &output, now), scene);
            self.mounts.insert(
                id,
                Mounted {
                    role,
                    output,
                    parts,
                    revision: self.revision,
                    signature: Vec::new(),
                },
            );
        }
        let ids: Vec<_> = self.mounts.keys().copied().collect();
        for id in ids {
            let mut mount = self.mounts.remove(&id).unwrap();
            if mount.revision != self.revision {
                let (scene, parts) = self.build(mount.role, &mount.output, now);
                engine.replace_scene(id, scene);
                mount.parts = parts;
                mount.revision = self.revision;
                mount.signature.clear();
            }
            if let Some(scene) = engine.scene(id) {
                let (sig, mask, size, layer) = self.layout(&mount, scene, now);
                if sig != mount.signature {
                    let config = self.config(mount.role, &mount.output, now);
                    engine.configure_surface(
                        id,
                        size,
                        layer,
                        config.exclusive_zone,
                        config.keyboard,
                    );
                    engine.set_input_region(id, Some(&mask));
                    mount.signature = sig;
                }
            }
            let running = match mount.role {
                Role::Bar => self
                    .bars
                    .iter()
                    .find(|b| b.output.name == mount.output)
                    .is_some_and(Bar::running),
                Role::Popup(slot) => self
                    .popup_on(slot, &mount.output)
                    .is_some_and(Popup::running),
                _ => false,
            };
            self.mounts.insert(id, mount);
            if running {
                engine.animate(id);
            }
        }
    }
    fn event(&mut self, id: SurfaceId, event: InputEvent, now: Duration) {
        let Some(m) = self.mounts.get(&id) else {
            return;
        };
        let role = m.role;
        let output = m.output.clone();
        let outside = m.parts.get("outside").copied();
        let center = m.parts.get("center").copied();
        let mut actions = Vec::new();
        match &event {
            InputEvent::Resize { width, height, .. } if *width > 0 && *height > 0 => {
                if let Some(b) = self.bars.iter_mut().find(|b| b.output.name == output) {
                    if role == Role::Bar && b.output.width != *width as f32 {
                        b.output.width = *width as f32;
                        self.invalidate();
                    } else if matches!(role, Role::Popup(_))
                        && (b.output.width != *width as f32 || b.output.height != *height as f32)
                    {
                        b.output.width = *width as f32;
                        b.output.height = *height as f32;
                        self.invalidate();
                    }
                }
            }
            InputEvent::Click { target, button } => match role {
                Role::Popup(slot) if Some(*target) == outside => {
                    actions.push(HostAction::CloseOn {
                        slot,
                        output: output.clone(),
                    })
                }
                Role::Bar if *button == 0x110 && Some(*target) == outside => {
                    actions.push(HostAction::CloseCalendar { force: false })
                }
                Role::Bar if *button == 0x110 && Some(*target) == center => {
                    actions.push(HostAction::Calendar {
                        output: output.clone(),
                    })
                }
                _ => {}
            },
            InputEvent::Key {
                event,
                pressed: true,
                ..
            } if event.keysym == KeySym::Escape => {
                if let Role::Popup(slot) = role {
                    if slot == Slot::TrayMenu && self.submenu_depth > 0 {
                        self.submenu_depth -= 1;
                        self.data
                            .insert("trayBackRequested".into(), json!(self.submenu_depth));
                        self.invalidate();
                    } else {
                        actions.push(HostAction::CloseOn {
                            slot,
                            output: output.clone(),
                        });
                    }
                }
            }
            InputEvent::Pointer { event, .. } if role == Role::Bar => {
                if let Some(b) = self.bars.iter_mut().find(|b| b.output.name == output) {
                    if b.calendar_visible {
                        let center = b.center(now);
                        let inside = center
                            .contains(event.position.0 as f32, event.position.1 as f32 - b.y(now))
                            && !matches!(event.kind, PointerKind::Leave { .. });
                        if inside {
                            b.calendar_leave = None;
                        } else if b.calendar_inside || b.calendar_arm.is_some_and(|d| now >= d) {
                            b.calendar_leave = Some(now + Duration::from_millis(220));
                        }
                        b.calendar_inside = inside;
                    }
                }
            }
            _ => {}
        }
        let slots = match role {
            Role::Bar => vec![Slot::Bar, Slot::Island, Slot::Calendar],
            Role::Popup(slot) => vec![slot],
            _ => Vec::new(),
        };
        for slot in slots {
            if let Some(component) = self.components.get_mut(&slot) {
                let context = ViewContext {
                    slot,
                    output: &output,
                    bounds: Rect::default(),
                    palette: &self.palette,
                    minute: self.minute,
                    store: &self.store,
                    data: &self.data,
                    quick_mode: &self.quick_mode,
                    brightness: self.brightness,
                    volume: self.volume,
                };
                actions.extend(component.input(&context, &event));
            }
        }
        for action in actions {
            self.apply(action, now);
        }
    }
}
impl UiHandler for HostAdapter {
    fn clock(&mut self, now: Duration) {
        self.0.borrow_mut().data_time = now;
    }
    fn output_changed(&mut self, output: &WlOutput, info: Option<MonitorInfo>) {
        self.output_added(output, info);
    }
    fn output_added(
        &mut self,
        output: &WlOutput,
        info: Option<MonitorInfo>,
    ) -> Vec<(LayerConfig, Scene)> {
        if let Some(info) = info {
            // Rotation-aware: xdg-output size, else the mode with w/h swapped for 90/270.
            let size = hyprv_ui::wayland::output_logical_size(&info).unwrap_or((1280, 720));
            let name = info.name.unwrap_or_else(|| format!("output-{}", info.id));
            let mut host = self.0.borrow_mut();
            host.outputs.insert(name.clone(), output.clone());
            host.output_scales.insert(name.clone(),info.scale_factor as f32);
            host.add_output(Output {
                name,
                width: size.0 as f32,
                height: size.1 as f32,
            });
        }
        Vec::new()
    }
    fn output_removed(&mut self, output: &WlOutput) {
        let name = self
            .0
            .borrow()
            .outputs
            .iter()
            .find(|(_, o)| *o == output)
            .map(|(n, _)| n.clone());
        if let Some(name) = name {
            self.0.borrow_mut().remove_output(&name);
        }
    }
    fn update(&mut self, engine: &mut Engine) {
        self.0.borrow_mut().sync(engine);
    }
    fn next_deadline(&self) -> Option<Duration> {
        self.0.borrow().next_deadline()
    }
    fn input(&mut self, id: SurfaceId, _: &mut Scene, event: InputEvent) -> Response {
        let now = self.0.borrow().last_time();
        self.0.borrow_mut().event(id, event, now);
        Response::default()
    }
    fn frame(&mut self, id: SurfaceId, scene: &mut Scene, time: FrameInfo) -> bool {
        let mut host = self.0.borrow_mut();
        host.data_time = time.time;
        host.tick(time.time);
        if let Some(m) = host.mounts.get(&id) {
            // A closing animation can unmap its model in tick above; the surface
            // is reconciled by update after this final frame callback.
            if let Role::Popup(slot) = m.role {
                if host.popup_on(slot, &m.output).is_none() {
                    return false;
                }
            }
            let (sig, _, _, _) = host.layout(m, scene, time.time);
            let output = m.output.clone();
            let role = m.role;
            let _ = sig;
            match role {
                Role::Bar => host
                    .bars
                    .iter()
                    .find(|b| b.output.name == output)
                    .is_some_and(Bar::running),
                Role::Popup(slot) => host.popup_on(slot, &output).is_some_and(Popup::running),
                _ => false,
            }
        } else {
            false
        }
    }
}
fn root_extent(size: (f32, f32)) -> Rect {
    Rect::new(0., 0., size.0, size.1)
}
fn find_mut(n: &mut Node, id: NodeId) -> Option<&mut Node> {
    if n.id() == id {
        Some(n)
    } else {
        n.children.iter_mut().find_map(|c| find_mut(c, id))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Host {
        let mut h = Host::default();
        h.add_output(Output {
            name: "DP-1".into(),
            width: 1280.,
            height: 720.,
        });
        h
    }
    fn request(target: &str, function: &str, args: &[&str]) -> Request {
        Request {
            target: target.into(),
            function: function.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn ipc_drives_real_calendar_and_osd_lifetimes() {
        let mut h = host();
        let start = Duration::from_secs(10);
        assert!(h.dispatch(request("calendar", "toggle", &[]), start).ok);
        assert!(h.bars[0].calendar_presented);
        assert_eq!(h.bars[0].height(), 720.);
        h.tick(start + MS210);
        assert_eq!(h.bars[0].calendar_progress.value(start + MS210), 1.);
        h.dispatch(
            request("calendar", "toggle", &[]),
            start + Duration::from_secs(1),
        );
        assert!(h.bars[0].calendar_presented);
        h.tick(start + Duration::from_millis(1210));
        assert!(!h.bars[0].calendar_presented);
        assert_eq!(h.bars[0].height(), 58.);
        h.dispatch(
            request("quickAdjust", "showBrightnessLevel", &["33"]),
            start,
        );
        assert_eq!(h.brightness, Some(34));
        h.tick(start + Duration::from_secs(3));
        assert!(h.popup(Slot::QuickAdjust).unwrap().mapped);
        assert!(h.popup(Slot::QuickAdjust).unwrap().closing);
        h.tick(start + Duration::from_millis(3140));
        assert!(h.popup(Slot::QuickAdjust).is_none());
        assert!(h.next_deadline().is_none());
    }
    #[test]
    fn popup_close_finishes_before_unmap_and_reopen_reverses() {
        let mut h = host();
        let open = HostAction::Open {
            slot: Slot::ControlPanel,
            output: "DP-1".into(),
            anchor: None,
        };
        h.apply(open.clone(), Duration::ZERO);
        h.tick(Duration::from_millis(16));
        h.tick(Duration::from_millis(300));
        assert!(!h.popup(Slot::ControlPanel).unwrap().running());
        h.apply(
            HostAction::Close(Slot::ControlPanel),
            Duration::from_millis(300),
        );
        h.tick(Duration::from_millis(400));
        assert!(h.popup(Slot::ControlPanel).is_some());
        h.apply(open, Duration::from_millis(400));
        h.tick(Duration::from_millis(416));
        h.tick(Duration::from_millis(700));
        assert!(h.popup(Slot::ControlPanel).unwrap().requested);
        h.apply(
            HostAction::Close(Slot::ControlPanel),
            Duration::from_millis(700),
        );
        h.tick(Duration::from_millis(910));
        assert!(h.popup(Slot::ControlPanel).is_none());
    }
    #[test]
    fn wifi_is_per_output_reverses_and_overflow_is_instant() {
        let mut h = host();
        h.add_output(Output {
            name: "SECOND".into(),
            width: 1280.,
            height: 720.,
        });
        let primary = h.primary().unwrap().to_owned();
        for output in [primary.clone(), "SECOND".into()] {
            h.apply(
                HostAction::Open {
                    slot: Slot::Wifi,
                    output,
                    anchor: None,
                },
                Duration::ZERO,
            );
        }
        assert_eq!(
            h.popups.values().filter(|p| p.slot == Slot::Wifi).count(),
            2
        );
        h.tick(Duration::from_millis(16));
        h.tick(Duration::from_millis(200));
        h.apply(
            HostAction::CloseOn {
                slot: Slot::Wifi,
                output: primary.clone(),
            },
            Duration::from_millis(200),
        );
        h.tick(Duration::from_millis(240));
        let height = h
            .popup_on(Slot::Wifi, &primary)
            .unwrap()
            .reveal
            .values()
            .height;
        assert!(height > 2.);
        h.apply(
            HostAction::Open {
                slot: Slot::Wifi,
                output: primary.clone(),
                anchor: None,
            },
            Duration::from_millis(240),
        );
        assert_eq!(
            h.popup_on(Slot::Wifi, &primary)
                .unwrap()
                .reveal
                .values()
                .height,
            height
        );
        assert!(!h.popup_on(Slot::Wifi, "SECOND").unwrap().closing);
        h.apply(
            HostAction::Open {
                slot: Slot::TrayOverflow,
                output: primary,
                anchor: None,
            },
            Duration::ZERO,
        );
        assert!(h.popup(Slot::TrayOverflow).unwrap().open_at.is_none());
        assert!(!h.popup(Slot::TrayOverflow).unwrap().running());
        h.apply(HostAction::Close(Slot::TrayOverflow), Duration::ZERO);
        assert!(!h.popup(Slot::TrayOverflow).unwrap().mapped);
    }
    #[test]
    fn primary_reselection_and_clock_demand_follow_outputs() {
        let mut h = host();
        let topic = hyprv_core::store::Topic::new(0);
        h.attach_clock(topic.clone());
        assert_eq!(topic.subscriber_count(), 1);
        h.add_output(Output {
            name: "DP-2".into(),
            width: 800.,
            height: 600.,
        });
        h.remove_output("DP-1");
        assert_eq!(h.primary(), Some("DP-2"));
        h.remove_output("DP-2");
        assert_eq!(topic.subscriber_count(), 0);
        assert_eq!(h.primary(), None);
    }
    #[test]
    fn fullscreen_keeps_exclusive_zone_and_uses_native_curve() {
        let mut h = host();
        h.apply(
            HostAction::Fullscreen {
                output: "DP-1".into(),
                value: true,
            },
            Duration::ZERO,
        );
        h.tick(Duration::from_millis(700));
        assert_eq!(h.bars[0].y(Duration::from_millis(700)), -68.);
        assert_eq!(h.config(Role::Bar, "DP-1", Duration::ZERO).exclusive_zone, 58);
        let json: Value = serde_json::from_str(
            &h.dispatch(
                request("bar", "layoutStatus", &["DP-1"]),
                Duration::from_millis(700),
            )
            .result,
        )
        .unwrap();
        assert_eq!(json["presented"], false);
        let before = h.revision;
        h.dispatch(request("bar", "toggle", &["DP-1"]), Duration::ZERO);
        assert_eq!(h.bars[0].hide.value(Duration::from_millis(700)), 1.);
        assert!(h.revision > before);
    }
    #[test]
    fn corner_geometry_and_osd_masks_are_distinct() {
        let h = host();
        for c in 0..4 {
            let config = h.config(Role::Corner(c), "DP-1", Duration::ZERO);
            assert_eq!(config.size, (0, 0));
            assert!(config.full_redraw && config.sparse);
            assert!(config.input_passthrough);
            assert_eq!(config.exclusive_zone, -1);
        }
        let config = h.config(Role::Popup(Slot::QuickAdjust), "DP-1", Duration::ZERO);
        assert_eq!(config.keyboard, KeyboardInteractivity::None);
        let config = h.config(Role::Popup(Slot::ControlPanel), "DP-1", Duration::ZERO);
        assert_eq!(config.keyboard, KeyboardInteractivity::OnDemand);
    }
    #[test]
    fn zero_height_popups_wait_without_polling() {
        let mut h = host();
        h.apply(
            HostAction::Open {
                slot: Slot::Stats,
                output: "DP-1".into(),
                anchor: None,
            },
            Duration::ZERO,
        );
        h.apply(
            HostAction::PopupSize {
                slot: Slot::Stats,
                width: 548.,
                height: 0.,
            },
            Duration::ZERO,
        );
        assert!(h.next_deadline().is_none());
        h.apply(
            HostAction::PopupSize {
                slot: Slot::Stats,
                width: 548.,
                height: 200.,
            },
            Duration::from_secs(1),
        );
        h.tick(Duration::from_secs(1));
        assert!(h.popup(Slot::Stats).unwrap().reveal.is_running());
    }
}
