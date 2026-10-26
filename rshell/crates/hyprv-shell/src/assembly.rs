//! Complete UI/provider assembly. The compatibility Host remains a model test fixture.
use crate::services::{Consumer, Hub};
use hyprv_core::{
    calloop::{
        channel::Event,
        timer::{TimeoutAction, Timer},
    },
    ipc::{self, IpcServer, Request, Response as IpcResponse},
    runtime::ServiceRuntime,
    store::StateStore,
};
use hyprv_ui::{geometry::Rect, scene::Scene, wayland::*};
use hyprv_ui_bar as bar;
use hyprv_ui_panels as panels;
use hyprv_widgets::controls::PointerEvent;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Duration};
#[derive(Clone, Debug, PartialEq, Eq)]
enum Role {
    Bar(String),
    Corner(String, usize),
    BarPopup(bar::popups::Kind),
    Panel(String, panels::PanelKind),
    Startup,
    Tooltip(String),
    Dismiss(Box<Role>),
}
fn role_output(role: &Role) -> Option<&str> {
    match role {
        Role::Bar(name) | Role::Panel(name, _) | Role::Corner(name, _) | Role::Tooltip(name) => Some(name),
        Role::Dismiss(inner) => role_output(inner),
        _ => None,
    }
}
struct Mount {
    role: Role,
    config: LayerConfig,
    mask: Vec<Rect>,
    bar_view: Option<bar::view::RetainedView>,
}
/// Stats-only edits can bypass component layout when all displayed metrics keep
/// their measured dimensions and severity. Structural/style changes use the bar's
/// normal retained reconciler. Match unique existing text, never guessed node slots.
fn stats_style(m:&bar::Model)->(bool,Option<u8>){
    (m.stats.temperature_c.unwrap_or(0.)>=70.,m.stats.package_power_w.map(|w|if w>=90.{2}else if w>=60.{1}else if w.is_finite(){0}else{3}))
}
fn stats_labels(m:&bar::Model)->[String;5]{
        [format!("\u{f2db} {}%",bar::format::js_round(m.stats.cpu_percent.unwrap_or(0.))),
         format!("\u{f0c9} {}%",bar::format::js_round(m.stats.memory_percent.unwrap_or(0.))),
         bar::format::network_label(m),
         format!("{} {}°C",if m.stats.temperature_c.unwrap_or(0.)>=70.{"\u{f2c7}"}else{"\u{f2c9}"},bar::format::js_round(m.stats.temperature_c.unwrap_or(0.))),
         bar::format::power_label(m)]
    }
fn patch_stats_text(scene:&mut Scene,before:&[String;5],before_style:(bool,Option<u8>),new:&bar::Model)->Option<usize>{
    let _timing=hyprv_ui::diagnostics::scope("shell.stats_text_patch");
    if before_style!=stats_style(new){return None;}
    fn find(n:&hyprv_ui::scene::Node,label:&str,out:&mut Vec<hyprv_ui::scene::NodeId>){
        if let hyprv_ui::scene::Content::Text(t)=&n.content {if t.text==label {out.push(n.id());}}
        for child in &n.children {find(child,label,out);}
    }
    let after=stats_labels(new);let mut edits=Vec::new();
    for (before,after) in before.iter().zip(after) {
        if before==&after {continue;}
        let mut ids=Vec::new();find(scene.root(),before,&mut ids);
        let [id]=ids.as_slice()else{return None;};
        let hyprv_ui::scene::Content::Text(text)=&scene.root().find(*id)?.content else{return None;};
        if bar::layout::measure(before,text.size,text.weight)!=bar::layout::measure(&after,text.size,text.weight){return None;}
        edits.push((*id,after));
    }
    let changed=edits.len();
    for(id,value)in edits {scene.edit(id,|node|if let hyprv_ui::scene::Content::Text(t)=&mut node.content {t.text=value;});}
    Some(changed)
}
fn bar_tree(model: &bar::Model) -> hyprv_ui::scene::Node {
    let mut tree = bar::build(model, &model.theme);
    // Compare canonical clip geometry, rather than undoing the retained tree's
    // device scissor and reapplying it after every paint callback.
    panels::qrect::convert(&mut tree, (0., 0.), model.output.scale);
    tree
}
#[cfg(test)]
fn update_bar_scene(scene: &mut Scene, model: &bar::Model) -> usize {
    let root = scene.root().id();
    bar::view::reconcile(scene, root, bar_tree(model))
}
fn shared_tree(tree: Rc<hyprv_ui::scene::Node>) -> hyprv_ui::scene::Node {
    hyprv_ui::scene::Node::new(
        Rect::new(0., 0., tree.frame.width, tree.frame.height),
        hyprv_ui::scene::Content::Shared(tree),
    )
}
#[derive(Default)]
struct IslandTree {
    revision: Option<u64>,
    tree: Option<Rc<hyprv_ui::scene::Node>>,
}
impl IslandTree {
    fn get(&mut self, island: &hyprv_ui_island::Island) -> hyprv_ui::scene::Node {
        let revision = island.visual_rev();
        if self.tree.is_none() || revision.is_none() || revision != self.revision {
            self.tree = Some(Rc::new(island.build()));
            self.revision = revision;
        }
        shared_tree(self.tree.as_ref().unwrap().clone())
    }
}
type CalendarKey = (
    panels::model::Theme,
    panels::model::Calendar,
    i64,
    (f32, f32, f32, f32, f32),
);
#[derive(Default)]
struct CalendarTree {
    prepared: Option<(u32, bool)>,
    key: Option<CalendarKey>,
    tree: Option<Rc<hyprv_ui::scene::Node>>,
    invalid: bool,
    was_animating: bool,
}
impl CalendarTree {
    fn release_hidden(&mut self) {
        self.tree = None;
        self.key = None;
        self.invalid = false;
        self.was_animating = false;
        // Keep the small first-frame material preparation key, not the last week/tree.
    }
    fn get(
        &mut self,
        panel: &mut panels::calendar::CalendarPanel,
        model: &panels::Model,
        abs: (f32, f32),
    ) -> hyprv_ui::scene::Node {
        let geometry = (panel.width, panel.height, model.output.scale, abs.0, abs.1);
        let minute = model.wall.and_utc().timestamp() / 60;
        let animated = panel.animating(model.now);
        let same = self.key.as_ref().is_some_and(|k| {
            k.0 == model.theme && k.1 == model.calendar && k.2 == minute && k.3 == geometry
        });
        if self.tree.is_none() || !same || self.invalid || animated || self.was_animating {
            self.tree = Some(Rc::new(panel.build(model, abs)));
            self.key = Some((model.theme, model.calendar.clone(), minute, geometry));
            self.invalid = false;
        }
        self.was_animating = animated;
        shared_tree(self.tree.as_ref().unwrap().clone())
    }
}
#[derive(Default)]
struct OutputOwners {
    bars: HashMap<String, bar::Controller>,
    panels: HashMap<String, Rc<RefCell<panels::Panels>>>,
    islands: HashMap<String, Rc<RefCell<hyprv_ui_island::Island>>>,
    brightness_preview: HashMap<String, (i32, Duration)>,
    brightness_probe: HashMap<String, Duration>,
    calendar_launch: HashMap<String, Duration>,
    island_trees: HashMap<String, Rc<RefCell<IslandTree>>>,
    calendar_trees: HashMap<String, Rc<RefCell<CalendarTree>>>,
}
impl OutputOwners {
    fn remove(&mut self, name: &str) {
        self.bars.remove(name);
        self.panels.remove(name);
        self.islands.remove(name);
        self.brightness_preview.remove(name);
        self.brightness_probe.remove(name);
        self.calendar_launch.remove(name);
        self.island_trees.remove(name);
        self.calendar_trees.remove(name);
    }
    fn singleton(&mut self, output: &str, kind: panels::PanelKind) {
        let target = self.panels[output].clone();
        for (name, source) in &self.panels {
            if name == output {
                continue;
            }
            let mut source = source.borrow_mut();
            let mut target = target.borrow_mut();
            match kind {
                panels::PanelKind::ControlPanel if source.control.is_mapped() => {
                    target.control = std::mem::take(&mut source.control)
                }
                panels::PanelKind::Stats if source.stats.is_mapped() => {
                    target.stats = std::mem::take(&mut source.stats)
                }
                panels::PanelKind::QuickAdjust if source.quick.mapped => {
                    target.quick = std::mem::take(&mut source.quick)
                }
                _ => {}
            }
        }
    }
    fn next_deadline(&self, now: Duration) -> Option<Duration> {
        self.bars
            .values()
            .filter_map(|c| c.next_deadline())
            .chain(
                self.islands
                    .values()
                    .filter_map(|i| i.borrow().next_deadline()),
            )
            .chain(
                self.panels
                    .values()
                    .filter_map(|p| p.borrow().next_deadline(now)),
            )
            .chain(self.brightness_preview.values().map(|v| v.1))
            .chain(self.brightness_probe.values().copied())
            .chain(self.calendar_launch.values().copied())
            .min()
    }
}
/// Qt's default timer-mode animation driver, as the full-screen references were captured
/// (`QSG_RENDER_LOOP=basic` under a virtual clock, parity/fullscreen/README.md): while any
/// animation runs, a 16 ms `Qt::PreciseTimer` ticks it and the scene shows the state of
/// the last tick; the timer starts when an animation starts from idle. Time only moves at
/// fixture ticks, so a capture between deadlines shows an older frame (control panel
/// open-t0060 is still blank, close-t0060 shows the t0050 state). Fixture replay only.
#[derive(Default)]
struct AnimationDriver {
    running: bool,
    pause_only: bool,
    deadline: Duration,
    time: Duration,
    wall_epoch: Duration,
    logical_epoch: Duration,
    last_wall: Duration,
}
const QT_ANIMATION_INTERVAL: Duration = Duration::from_millis(16);
impl AnimationDriver {
    fn advance(&mut self, wall: Duration, animating: bool, pause_only: bool) -> Duration {
        let whole_ms = |t: Duration| Duration::from_millis(t.as_millis() as u64);
        if !animating {
            self.running = false;
            self.time = whole_ms(wall);
        } else if !self.running {
            self.running = true;
            self.pause_only = pause_only;
            self.wall_epoch = wall;
            self.logical_epoch = whole_ms(wall);
            self.time = self.logical_epoch;
            self.deadline = wall + QT_ANIMATION_INTERVAL;
        } else {
            if self.pause_only != pause_only {
                // QUnifiedTimer::startAnimationDriver snapshots integer elapsed,
                // then QAnimationDriver starts a fresh QElapsedTimer. The fractional
                // epoch is discarded at this boundary, not at every fixture tick.
                // Component state observes the leaf starting on the previous sync.
                self.wall_epoch = self.last_wall;
                self.logical_epoch = self.time;
                self.deadline = self.last_wall + QT_ANIMATION_INTERVAL;
                self.pause_only = pause_only;
            }
            if wall >= self.deadline {
                self.time = self.logical_epoch + whole_ms(wall.saturating_sub(self.wall_epoch));
                self.deadline += QT_ANIMATION_INTERVAL;
                if self.deadline < wall {
                    self.deadline = wall + QT_ANIMATION_INTERVAL;
                }
            }
        }
        self.last_wall = wall;
        self.time
    }
}

pub struct Assembly {
    fixture: Option<crate::fixture::Fixture>,
    driver: AnimationDriver,
    pub hub: Arc<Hub>,
    outputs: HashMap<String, (WlOutput, bar::model::Output)>,
    popups: bar::popups::Popups,
    owners: OutputOwners,
    bar_expansion: crate::laptop::BarExpansion,
    island_agent_revisions: HashMap<String, u64>,
    mounts: HashMap<SurfaceId, Mount>,
    now: Duration,
    primary: Option<String>,
    dark: bool,
    no_exclusive: bool,
    quit: bool,
    pointer: (f32, f32),
    dirty: bool,
    animation_active: bool,
    stats_dirty: bool,
    scheduled_deadline: Option<Duration>,
    assets: Rc<panels::model::Assets>,
    position: PositionClock,
    vm_released: Option<u64>,
    startup: Option<panels::startup::StartupConfirm>,
    awaiting_startup: bool,
    bluetooth_show_unnamed: bool,
    input_epoch: bool,
    input_values: HashMap<String, i32>,
    art_url: String,
    art: Option<hyprv_ui::skia::Image>,
    tray_loader: crate::assets::TrayLoader,
    art_ready: hyprv_core::calloop::channel::Sender<crate::assets::Ready>,
}
impl Assembly {
    fn new(
        hub: Arc<Hub>,
        no_exclusive: bool,
        art_ready: hyprv_core::calloop::channel::Sender<crate::assets::Ready>,
        assets: Rc<panels::model::Assets>,
    ) -> Self {
        // Qt creates hidden popup Text objects during component construction.
        // Prepare their shared font styles at startup rather than on a click.
        let styles: Vec<_> = [500, 600, 700].into_iter().map(|weight| hyprv_ui::scene::Text {
            text: "M电池".into(),
            families: vec!["JetBrainsMono Nerd Font".into(), "HarmonyOS Sans SC".into()],
            size: 13., weight, elide: false, max_lines: None,
            render_type: hyprv_ui::scene::RenderType::NativeRendering,
            ..Default::default()
        }).collect();
        if let Err(error) = hyprv_ui::text::prepare_fonts(&styles) {
            eprintln!("hyprv-shell font preparation: {error}");
        }
        let startup = (!hub.is_mock() && hub.desktop.startup.state().approval_path.is_some())
            .then(|| panels::startup::StartupConfirm::new(false, Duration::ZERO));
        Self {
            hub,
            fixture: None,
            driver: AnimationDriver::default(),
            outputs: HashMap::new(),
            popups: Default::default(),
            mounts: HashMap::new(),
            now: Duration::ZERO,
            primary: None,
            dark: false,
            no_exclusive,
            quit: false,
            pointer: (0., 0.),
            dirty: true,
            animation_active: false,
            stats_dirty: false,
            scheduled_deadline: None,
            owners: OutputOwners::default(),
            bar_expansion: Default::default(),
            island_agent_revisions: Default::default(),
            assets,
            position: Default::default(),
            vm_released: None,
            startup,
            awaiting_startup: false,
            bluetooth_show_unnamed: true,
            input_epoch: false,
            input_values: HashMap::new(),
            art_url: String::new(),
            art: None,
            art_ready,
            tray_loader: Default::default(),
        }
    }
    fn tray_assets(&self, scale: f32) -> bar::model::Assets {
        let images = self
            .hub
            .tray
            .topic
            .snapshot()
            .value
            .items
            .iter()
            .filter_map(|item| {
                self.tray_loader.get(item, scale, &self.hub.runtime, &self.art_ready)
                    .map(|image| (item.key.clone(), image))
            })
            .collect();
        bar::model::Assets { images }
    }
    fn bar_model(&self, name: &str) -> bar::Model {
        let _timing = hyprv_ui::diagnostics::scope("shell.bar_model");
        if let Some(fixture) = &self.fixture {
            let mut m = fixture.bar(&self.outputs[name].1);
            m.is_primary = self.primary.as_deref() == Some(name);
            m.profile = hyprv_core::profile::profile();
            m.expanded = self.bar_expansion.expanded(name);
            m.clock = wall_clock(fixture.wall());
            return m;
        }
        bar::Model {
            output: self.outputs[name].1.clone(),
            profile: hyprv_core::profile::profile(),
            expanded: self.bar_expansion.expanded(name),
            theme: bar::Theme { dark: self.dark },
            clock: wall_clock(chrono::Local::now().naive_local()),
            now: self.now,
            monotonic_us: hyprv_svc_media::monotonic_us(),
            desktop: self.hub.hyprland.state.snapshot().value,
            stats: self.hub.stats.state.snapshot().value,
            power: self.hub.power.state.snapshot().value,
            battery: self.hub.battery.state.snapshot().value,
            audio: self.hub.audio.topic.snapshot().value,
            media: self.hub.mpris.topic.snapshot().value,
            network: self.hub.network.state.snapshot().value,
            tray: crate::mapping::tray(self.hub.tray.topic.snapshot().value),
            notifications: self.hub.notifications.topic.snapshot().value,
            assets: self.tray_assets(self.outputs[name].1.scale),
            is_primary: self.primary.as_deref() == Some(name),
            ..Default::default()
        }
    }
    /// Any component animation in flight (drives [`AnimationDriver`]).
    fn animations_running(&self) -> bool {
        self.popups.animating()
            || self.owners.bars.values().any(|c| c.animating())
            || self.owners.islands.values().any(|i| i.borrow().animating(self.now))
            || self.owners.panels.iter().any(|(name, p)| {
                self.outputs.contains_key(name) && {
                    let p = p.borrow();
                    p.animating(&self.panel_model(name)) || p.calendar.animating(self.now)
                }
            })
    }
    /// QML Timers and a reveal's initial PauseAnimation stop the continuous
    /// QUnifiedTimer driver. Other visible leaf animations keep it running.
    fn fixture_pause_only(&self) -> bool {
        if self.popups.animating()
            || self.owners.bars.values().any(|c| c.animating())
            || self.owners.islands.values().any(|i| i.borrow().animating(self.now))
        { return false; }
        let mut waiting = false;
        for (name, panel) in &self.owners.panels {
            let p = panel.borrow();
            if p.wifi.animating(self.now) || p.quick.animating(self.now)
                || p.stats.animating() || p.calendar.animating(self.now)
            { return false; }
            if p.control.animating(self.now, &self.panel_model(name)) {
                let m = &p.control.m;
                if m.pending || (!m.closing && m.reveal.is_running() && m.reveal.values().height == 0.) {
                    waiting = true;
                } else { return false; }
            }
        }
        waiting
    }
    fn panel_model(&self, name: &str) -> panels::Model {
        let _timing = hyprv_ui::diagnostics::scope("shell.panel_model");
        if let Some(fixture) = &self.fixture {
            let mut model = fixture.panels(&self.outputs[name].1);
            model.bluetooth_show_unnamed = self.bluetooth_show_unnamed;
            if let Some(value) = self.owners.brightness_preview.get(name) {
                model.controls.brightness_percent = value.0;
            }
            model.calendar.bridge = self
                .owners
                .bars
                .get(name)
                .is_some_and(|c| c.model.calendar_presented);
            self.laptop_panel_geometry(name, &mut model);
            return model;
        }
        let mut model =
            crate::mapping::panels(&self.hub, &self.outputs[name].1, self.dark, self.now);
        if let Some(value) = self.owners.brightness_preview.get(name) {
            model.controls.brightness_percent = value.0;
        }
        model.assets = self.assets.clone();
        model.media.art = self.art.clone();
        model.bluetooth_show_unnamed = self.bluetooth_show_unnamed;
        model.calendar.bridge = self
            .owners
            .bars
            .get(name)
            .is_some_and(|c| c.model.calendar_presented);
        self.laptop_panel_geometry(name, &mut model);
        model
    }
    fn laptop_panel_geometry(&self,name:&str,model:&mut panels::Model) {
        if crate::laptop::active() {
            model.controls.fan_available=false;
            model.shell.bar_exclusive_zone=self.owners.bars.get(name).map_or(25.,|c|(25.+33.*c.model.reveal).round() as f32);
        }
    }
    fn island_inputs(&self, name: &str) -> hyprv_ui_island::Inputs {
        let _timing = hyprv_ui::diagnostics::scope("shell.island_inputs");
        if let Some(fixture) = &self.fixture {
            let mut input = fixture.island();
            if let Some(c) = self.owners.bars.get(name) {
                input.calendar_open = c.model.calendar_presented;
                input.calendar_progress = c.model.calendar_progress as f32;
            }
            return input;
        }
        let media = self.hub.mpris.topic.snapshot().value;
        let (calendar_open, calendar_progress) =
            self.owners.bars.get(name).map_or((false, 0.), |c| {
                (c.model.calendar_presented, c.model.calendar_progress as f32)
            });
        hyprv_ui_island::Inputs {
            dark: self.dark,
            now_hh_mm: chrono::Local::now().format("%H:%M").to_string(),
            media: hyprv_ui_island::Media::from_mpris(&media, self.position.seconds),
            spectrum: hyprv_ui_island::model::spectrum_values(
                &self.hub.spectrum.topic.snapshot().value,
            ),
            agents: hyprv_ui_island::model::agents(
                &self.hub.desktop.agent_island.state(),
                chrono::Local::now().timestamp_millis() as f64 / 1000.,
            ),
            calendar_open,
            calendar_progress,
            art: self.art.clone(),
        }
    }
    fn island_action(&mut self, action: hyprv_ui_island::IslandAction, output: &str) {
        use hyprv_svc_media::mpris::MprisAction as M;
        use hyprv_ui_island::IslandAction as A;
        let media = match action {
            A::Previous => Some(M::Previous),
            A::PlayPause => Some(M::PlayPause),
            A::Next => Some(M::Next),
            A::Seek(s) => Some(M::Seek(s)),
            A::FocusMediaApp => Some(M::Raise),
            _ => None,
        };
        if let Some(action) = media {
            if matches!(action, M::PlayPause) {
                if let Some(island) = self.owners.islands.get(output) {
                    let mut i = island.borrow_mut();
                    let mut inputs = i.inputs().clone();
                    inputs.media.playing = !inputs.media.playing;
                    i.set_inputs(inputs, self.now);
                }
            }
            self.hub.bar_action(bar::Action::Media(action));
            return;
        }
        match action {
            A::ToggleCalendar => self.action(
                bar::Action::Host(bar::model::HostAction::ToggleCalendar),
                output,
            ),
            A::Lock => self
                .hub
                .launch(hyprv_svc_desktop::applications::LaunchTarget::Lock),
            A::Power => self
                .hub
                .launch(hyprv_svc_desktop::applications::LaunchTarget::Logout),
            A::FocusAgent(id) if !self.hub.read_only => {
                self.hub.agent_focus(id);
            }
            _ => {}
        }
    }
    fn refresh(&mut self) {
        let _timing = hyprv_ui::diagnostics::scope("shell.refresh");
        self.dark = self
            .fixture
            .as_ref()
            .map_or_else(|| self.hub.desktop.theme.state().dark_mode, |f| f.dark);
        let url = if self.hub.is_mock() {
            String::new()
        } else {
            self.hub.mpris.topic.snapshot().value.art_url
        };
        if self.art_url != url {
            self.art_url = url.clone();
            self.art = None;
            if !url.is_empty() {
                let sender = self.art_ready.clone();
                self.hub.runtime.spawn(async move {
                    let bytes = crate::assets::art_bytes(&url).await;
                    let _ = sender.send(crate::assets::Ready::Art(crate::assets::Art { url, bytes }));
                });
            }
        }
        if self.fixture.is_none() && !self.hub.is_mock() {
            self.tray_loader.retain(&self.hub.tray.topic.snapshot().value.items,
                self.outputs.values().map(|(_, output)| output.scale));
        }
        let names: Vec<_> = self.outputs.keys().cloned().collect();
        for name in names {
            let mut next = self.bar_model(&name);
            // Provider currently exposes only a bool fullscreen flag; only the active
            // workspace on this output may hide its bar.
            next.calendar_child_opening = self.owners.calendar_launch.contains_key(&name);
            next.calendar_child_active = next
                .desktop
                .windows
                .iter()
                .any(|w| w.class == "com.danklinux.dankcalendar");
            next.fullscreen |= next.desktop.windows.iter().any(|w| {
                w.fullscreen
                    && next
                        .desktop
                        .monitors
                        .iter()
                        .any(|m| m.id == w.monitor && m.name == name)
            });
            self.owners
                .panels
                .entry(name.clone())
                .or_insert_with(|| Rc::new(RefCell::new(panels::Panels::default())));
            // Store actors keep authoritative data while compact. Do not resample
            // hidden agent progress/waves; refresh all inputs before first reveal.
            let agent_revision = self.fixture.as_ref().map_or_else(|| self.hub.desktop.agent_island.topic.snapshot().revision, |f| f.revision);
            let inputs = if crate::laptop::active() && !next.expanded && self.island_agent_revisions.get(&name) == Some(&agent_revision) {
                self.owners.islands.get(&name).filter(|i| i.borrow().bar_hidden())
                    .map(|i| i.borrow().inputs().clone()).unwrap_or_else(||self.island_inputs(&name))
            } else {self.island_inputs(&name)};
            self.island_agent_revisions.insert(name.clone(), agent_revision);
            let island = self
                .owners
                .islands
                .entry(name.clone())
                .or_insert_with(|| {
                    Rc::new(RefCell::new(hyprv_ui_island::Island::new(
                        inputs.clone(),
                        next.output.width,
                        self.now,
                    )))
                })
                .clone();
            island
                .borrow_mut()
                .set_host_width(next.output.width, self.now);
            island.borrow_mut().set_inputs(inputs, self.now);
            if crate::laptop::active() { let reveal = self.owners.bars.get(&name).map_or(if next.expanded {1.} else {0.}, |c| c.model.reveal); island.borrow_mut().set_bar_reveal(reveal, self.now); }
            let content = island.clone();
            let cache = self
                .owners
                .island_trees
                .entry(name.clone())
                .or_default()
                .clone();
            next.slots.island = Some(Rc::new(move || cache.borrow_mut().get(&content.borrow())));
            let geometry = island.borrow().geometry();
            next.island = bar::model::IslandGeometry {
                width: geometry.width,
                height: geometry.height,
                attached_center_offset: geometry.attached_center_offset,
                expanded: geometry.expanded,
            };
            if let Some(controller) = self.owners.bars.get_mut(&name) {
                next.calendar_open = controller.model.calendar_open;
                next.calendar_presented = controller.model.calendar_presented;

                controller.sync(next, self.now);
            } else {
                self.owners.bars.insert(name, bar::Controller::new(next));
            }
        }
        self.dirty = false;
    }
    fn action(&mut self, action: bar::Action, output: &str) {
        use bar::model::HostAction as H;
        match action {
            bar::Action::Host(host) => match host {
                H::ToggleControlPanel { anchor } => {
                    self.owners
                        .singleton(output, panels::PanelKind::ControlPanel);
                    let actions = self.owners.panels[output].borrow_mut().control.toggle_for(
                        panels::model::Anchor {
                            x: anchor.x,
                            y: anchor.y,
                            width: anchor.width,
                            height: anchor.height,
                        },
                        output,
                        self.now,
                    );
                    for a in actions {
                        self.panel_action(a, output)
                    }
                }
                H::ToggleCalendar => {
                    if let Some(c) = self.owners.bars.get_mut(output) {
                        c.model.calendar_open = !c.model.calendar_open;
                        c.motion.sync(&c.model, self.now);
                    }
                }
                H::CloseCalendar { force } => {
                    if let Some(c) = self.owners.bars.get_mut(output) {
                        if force || !c.model.calendar_child_active {
                            c.model.calendar_open = false;
                            c.motion.sync(&c.model, self.now);
                        }
                    }
                }
                H::ToggleWifi { anchor } => {
                    let model = self.panel_model(output);
                    let panel = self.owners.panels[output].clone();
                    panel.borrow_mut().wifi.set_anchor(
                        panels::model::Anchor {
                            x: anchor.x,
                            y: anchor.y,
                            width: anchor.width,
                            height: anchor.height,
                        },
                        output,
                    );
                    let actions = panel.borrow_mut().wifi.toggle(&model);
                    for action in actions {
                        self.panel_action(action, output)
                    }
                }
                H::ToggleStats { anchor } => {
                    self.owners.singleton(output, panels::PanelKind::Stats);
                    let actions = self.owners.panels[output].borrow_mut().stats.toggle_for(
                        panels::model::Anchor {
                            x: anchor.x,
                            y: anchor.y,
                            width: anchor.width,
                            height: anchor.height,
                        },
                        output,
                        self.now,
                    );
                    for action in actions {
                        self.panel_action(action, output)
                    }
                }
                H::ToggleBar => {
                    if crate::laptop::active() && self.bar_expansion.toggle(output,crate::laptop::wall_ms()) { self.dirty=true; }
                }
                H::ShowVolume => {
                    self.owners
                        .singleton(output, panels::PanelKind::QuickAdjust);
                    let actions = self.owners.panels[output]
                        .borrow_mut()
                        .quick
                        .show(panels::quick_adjust::Mode::Volume, self.now);
                    for action in actions {
                        self.panel_action(action, output)
                    }
                }
                other => self.hub.bar_host(other),
            },
            other => self.hub.bar_action(other),
        }
        self.dirty = true;
    }
    fn panel_action(&mut self, action: panels::Action, output: &str) {
        // QML valueChanged emits once per distinct value, even if a component
        // reports the final value on both press/drag and release.
        let preview = match &action {
            panels::Action::SetVolume(v) => Some(("volume".to_string(), *v)),
            panels::Action::BrightnessSet(v) => Some(("brightness".to_string(), *v)),
            panels::Action::FanLimit { channel, percent } => {
                Some((format!("fan-{channel:?}"), *percent))
            }
            _ => None,
        };
        if self.input_epoch {
            if let Some((key, value)) = preview {
                let key = format!("{output}/{key}");
                if self.input_values.insert(key, value) == Some(value) {
                    return;
                }
            }
        }

        if crate::laptop::active() && matches!(action, panels::Action::BrightnessSet(_)) {
            self.owners.brightness_probe.insert(output.into(), self.now + Duration::from_millis(90));
        }
        if matches!(
            action,
            panels::Action::CalendarOpenEvent { .. } | panels::Action::CalendarNewEvent
        ) {
            self.owners
                .calendar_launch
                .insert(output.into(), self.now + Duration::from_millis(2600));
            if let Some(c) = self.owners.bars.get_mut(output) {
                c.model.calendar_child_opening = true;
            }
        }

        if matches!(
            action,
            panels::Action::BrightnessSet(_) | panels::Action::BrightnessFinish(_)
        ) {
            self.owners.brightness_preview.remove(output);
        }
        if let panels::Action::Host(host) = action {
            match host {
                panels::HostAction::StartupDecision(approved) => {
                    let hub = self.hub.clone();
                    if !hub.read_only {
                        let changed = hub.desktop.startup.clone();
                        hub.runtime.spawn(async move {
                            let _ = changed
                                .request(hyprv_svc_desktop::startup::StartupAction::Decide(
                                    approved,
                                ))
                                .await;
                        });
                    }
                    self.startup = None;
                    if self.hub.read_only {
                        self.quit = true
                    } else {
                        self.awaiting_startup = true;
                    }
                }
                panels::HostAction::SetBluetoothShowUnnamed(value) => {
                    self.bluetooth_show_unnamed = value
                }
                panels::HostAction::CalendarPointerInside(value) => {
                    if let Some(c) = self.owners.bars.get_mut(output) {
                        c.motion.pointer_calendar(value, &c.model, self.now);
                    }
                }
                panels::HostAction::CloseCalendar { force } => self.action(
                    bar::Action::Host(bar::model::HostAction::CloseCalendar { force }),
                    output,
                ),
                panels::HostAction::OpenWifiPanel => self.action(
                    bar::Action::Host(bar::model::HostAction::ToggleWifi {
                        anchor: Rect::new(self.outputs[output].1.width - 10., 10., 0., 38.),
                    }),
                    output,
                ),
                panels::HostAction::ExpandCalendar => {
                    self.action(
                        bar::Action::Host(bar::model::HostAction::CloseCalendar { force: true }),
                        output,
                    );
                    self.hub
                        .panel_action(panels::Action::CalendarShowFullApp, output);
                }
                _ => self.hub.panel_host(host),
            }
        } else {
            self.hub.panel_action(action, output)
        }
        self.dirty = true;
    }
    pub fn dispatch(&mut self, request: Request) -> IpcResponse {
        if let Err(error) = request.validate() {
            return IpcResponse::error(error);
        }
        if request.target == "shell" {
            self.quit = true;
            return IpcResponse::success("");
        }
        let Some(output) = self.primary.clone() else {
            return IpcResponse::error("no outputs");
        };
        match (request.target.as_str(), request.function.as_str()) {
            ("calendar", "toggle") => self.action(
                bar::Action::Host(bar::model::HostAction::ToggleCalendar),
                &output,
            ),
            ("controlPanel", "toggle") => {
                self.owners
                    .singleton(&output, panels::PanelKind::ControlPanel);
                let model = self.panel_model(&output);
                let actions = self.owners.panels[&output]
                    .borrow_mut()
                    .ipc_control_panel_toggle(&output, &model);
                for action in actions {
                    self.panel_action(action, &output)
                }
            }
            ("bar", "toggle") => {
                if crate::laptop::active() && self.bar_expansion.toggle(&request.args[0],crate::laptop::wall_ms()) { self.dirty = true; }
            }
            ("bar", "layoutStatus") => {
                return IpcResponse::success(self.owners.bars.get(&request.args[0]).map_or_else(
                    || "null".into(),
                    |c| bar::ipc::layout_status(Some(&c.model)),
                ))
            }
            ("bar", "inputStatus") => {
                return IpcResponse::success(bar::ipc::input_status(
                    self.owners.bars.get(&output).map(|c| &c.model),
                ))
            }
            ("bar", "dataStatus") => {
                if hyprv_ui::diagnostics::enabled() {hyprv_ui::diagnostics::clear();}
                let status = bar::ipc::data_status(
                    &self.owners.bars[&output].model,
                    bar::ipc::Diagnostics {
                        spectrum: !self.hub.is_mock()
                            && self.hub.spectrum.topic.snapshot().value.running,
                        panel_sampling: self.owners.panels[&output].borrow().control.is_mapped(),
                        battery_detail_sampling: !self.hub.is_mock()
                            && self.hub.battery.state.subscriber_count() > 0,
                        any_bar_expanded: Some(self.owners.bars.values().any(|c| c.model.expanded)),
                        detailed_stats: Some(self.owners.bars.values().any(|c| c.model.expanded) || self.owners.panels.values().any(|p| p.borrow().stats.wants_samples())),
                        wifi_scanning: self.owners.panels.values().any(|p| {let p=p.borrow(); if crate::laptop::active() {p.wifi.popup_visible} else {p.wifi.is_mapped()}}),
                        notifications_alive: Some(self.owners.bars[&output].model.notifications.available),
                        agent_watcher_alive: Some(!self.hub.is_mock()),
                    },
                );
                if crate::laptop::active() {
                    let mut value: serde_json::Value = serde_json::from_str(&status).expect("bar diagnostics JSON");
                    value["mediaCounters"] = serde_json::to_value(self.hub.media_counters.snapshot()).expect("media counters JSON");
                    return IpcResponse::success(value.to_string());
                }
                return IpcResponse::success(status);
            }
            ("quickAdjust", function) => {
                self.owners
                    .singleton(&output, panels::PanelKind::QuickAdjust);
                let model = self.panel_model(&output);
                let (actions, preview) = quick_adjust_ipc(
                    &mut self.owners.panels[&output].borrow_mut(),
                    function,
                    &request.args,
                    &model,
                );
                for action in actions {
                    self.panel_action(action, &output)
                }
                if let Some(value) = preview {
                    self.owners.brightness_preview.insert(
                        output.clone(),
                        (value, self.now + Duration::from_millis(90)),
                    );
                }
            }
            _ => return IpcResponse::error("unknown UI target"),
        }
        self.dirty = true;
        IpcResponse::success("")
    }
    fn compute_next_deadline(&self) -> Option<Duration> {
        self.owners.next_deadline(self.now).into_iter()
            .chain(self.popups.next_deadline())
            .chain(self.position.deadline)
            .chain(self.startup.as_ref().and_then(|s| s.next_deadline()))
            .min()
    }
    fn sync_stats(&mut self, engine: &mut Engine) {
        let _work = hyprv_ui::diagnostics::scope("shell.stats_only");
        let stats = self.hub.stats.state.snapshot().value;
        for (name, controller) in &mut self.owners.bars {
            let old_labels=stats_labels(&controller.model);
            let old_style=stats_style(&controller.model);
            controller.model.stats = stats.clone();
            controller.model.now = self.now;
            for (id, mount) in &mut self.mounts {
                if controller.model.compact_strip_visible() && matches!(&mount.role,Role::Corner(output,_) if output == name) {
                    if let Some(scene) = engine.scene(*id) {
                        let m=&controller.model;
                        let mut node=bar::surfaces::corner_window(m.output.width,m.output.height,m.output.scale,m.top_corner_offset(),0.);
                        node.add(bar::surfaces::compact_strip(m).node);
                        bar::view::reconcile(scene,scene.root().id(),node);
                    }
                    continue;
                }
                if mount.role != Role::Bar(name.clone()) || mount.config.input_only { continue; }
                if let Some(scene) = engine.scene(*id) {
                    let model = &controller.model;
                    if let Some(edits)=patch_stats_text(scene,&old_labels,old_style,model) {
                        hyprv_ui::diagnostics::count_by("stats.edited_nodes",edits as u64);
                        hyprv_ui::diagnostics::count("stats.text_fast_path");
                        // Private component signatures describe the last component
                        // update. A later value may cycle back to that signature;
                        // never let it retain text we patched to a different value.
                        if edits>0 {mount.bar_view=None;}
                        continue;
                    }
                    let changes = mount.bar_view.get_or_insert_with(Default::default).update(scene, model, |tree| {
                        panels::qrect::convert(tree, (0., 0.), model.output.scale);
                    });
                    hyprv_ui::diagnostics::count_by("stats.built_modules", changes.built_modules as u64);
                    hyprv_ui::diagnostics::count_by("stats.edited_nodes", changes.edited_nodes as u64);
                }
            }
        }
        hyprv_ui::diagnostics::mark("stats.model.end","");
    }
    fn sync(&mut self, engine: &mut Engine) {
        let _timing = hyprv_ui::diagnostics::scope("shell.sync");
        self.now = match self.fixture.as_ref() {
            None => engine.now(),
            Some(f) => {
                // The reference vclock rounds to microseconds before Qt reads it.
                let raw = Duration::from_micros((f.time * 1000.).round() as u64);
                let animating = self.animations_running();
                let pause_only = self.fixture_pause_only();
                let now = self.driver.advance(raw, animating, pause_only);
                let f = self.fixture.as_mut().unwrap();
                f.clock = if now == raw { f.time } else { now.as_millis() as f64 };
                now
            }
        };
        // A frame acknowledgement is not a model change. Keep protocol dispatch
        // cheap when no input/provider/output change or component deadline is due.
        // Missing native surfaces must still go through reconciliation (SHELL-2).
        if self.fixture.is_none() && !self.dirty && !self.animation_active
            && self.scheduled_deadline.is_none_or(|at| at > self.now)
            && engine.surface_count() == self.mounts.len()
        {
            if !self.stats_dirty {
                hyprv_ui::diagnostics::count("shell.sync.skipped");
                return;
            }
            if !self.hub.is_mock() && self.mounts.values().all(|m| matches!(m.role, Role::Bar(_) | Role::Corner(_, _))) {
                self.stats_dirty = false;
                self.sync_stats(engine);
                return;
            }
        }
        self.stats_dirty = false;
        let _work = hyprv_ui::diagnostics::scope("shell.sync.work");
        if self.awaiting_startup && self.hub.desktop.startup.state().decided {
            self.quit = true;
        }
        let launches = self.owners.calendar_launch.len();
        self.owners.calendar_launch.retain(|name, until| {
            *until > self.now
                && !self
                    .owners
                    .bars
                    .get(name)
                    .is_some_and(|c| c.model.calendar_child_active)
        });
        self.dirty |= launches != self.owners.calendar_launch.len();
        let probe_due = self.owners.brightness_probe.values().any(|d| *d <= self.now);
        self.owners.brightness_probe.retain(|_, d| *d > self.now);
        if probe_due { self.hub.brightness_probe(); }
        let previews = self.owners.brightness_preview.len();
        self.owners.brightness_preview.retain(|_, v| v.1 > self.now);
        self.dirty |= previews != self.owners.brightness_preview.len();
        if self.quit {
            engine.quit();
            return;
        }
        let expanded = self
            .owners
            .islands
            .values()
            .any(|i| {let i=i.borrow();i.derived().music_expanded && !i.bar_hidden()});
        let sample = self.position.step(
            self.now,
            expanded,
            if self.hub.is_mock() {
                self.fixture
                    .as_ref()
                    .map_or(0., |f| f.island().media.position_s)
            } else {
                self.hub
                    .mpris
                    .topic
                    .snapshot()
                    .value
                    .position_seconds(hyprv_svc_media::monotonic_us())
            },
        );
        self.dirty |= sample;
        if self.dirty {
            self.refresh()
        }
        let vm = if self.hub.is_mock() {
            Default::default()
        } else {
            self.hub.vm.state.snapshot().value
        };
        if !vm.busy {
            self.vm_released = None;
        }
        let releasing = vm.phase == Some(hyprv_svc_sys::vm::Phase::AwaitingUiRelease);
        if releasing {
            self.popups = bar::popups::Popups::default();
            for (name, panel) in &self.owners.panels {
                let model = self.panel_model(name);
                let mut panel = panel.borrow_mut();
                panel.control.close(self.now);
                panel.wifi.close(&model);
                panel.stats.close(self.now);
                panel.quick.mapped = false;
            }
            for controller in self.owners.bars.values_mut() {
                controller.model.calendar_open = false;
                controller.motion.sync(&controller.model, self.now);
            }
        }
        let mut desired: Vec<(Role, LayerConfig, Vec<Rect>, Scene)> = Vec::new();
        let names: Vec<_> = self.owners.bars.keys().cloned().collect();
        for name in names {
            let island = self.owners.islands[&name].clone();
            if crate::laptop::active() { island.borrow_mut().set_bar_reveal(self.owners.bars[&name].model.reveal,self.now); }
            island.borrow_mut().set_calendar(
                self.owners.bars[&name].model.calendar_presented,
                self.owners.bars[&name].model.calendar_progress as f32,
                self.now,
            );
            let frame_due = {
                let i = island.borrow();
                i.animating(self.now) || i.next_deadline().is_some_and(|d| self.now >= d)
            };
            if frame_due {
                island.borrow_mut().frame(self.now);
            }
            let geometry = island.borrow().geometry();
            let mut panel_model = self.panel_model(&name);
            panel_model.calendar.bridge = self.owners.bars[&name].model.calendar_presented;
            let panel = self.owners.panels[&name].clone();
            let size = panels::calendar::CalendarPanel::host_size(
                self.outputs[&name].1.width,
                self.owners.bars[&name].model.surface_height(),
            );
            panel.borrow_mut().calendar.width = size.0;
            panel.borrow_mut().calendar.height = size.1;
            let calendar_model = panel_model.clone();
            let calendar = panel.clone();
            let scale = panel_model.output.scale;
            let abs = (
                ((self.outputs[&name].1.width - size.0) / 2. * scale).round() / scale,
                (10. * scale).round() / scale,
            );
            let calendar_cache = self
                .owners
                .calendar_trees
                .entry(name.clone())
                .or_default()
                .clone();
            // Prime the header and first timeline row, not a full day's text or
            // provider event summaries. These are the only styles needed as the
            // calendar first reveals; warmup size is independent of event count.
            let prepare_key = (scale.to_bits(), self.dark);
            if self.fixture.is_none() && !self.hub.is_mock()
                && (!crate::laptop::active() || self.owners.bars[&name].model.calendar_presented)
                && !(hyprv_ui::diagnostics::enabled()
                    && std::env::var_os("HYPRV_SKIP_CALENDAR_PREPARE").is_some())
                && calendar_cache.borrow().prepared != Some(prepare_key)
            {
                let _timing = hyprv_ui::diagnostics::scope("shell.calendar_prepare");
                let warm_size = (
                    size.0,
                    panels::calendar::TOOLBAR_H
                        + panels::calendar::DAY_HEADER_H
                        + panels::calendar::HOUR,
                );
                let mut warm_model = calendar_model.clone();
                warm_model.calendar.events.clear();
                let mut warm = panels::calendar::CalendarPanel::new(warm_size.0, warm_size.1);
                let mut node = warm.build(&warm_model, abs);
                node.native_materials = Some(true);
                if let Err(error) = engine.prepare_scene(&Scene::new(node), warm_size, scale.into()) {
                    eprintln!("hyprv-shell calendar preparation: {error}");
                }
                calendar_cache.borrow_mut().prepared = Some(prepare_key);
            }
            if !self.owners.bars[&name].model.calendar_presented {
                calendar_cache.borrow_mut().release_hidden();
            }
            let c = self.owners.bars.get_mut(&name).unwrap();
            c.model.now = self.now;
            c.model.island = bar::model::IslandGeometry {
                width: geometry.width,
                height: geometry.height,
                attached_center_offset: geometry.attached_center_offset,
                expanded: geometry.expanded,
            };
            c.model.slots.calendar = Some(Rc::new(move || {
                calendar_cache.borrow_mut().get(
                    &mut calendar.borrow_mut().calendar,
                    &calendar_model,
                    abs,
                )
            }));
            c.motion.sync(&c.model, self.now);
            let mut actions = c.motion.advance(&mut c.model, self.now);
            actions.extend(c.input.advance(self.now));
            c.input.tooltip(&mut c.model,self.now);
            for action in actions {
                self.action(action, &name);
            }
            let c = &self.owners.bars[&name];
            if c.model.reveal > 0. {
                if let (Some(card),Some(node))=(bar::tooltip::rect(&c.model),bar::tooltip::build(&c.model,&c.model.theme)) {
                    let crop=popup_crop(card,c.model.output.scale,24.);
                    let mut node=node;
                    panels::qrect::convert(&mut node,(0.,0.),c.model.output.scale);
                    let mut scene=Scene::new(node);
                    position_crop(&mut scene,crop,c.model.output.scale);
                    let mut config=bar::surfaces::bar(&c.model).layer_config();
                    config.namespace="shell:hyprv-cpu-temperature-tooltip".into();
                    config.output=Some(self.outputs[&name].0.clone());
                    config.layer=hyprv_ui::wayland::Layer::Overlay;
                    config.exclusive_zone = -1;
                    config.keyboard=hyprv_ui::wayland::KeyboardInteractivity::None;
                    let (config,_)=cropped_config(config,crop,None);
                    desired.push((Role::Tooltip(name.clone()),config,vec![],scene));
                }
            }
            let mut requirement = bar::surfaces::bar(&c.model);
            if self.no_exclusive {
                requirement.exclusive_zone = 0;
            }
            let mut config = requirement.layer_config();
            // Keep the layer role/exclusive zone, but no full-size transparent GL
            // target when the laptop contentRoot is settled outside the viewport.
            config.input_only = c.model.laptop() && c.model.reveal <= 0. && !c.animating();
            config.output = Some(self.outputs[&name].0.clone());
            desired.push((
                Role::Bar(name.clone()),
                config,
                requirement.input_regions,
                if (c.model.laptop() && c.model.reveal <= 0. && !c.animating()) || self
                    .mounts
                    .iter()
                    .any(|(id, m)| m.role == Role::Bar(name.clone()) && engine.scene(*id).is_some())
                {
                    Scene::new(hyprv_ui::scene::Node::new(
                        Rect::default(),
                        hyprv_ui::scene::Content::Group,
                    ))
                } else {
                    Scene::new(bar_tree(&c.model))
                },
            ));
            for (i, (mut config, mask, node)) in bar::surfaces::corners(&c.model).into_iter()
                .flat_map(|(requirement, node)| {
                    let mask = requirement.input_regions.clone();
                    corner_surfaces(requirement.layer_config(), node, c.model.output.scale).into_iter().map(move |(config,node)| (config,mask.clone(),node))
                })
                .enumerate()
            {
                config.output = Some(self.outputs[&name].0.clone());
                desired.push((
                    Role::Corner(name.clone(), i),
                    config,
                    mask,
                    Scene::new(node),
                ));
            }
        }
        if let Some(name) = self.primary.clone() {
            let model = self.owners.bars[&name].model.clone();
            if [
                bar::popups::Kind::Menu,
                bar::popups::Kind::Overflow,
                bar::popups::Kind::Battery,
            ]
            .into_iter()
            .any(|k| self.popups.surface(k).mapped)
            {
                let actions = self.popups.update(&model, self.now);
                for a in actions {
                    self.action(a, &name)
                }
            }
            for (kind, requirement) in self.popups.requirements(&model) {
                if requirement.mapped {
                    let role = Role::BarPopup(kind);
                    let output = self
                        .outputs
                        .get(&requirement.output)
                        .map_or(&self.outputs[&name].1, |o| &o.1);
                    let po = panels::model::Output {
                        name: output.name.clone(),
                        width: output.width,
                        height: output.height,
                        scale: output.scale,
                    };
                    let existing = self
                        .mounts
                        .iter()
                        .find(|(_, m)| m.role == role)
                        .map(|(id, _)| *id);
                    let _timing = hyprv_ui::diagnostics::scope("shell.popup_scene");
                    let mut scene =
                        if let Some(mut scene) = existing.and_then(|id| engine.take_scene(id)) {
                            if let Some(root) = scene.root().children.first().map(|n| n.id()) {
                                bar::view::reconcile(
                                    &mut scene,
                                    root,
                                    self.popups.build(&model, &model.theme, kind),
                                );
                            }
                            scene
                        } else {
                            wrap_surface(self.popups.build(&model, &model.theme, kind), &po)
                        };
                    let crop = popup_crop(self.popups.card_rect(&model, kind), output.scale, 24.);
                    position_crop(&mut scene, crop, output.scale);
                    let mut base = requirement.layer_config();
                    base.output = self.outputs.get(&po.name).map(|o| o.0.clone());
                    let (config, mask) = cropped_config(base.clone(), crop, None);
                    desired.push(dismiss_surface(role.clone(), base, &po, crop));
                    desired.push((role, config, mask, scene));
                }
            }
        }
        for name in self.outputs.keys().cloned().collect::<Vec<_>>() {
            let model = self.panel_model(&name);
            let panels = self.owners.panels[&name].clone();
            let actions = panels.borrow_mut().tick(&model);
            for action in actions {
                self.panel_action(action, &name)
            }
            let specs = panels.borrow().surfaces(&model);
            for (kind, spec) in specs {
                let role = Role::Panel(name.clone(), kind);
                let existing = self
                    .mounts
                    .iter()
                    .find(|(_, m)| m.role == role)
                    .map(|(id, _)| *id);
                let mut scene =
                    if let Some(mut scene) = existing.and_then(|id| engine.take_scene(id)) {
                        panels.borrow_mut().update(kind, &mut scene, &model);
                        scene
                    } else {
                        let content = panels.borrow_mut().build(kind, &model);
                        wrap_surface(content, &model.output)
                    };
                let card = surface_card(&scene).unwrap_or(Rect::new(
                    0.,
                    0.,
                    model.output.width,
                    model.output.height,
                ));
                let crop = popup_crop(card, model.output.scale, 24.);
                position_crop(&mut scene, crop, model.output.scale);
                let base = panel_config(&spec, &self.outputs);
                let (config, mask) = cropped_config(base.clone(), crop, spec.input_region);
                if spec.input_region.is_none() {
                    desired.push(dismiss_surface(role.clone(), base, &model.output, crop));
                }
                desired.push((role, config, mask, scene));
            }
        }
        if let Some(startup) = &mut self.startup {
            startup.tick(self.now);
            let spec = startup.surface();
            let config = panel_config(&spec, &self.outputs);
            let scale = self
                .primary
                .as_ref()
                .map_or(1., |name| self.outputs[name].1.scale);
            let mask = vec![Rect::new(
                0.,
                0.,
                config.size.0 as f32,
                config.size.1 as f32,
            )];
            desired.push((
                Role::Startup,
                config,
                mask,
                Scene::new(startup.build(scale)),
            ));
        }
        let live = engine.surfaces();
        let stale = stale_mounts(
            self.mounts.iter().map(|(id, m)| (*id, &m.role, &m.config)),
            live.iter().map(|(id, _)| *id),
            desired.iter().map(|d| (&d.0, &d.1)),
        );
        for id in stale {
            engine.close(id);
            self.mounts.remove(&id);
        }
        self.animation_active = false;
        for (role, config, mask, scene) in desired {
            let existing = self
                .mounts
                .iter()
                .find(|(_, m)| m.role == role)
                .map(|(id, _)| *id);
            let id = if let Some(id) = existing {
                let mount = &self.mounts[&id];
                let changed = mount.config.size != config.size
                    || mount.config.layer != config.layer
                    || mount.config.keyboard != config.keyboard
                    || mount.config.exclusive_zone != config.exclusive_zone;
                if changed {
                    engine.configure_surface(
                        id,
                        config.size,
                        config.layer,
                        config.exclusive_zone,
                        config.keyboard,
                    );
                }
                match &role {
                    Role::Bar(name) => {
                        if config.input_only { /* transparent1px role; no hidden component tree */ } else if let Some(s) = engine.scene(id) {
                            let _timing = hyprv_ui::diagnostics::scope("shell.bar_update");
                            let model = &self.owners.bars[name].model;
                            self.mounts.get_mut(&id).unwrap().bar_view.get_or_insert_with(Default::default).update(s, model, |tree| {
                                panels::qrect::convert(tree, (0., 0.), model.output.scale);
                            });
                        }
                    }
                    Role::BarPopup(_) => {}
                    Role::Panel(_, _) => {}
                    Role::Corner(name,_) => {
                        if let Some(existing)=engine.scene(id) {
                            let root=scene.root();
                            let top=self.owners.bars[name].model.top_corner_offset();
                            let bottom=0.;
                            let mut node=bar::surfaces::corner_window(root.frame.width,root.frame.height,
                                self.outputs[name].1.scale,top,bottom);
                            if self.owners.bars[name].model.compact_strip_visible() { node.add(bar::surfaces::compact_strip(&self.owners.bars[name].model).node); }
                            let root_id=existing.root().id();
                            bar::view::reconcile(existing,root_id,node);
                        }
                    }
                    Role::Startup => {
                        if let (Some(startup), Some(scene)) = (&mut self.startup, engine.scene(id))
                        {
                            let scale = self
                                .primary
                                .as_ref()
                                .map_or(1., |name| self.outputs[name].1.scale);
                            startup.update(scene, scale);
                        }
                    }
                    _ => {}
                }
                if matches!(role, Role::Panel(_, _) | Role::BarPopup(_) | Role::Tooltip(_)) {
                    let mut scene = scene;
                    if changed {
                        scene.invalidate();
                    }
                    engine.replace_scene(id, scene);
                }
                id
            } else {
                engine.add_surface(config.clone(), scene)
            };
            if self.mounts.get(&id).is_none_or(|m| m.mask != mask) {
                engine.set_input_region(id, Some(&mask));
            }
            let animating = match &role {
                Role::Bar(name) => {
                    self.owners.bars[name].animating()
                        || (self.owners.bars[name].model.calendar_presented
                            && self.owners.panels[name]
                                .borrow()
                                .calendar
                                .animating(self.now))
                        || self.owners.islands[name].borrow().animating(self.now)
                }
                Role::BarPopup(_) => self.popups.animating(),
                Role::Panel(name, _) => self.owners.panels[name]
                    .borrow()
                    .animating(&self.panel_model(name)),
                _ => false,
            };
            let bar_view = self.mounts.remove(&id).and_then(|mount| mount.bar_view);
            self.mounts.insert(id, Mount { role, config, mask, bar_view });
            self.animation_active |= animating;
            if animating && self.fixture.is_none() {
                engine.animate(id);
            }
        }
        if releasing
            && !self
                .mounts
                .values()
                .any(|m| matches!(m.role, Role::Panel(_, _) | Role::BarPopup(_)))
        {
            if let Some(request) = vm
                .action
                .filter(|request| self.vm_released != Some(*request))
            {
                self.vm_released = Some(request);
                let hub = self.hub.clone();
                let runtime = hub.runtime.clone();
                runtime.spawn(async move {
                    if let Err(e) = hub
                        .vm
                        .dispatch(hyprv_svc_sys::vm::Action::InputReleased { request })
                        .await
                    {
                        eprintln!("VM input release: {e}")
                    }
                });
            }
        }
        let v = &self.hub.visibility;
        v.set(Consumer::Bar, !self.owners.bars.is_empty());
        v.set(Consumer::Presentation, crate::laptop::active() && (self.owners.bars.values().any(|c|c.model.expanded) || self.owners.panels.values().any(|p|p.borrow().control.popup_requested())));
        let bar_values_painted = self.owners.bars.values().any(|c| bar_stats_painted(&c.model));
        v.set(Consumer::Tray, bar_values_painted || self.popups.menu.surface.mapped || self.popups.overflow.surface.mapped);
        let compact_painted = self.owners.bars.values().any(|c| c.model.compact_strip_visible() && c.model.top_corner_offset() > 0.);
        v.set(Consumer::Clock,compact_painted || bar_values_painted || self.owners.bars.values().any(|c|c.model.calendar_presented));
        v.set_bar_stats(if bar_values_painted {
            crate::services::BarStatsDemand::Detailed
        } else if compact_painted { crate::services::BarStatsDemand::Compact } else { crate::services::BarStatsDemand::Hidden });
        v.set(Consumer::AgentModel, !self.outputs.is_empty());
        v.set(Consumer::Island, !crate::laptop::active() || self.owners.islands.values().any(|i| !i.borrow().bar_hidden()));
        v.set(
            Consumer::Panel,
            self.owners
                .panels
                .values()
                .any(|p| p.borrow().control.is_mapped() || p.borrow().quick.mapped || vm.busy),
        );
        v.set(
            Consumer::Wifi,
            self.owners
                .panels
                .values()
                .any(|p| { let p=p.borrow(); if crate::laptop::active() {p.wifi.popup_visible} else {p.wifi.is_mapped()} }),
        );
        v.set(
            Consumer::Stats,
            self.owners
                .panels
                .values()
                .any(|p| p.borrow().stats.wants_samples()),
        );
        v.set(
            Consumer::Calendar,
            self.owners
                .bars
                .values()
                .any(|c| c.model.calendar_presented),
        );
        v.set(
            Consumer::Battery,
            if crate::laptop::active() { self.popups.battery.surface.requested } else { self.popups.surface(bar::popups::Kind::Battery).mapped },
        );
        v.set(
            Consumer::Media,
            self.owners.islands.values().any(|i| {
                let i = i.borrow();
                i.derived().music_active && !i.inputs().calendar_open && !i.bar_hidden()
            }) && if self.hub.is_mock() {
                self.fixture
                    .as_ref()
                    .is_some_and(|f| f.island().media.playing)
            } else {
                self.hub.mpris.topic.snapshot().value.playing
            },
        );
        // Preserve the armed absolute deadline until a tick consumes it. Some
        // components compute their next phase relative to `now` (cursor blink).
        self.scheduled_deadline = self.compute_next_deadline();
    }
}
#[derive(Default)]
struct PositionClock {
    seconds: f64,
    deadline: Option<Duration>,
}
impl PositionClock {
    fn step(&mut self, now: Duration, visible: bool, position: f64) -> bool {
        if !visible {
            self.deadline = None;
            return false;
        }
        if self.deadline.is_none_or(|deadline| now >= deadline) {
            self.seconds = position;
            self.deadline = Some(now + Duration::from_secs(1));
            return true;
        }
        false
    }
}
fn load_assets() -> Result<Rc<panels::model::Assets>, Box<dyn std::error::Error>> {
    let root = crate::runtime_assets::installed()?;
    Ok(Rc::new(panels::fixtures::load_assets(&root, &root.join("icons"))))
}
// Quantize the crop to whole logical coordinates AND whole device pixels.
// Fractional-scale denominators are 120, so this preserves the original glyph
// and rectangle sampling phase while rendering only a padded card.
fn wall_clock(wall:chrono::NaiveDateTime)->bar::model::WallClock {
    use chrono::{Datelike,Timelike};
    bar::model::WallClock {weekday:wall.weekday().num_days_from_monday() as u8,hour:wall.hour() as u8,minute:wall.minute() as u8}
}
fn bar_stats_painted(model: &bar::Model) -> bool {
    let layout = bar::layout::compute(model,&model.theme);
    [bar::model::Target::Cpu,bar::model::Target::Memory].iter().any(|target| {
        layout.target(target).is_some_and(|r| r.x < model.output.width && r.x+r.width > 0.
            && r.y < model.surface_height() && r.y+r.height > 0.)
    })
}
fn popup_crop(card: Rect, scale: f32, padding: f32) -> Rect {
    let mut a = (scale * 120.).round().max(1.) as u32;
    let mut b = 120;
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    let step = (120 / a.max(1)) as f32;
    let left = ((card.x - padding) / step).floor() * step;
    let top = ((card.y - padding) / step).floor() * step;
    let right = ((card.x + card.width + padding) / step).ceil() * step;
    let bottom = ((card.y + card.height + padding) / step).ceil() * step;
    Rect::new(
        left,
        top,
        (right - left).max(step),
        (bottom - top).max(step),
    )
}
fn surface_card(scene: &Scene) -> Option<Rect> {
    scene
        .root()
        .children
        .first()?
        .children
        .first()
        .map(|n| n.frame)
}
/// Match the legacy full-output Canvas window. Texture sampling depends on the
/// original viewport; four cropped surfaces change the presented alpha fringes.
/// Direct full redraw avoids an additional full-output retained target/copy.
fn corner_surfaces(mut config: LayerConfig, node: hyprv_ui::scene::Node, scale: f32)
    -> Vec<(LayerConfig, hyprv_ui::scene::Node)>
{
    if node.children.len()<4 {return vec![(config,node)];}
    let (width,height)=(node.frame.width,node.frame.height);
    let top=node.children[0].frame.y;
    let bottom=height-27.-node.children[2].frame.y;
    config.full_redraw=true;
    let mut atlas=bar::surfaces::corner_window(width,height,scale,top,bottom);
    for child in node.children.into_iter().skip(4) { atlas.add(child); }
    vec![(config,atlas)]
}

fn wrap_surface(content: hyprv_ui::scene::Node, output: &panels::model::Output) -> Scene {
    let mut root = hyprv_ui::scene::Node::new(
        Rect::new(0., 0., output.width, output.height),
        hyprv_ui::scene::Content::Group,
    );
    root.add(content);
    Scene::new(root)
}
fn position_crop(scene: &mut Scene, crop: Rect, scale: f32) {
    let id = scene.root().id();
    let frame = scene.root().frame;
    if frame.x != -crop.x || frame.y != -crop.y {
        scene.set_position(id, -crop.x, -crop.y);
        panels::qrect::finalize(scene, scale);
    }
}
fn cropped_config(
    mut config: LayerConfig,
    crop: Rect,
    input: Option<Rect>,
) -> (LayerConfig, Vec<Rect>) {
    config.anchor = Anchor::TOP | Anchor::LEFT;
    config.size = (crop.width.ceil() as u32, crop.height.ceil() as u32);
    config.margins = (crop.y as i32, 0, 0, crop.x as i32);
    let mask = input.map_or(Rect::new(0., 0., crop.width, crop.height), |r| {
        Rect::new(r.x - crop.x, r.y - crop.y, r.width, r.height)
    });
    (config, vec![mask])
}
fn outside_regions(output: (f32, f32), crop: Rect) -> Vec<Rect> {
    let (w, h) = output;
    let left = crop.x.clamp(0., w);
    let top = crop.y.clamp(0., h);
    let right = (crop.x + crop.width).clamp(0., w);
    let bottom = (crop.y + crop.height).clamp(0., h);
    [
        Rect::new(0., 0., w, top),
        Rect::new(0., bottom, w, h - bottom),
        Rect::new(0., top, left, bottom - top),
        Rect::new(right, top, w - right, bottom - top),
    ]
    .into_iter()
    .filter(|r| r.width > 0. && r.height > 0.)
    .collect()
}
fn dismiss_surface(
    role: Role,
    mut config: LayerConfig,
    output: &panels::model::Output,
    crop: Rect,
) -> (Role, LayerConfig, Vec<Rect>, Scene) {
    config.namespace.push_str(".dismiss");
    config.input_only = true;
    config.keyboard = KeyboardInteractivity::None;
    config.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
    config.size = (0, 0);
    config.margins = (0, 0, 0, 0);
    let mask = outside_regions((output.width, output.height), crop);
    (
        Role::Dismiss(Box::new(role)),
        config,
        mask,
        Scene::new(hyprv_ui::scene::Node::new(
            Rect::default(),
            hyprv_ui::scene::Content::Group,
        )),
    )
}
fn panel_cursor(c: panels::ui::Cursor) -> CursorShape {
    match c {
        panels::ui::Cursor::Pointer => CursorShape::Pointer,
        _ => CursorShape::Default,
    }
}
fn remove_output_popups(popups: &mut bar::popups::Popups, output: &str) -> Vec<bar::Action> {
    let fresh = bar::popups::Popups::default();
    let mut actions = vec![];
    if popups
        .menu
        .surface
        .output
        .as_ref()
        .is_some_and(|o| o.name == output)
    {
        if let Some(item) = popups.menu.item.clone() {
            actions.push(bar::Action::Tray(
                hyprv_svc_media::tray::TrayAction::MenuClose { item },
            ));
        }
        popups.menu = fresh.menu;
    }
    if popups
        .overflow
        .surface
        .output
        .as_ref()
        .is_some_and(|o| o.name == output)
    {
        popups.overflow = fresh.overflow;
    }
    if popups
        .battery
        .surface
        .output
        .as_ref()
        .is_some_and(|o| o.name == output)
    {
        popups.battery = fresh.battery;
    }
    actions
}
fn stale_mounts<'a, I: Copy + Eq>(
    mounted: impl Iterator<Item = (I, &'a Role, &'a LayerConfig)>,
    live: impl Iterator<Item = I>,
    desired: impl Iterator<Item = (&'a Role, &'a LayerConfig)>,
) -> Vec<I> {
    let live: Vec<_> = live.collect();
    let desired: Vec<_> = desired.collect();
    mounted
        .filter(|(id, role, config)| {
            !live.contains(id)
                || !desired
                    .iter()
                    .any(|(r, c)| *r == *role && same_binding(config, c))
        })
        .map(|(id, _, _)| id)
        .collect()
}
// Layer-shell output binding and anchors are immutable after creation.
fn same_binding(a: &LayerConfig, b: &LayerConfig) -> bool {
    a.output == b.output
        && a.anchor == b.anchor
        && a.namespace == b.namespace
        && a.margins == b.margins
        && a.input_only == b.input_only
        && a.full_redraw == b.full_redraw
}
fn quick_adjust_ipc(
    controller: &mut panels::Panels,
    function: &str,
    args: &[String],
    model: &panels::Model,
) -> (Vec<panels::Action>, Option<i32>) {
    (
        controller.ipc_quick_adjust(function, model),
        preview_brightness(function, args, model.controls.brightness_percent),
    )
}
/// Legacy quick-adjust brightness IPC reports a hardware change made by its caller.
/// It only changes the local OSD projection; it never constructs a provider action.
fn preview_brightness(function: &str, args: &[String], current: i32) -> Option<i32> {
    let step = if hyprv_core::profile::profile() == hyprv_core::profile::Profile::Laptop { 1 } else { 2 };
    let value = match function {
        "showBrightnessLevel" => args.first()?.parse::<i32>().ok()?,
        "showBrightnessIncrease" => current + step,
        "showBrightnessDecrease" => current - step,
        _ => return None,
    };
    Some(((value.clamp(0, 100) + step / 2) / step * step).min(100))
}
fn panel_config(
    spec: &panels::ui::SurfaceSpec,
    outputs: &HashMap<String, (WlOutput, bar::model::Output)>,
) -> LayerConfig {
    let mut anchor = Anchor::empty();
    for (on, value) in
        spec.anchors
            .into_iter()
            .zip([Anchor::TOP, Anchor::LEFT, Anchor::RIGHT, Anchor::BOTTOM])
    {
        if on {
            anchor |= value
        }
    }
    LayerConfig {
        namespace: spec.namespace.into(),
        output: outputs.get(&spec.output).map(|o| o.0.clone()),
        size: spec.size.map_or((0, 0), |(w, h)| (w as u32, h as u32)),
        layer: Layer::Overlay,
        anchor,
        exclusive_zone: spec.exclusive_zone,
        keyboard: match spec.keyboard {
            panels::ui::KeyboardMode::None => KeyboardInteractivity::None,
            panels::ui::KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
            _ => KeyboardInteractivity::OnDemand,
        },
        ..Default::default()
    }
}
#[derive(Clone)]
struct Adapter(Rc<RefCell<Assembly>>);
impl UiHandler for Adapter {
    fn clock(&mut self, now: Duration) {
        let mut a = self.0.borrow_mut();
        if a.fixture.is_none() {
            a.now = now;
        }
    }
    fn update(&mut self, engine: &mut Engine) {
        self.0.borrow_mut().sync(engine)
    }
    fn update_after_frame(&self) -> bool {
        let a = self.0.borrow();
        a.fixture.is_some() || a.dirty || a.stats_dirty || a.animation_active
            || a.scheduled_deadline.is_some_and(|at| at <= a.now)
    }
    fn immediate_static_frames(&self) -> bool {
        static CONTROL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        self.0.borrow().fixture.is_none()
            && !*CONTROL.get_or_init(|| std::env::var_os("HYPRV_STATIC_CALLBACK_CONTROL").is_some())
    }
    fn next_deadline(&self) -> Option<Duration> {
        let a = self.0.borrow();
        if a.fixture.is_some() && std::env::var("HYPRV_IDLE_TEST").as_deref() != Ok("1") {
            return None;
        }
        a.scheduled_deadline
    }
    fn output_added(
        &mut self,
        output: &WlOutput,
        info: Option<MonitorInfo>,
    ) -> Vec<(LayerConfig, Scene)> {
        if let Some(info) = info {
            let mut a = self.0.borrow_mut();
            // Rotation-aware: xdg-output size, else the mode with w/h swapped for 90/270.
            let (w, h) = hyprv_ui::wayland::output_logical_size(&info).unwrap_or((1280, 720));
            let name = info.name.unwrap_or_else(|| format!("output-{}", info.id));
            if a.primary.is_none() {
                a.primary = Some(name.clone());
            }
            a.outputs.insert(
                name.clone(),
                (
                    output.clone(),
                    bar::model::Output {
                        name,
                        width: w as f32,
                        height: h as f32,
                        scale: info.scale_factor as f32,
                        global_x: info.logical_position.map_or(0., |(x, _)| x as f32),
                        global_y: info.logical_position.map_or(0., |(_, y)| y as f32),
                    },
                ),
            );
            a.dirty = true;
        }
        vec![]
    }
    fn output_changed(&mut self, output: &WlOutput, info: Option<MonitorInfo>) {
        self.output_added(output, info);
    }
    fn output_removed(&mut self, output: &WlOutput) {
        let mut a = self.0.borrow_mut();
        let name = a
            .outputs
            .iter()
            .find(|(_, o)| o.0 == *output)
            .map(|(n, _)| n.clone());
        if let Some(name) = name {
            a.outputs.remove(&name);
            a.bar_expansion.remove(&name);
            a.island_agent_revisions.remove(&name);
            a.owners.remove(&name);
            let actions = remove_output_popups(&mut a.popups, &name);
            for action in actions {
                a.hub.bar_action(action);
            }
            if a.primary.as_ref() == Some(&name) {
                a.primary = a.outputs.keys().min().cloned();
            }
            a.dirty = true;
        }
    }
    fn resource_transition_active(&self)->bool {self.0.borrow().animation_active}
    fn presentation_feedback(&self,_:SurfaceId)->bool {self.0.borrow().animation_active}
    fn frame(&mut self, _: SurfaceId, _: &mut Scene, time: FrameInfo) -> bool {
        let mut a = self.0.borrow_mut();
        if a.fixture.is_none() {
            a.now = time.time;
        }
        false
    }
    fn input(&mut self, id: SurfaceId, _: &mut Scene, event: InputEvent) -> Response {
        let mut a = self.0.borrow_mut();
        if a.hub.is_mock() {
            functional::observe(&event);
        }
        let Some((mut role, config)) = a
            .mounts
            .get(&id)
            .map(|m| (m.role.clone(), m.config.clone()))
        else {
            return Response::default();
        };
        if matches!(&event, InputEvent::Pointer { event, .. } if matches!(event.kind, PointerKind::Release { .. })) {
            hyprv_ui::diagnostics::mark("input.release", &config.namespace);
        }
        // An output's removal precedes cancellation of its stale native mounts.
        // Their controllers have already been dropped; ignore late seat events.
        if role_output(&role).is_some_and(|name| !a.outputs.contains_key(name)) {
            return Response::default();
        }
        let origin = if matches!(role, Role::Panel(_, _) | Role::BarPopup(_)) {
            (config.margins.3 as f32, config.margins.0 as f32)
        } else {
            (0., 0.)
        };
        if let Role::Dismiss(inner) = role {
            role = *inner;
        }

        if let InputEvent::Resize { scale, .. } = &event {
            let name = match &role {
                Role::Bar(name) | Role::Panel(name, _) | Role::Corner(name, _) => {
                    Some(name.clone())
                }
                _ => None,
            };
            if let Some(name) = name {
                if let Some(output) = a.outputs.get_mut(&name) {
                    output.1.scale = *scale as f32;
                }
            }
        }
        let mut cursor = None;
        let pointer = crate::mapping::pointer(&event, &mut a.pointer).map(|event| {
            a.pointer.0 += origin.0;
            a.pointer.1 += origin.1;
            crate::mapping::offset_pointer(event, origin)
        });
        let native_wheel = crate::mapping::native_wheel(&event);
        if let Some(event) = pointer {
            if matches!(event, PointerEvent::Press { .. }) {
                a.input_epoch = true;
                a.input_values.clear();
            }

            match role {
                Role::Bar(ref name) => {
                    if let Some(wheel) = native_wheel {
                        let now = a.now;
                        let (x, y) = a.pointer;
                        let outcome = a.owners.islands[name].borrow_mut().wheel(x, y, wheel, now);
                        for action in outcome.actions {
                            a.island_action(action, name);
                        }
                    } else if let Some(ev) = crate::mapping::island_pointer(event, a.pointer) {
                        let now = a.now;
                        let outcome = a.owners.islands[name].borrow_mut().pointer(ev, now);
                        cursor = Some(match outcome.cursor {
                            hyprv_ui_island::CursorShape::Pointer => CursorShape::Pointer,
                            _ => CursorShape::Default,
                        });
                        for action in outcome.actions {
                            a.island_action(action, name);
                        }
                    }
                    if a.owners.bars[name].model.calendar_presented {
                        let model = a.panel_model(name);
                        let abs = (
                            (model.output.width - a.owners.panels[name].borrow().calendar.width)
                                / 2.,
                            10.,
                        );
                        if let Some(ev) = crate::mapping::panel_pointer(event, a.pointer) {
                            let reply = a.owners.panels[name]
                                .borrow_mut()
                                .calendar
                                .pointer(ev, &model, abs);
                            if let Some(cache) = a.owners.calendar_trees.get(name) {
                                cache.borrow_mut().invalid = true;
                            }
                            for action in reply.actions {
                                a.panel_action(action, name)
                            }
                        }
                    }
                    if let Some(c) = a.owners.bars.get_mut(name) {
                        let result = c.pointer(event);
                        if let Some(c) = result.cursor {
                            cursor = Some(match c {
                                hyprv_widgets::controls::Cursor::Pointer => CursorShape::Pointer,
                                _ => CursorShape::Default,
                            });
                        }
                        let model = c.model.clone();
                        for action in result.actions {
                            a.action(action, name)
                        }
                        if let Some(command) = result.popup {
                            let now = a.now;
                            let actions = a.popups.command(&model, command, now);
                            for action in actions {
                                a.action(action, name)
                            }
                        }
                    }
                }
                Role::Corner(ref name, _) if crate::laptop::active() => {
                    if let Some(c)=a.owners.bars.get_mut(name) {
                        let result=c.input.strip_pointer(&mut c.model,event);
                        let model=c.model.clone();
                        cursor=result.cursor.map(|c|match c {hyprv_widgets::controls::Cursor::Pointer=>CursorShape::Pointer,_=>CursorShape::Default});
                        for action in result.actions {a.action(action,name);}
                        if let Some(command)=result.popup {
                            let now=a.now;let actions=a.popups.command(&model,command,now);
                            for action in actions {a.action(action,name);}
                        }
                    }
                }
                Role::Panel(ref name, kind) => {
                    let model = a.panel_model(name);
                    if let Some(event) = crate::mapping::panel_pointer(event, a.pointer) {
                        let reply = a.owners.panels[name]
                            .borrow_mut()
                            .pointer(kind, event, &model);
                        cursor = reply.cursor.map(panel_cursor);
                        for action in reply.actions {
                            a.panel_action(action, name)
                        }
                    }
                }
                Role::Startup => {
                    if let Some(ev) = crate::mapping::panel_pointer(event, a.pointer) {
                        let reply = a.startup.as_mut().unwrap().pointer(ev);
                        cursor = reply.cursor.map(panel_cursor);
                        if let Some(name) = a.primary.clone() {
                            for action in reply.actions {
                                a.panel_action(action, &name)
                            }
                        }
                    }
                }
                Role::BarPopup(kind) => {
                    if let Some(name) = a.primary.clone() {
                        let model = a.owners.bars[&name].model.clone();
                        let now = a.now;
                        let reply = if let Some(wheel) =
                            native_wheel.filter(|_| kind == bar::popups::Kind::Menu)
                        {
                            a.popups.menu_wheel(&model, wheel, now)
                        } else {
                            a.popups.pointer(&model, kind, event, now)
                        };
                        if let Some(c) = reply.cursor {
                            cursor = Some(match c {
                                hyprv_widgets::controls::Cursor::Pointer => CursorShape::Pointer,
                                _ => CursorShape::Default,
                            });
                        }
                        for action in reply.actions {
                            a.action(action, &name)
                        }
                        if let Some(command) = reply.command {
                            let actions = a.popups.command(&model, command, now);
                            for action in actions {
                                a.action(action, &name)
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let InputEvent::Key { event, pressed, .. } = &event {
            if let Role::BarPopup(kind) = role {
                let now = a.now;
                let actions = a.popups.keyboard(kind, event.keysym.raw(), *pressed, now);
                if let Some(name) = a.primary.clone() {
                    for action in actions {
                        a.action(action, &name)
                    }
                }
            }
        }
        if let InputEvent::Key { event, pressed, .. } = &event {
            if role == Role::Startup {
                if let Some(key) = crate::mapping::key(event.keysym, event.utf8.clone()) {
                    let reply = a.startup.as_mut().unwrap().key(&panels::ui::KeyEvent {
                        key,
                        pressed: *pressed,
                        autorepeat: false,
                    });
                    if let Some(name) = a.primary.clone() {
                        for action in reply.actions {
                            a.panel_action(action, &name)
                        }
                    }
                }
            }
        }
        if let InputEvent::Key { event, pressed, .. } = event {
            if a.primary.is_some() {
                if let Some(key) = crate::mapping::key(event.keysym, event.utf8) {
                    if let Role::Panel(ref output, kind) = role {
                        let model = a.panel_model(output);
                        let reply = a.owners.panels[output].borrow_mut().key(
                            kind,
                            &panels::ui::KeyEvent {
                                key,
                                pressed,
                                autorepeat: false,
                            },
                            &model,
                        );
                        for action in reply.actions {
                            a.panel_action(action, output)
                        }
                    }
                }
            }
        }
        if matches!(
            pointer,
            Some(PointerEvent::Release { .. } | PointerEvent::Cancel)
        ) {
            a.input_epoch = false;
            a.input_values.clear();
        }
        a.dirty = true;
        Response {
            cursor,
            ..Default::default()
        }
    }
}
/// Production entry point. Isolated smoke runs use the same provider/component graph.
pub fn run(
    no_exclusive: bool,
    exit_after: Option<Duration>,
    dump: Option<std::path::PathBuf>,
    read_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("HYPRV_NESTED_FIXTURE").is_some()
        && (!read_only || std::env::var("HYPRV_NESTED_TEST").as_deref() != Ok("1"))
    {
        return Err("fixture replay requires the isolated nested-test child".into());
    }
    // Fail before providers, IPC or Wayland start if this release is incomplete.
    let assets = load_assets()?;
    let runtime = Arc::new(ServiceRuntime::new()?);
    let store = Arc::new(StateStore::default());
    let server = runtime.enter(|| IpcServer::bind(ipc::socket_path()?))?;
    let (changed, changes) = hyprv_core::calloop::ping::make_ping()?;
    let functional = std::env::var("HYPRV_FUNCTIONAL_TEST").as_deref() == Ok("1");
    if functional
        && (std::env::var("HYPRV_NESTED_TEST").as_deref() != Ok("1")
            || !read_only
            || std::env::var_os("HYPRV_NESTED_FIXTURE").is_none())
    {
        return Err("functional test requires private nested fixture child".into());
    }
    let hub = if functional {
        Hub::mocked(runtime.clone(), store)
    } else {
        // The real shell rebuilds its system-bus services by re-executing once the bus returns.
        crate::system_bus::set_recovery(crate::system_bus::Recovery::ReExec);
        Hub::spawn(runtime.clone(), store, changed.clone(), read_only)?
    };
    let (art_ready, art_receive) = hyprv_core::calloop::channel::channel();
    let assembly = Rc::new(RefCell::new(Assembly::new(
        hub.clone(),
        no_exclusive,
        art_ready,
        assets,
    )));
    if let Some(path) = std::env::var_os("HYPRV_NESTED_FIXTURE") {
        let dark = std::env::var("HYPRV_NESTED_THEME").as_deref() == Ok("dark");
        let scale = std::env::var("HYPRV_NESTED_SCALE")?.parse()?;
        let assets = assembly.borrow().assets.clone();
        let fixture =
            crate::fixture::Fixture::load(std::path::Path::new(&path), dark, scale, assets)?;
        let mut a = assembly.borrow_mut();
        a.dark = dark;
        a.fixture = Some(fixture);
    }
    let mut app = Application::connect(Adapter(assembly.clone()))?;
    if assembly.borrow().fixture.is_some() && !functional {
        app.set_external_frame_clock(true);
    }
    let state = assembly.clone();
    app.event_loop_handle()
        .insert_source(art_receive, move |event, _, engine| {
            if let Event::Msg(art) = event {
                let mut a = state.borrow_mut();
                match art {
                    crate::assets::Ready::Art(art) if a.art_url == art.url => {
                        a.art = crate::assets::decode(&art.bytes);
                        a.dirty = true;
                    }
                    crate::assets::Ready::Tray(ready) => {
                        a.tray_loader.complete(ready);
                        a.dirty = true;
                    }
                    _ => return,
                }
                // request_redraw runs Adapter::update synchronously, which borrows the assembly.
                drop(a);
                engine.request_redraw();
            }
        })?;
    let (signal_send, signal_receive) = hyprv_core::calloop::channel::channel::<()>();
    let mut stop = runtime.shutdown_signal();
    runtime.spawn(async move{use hyprv_core::tokio::signal::unix::{signal,SignalKind};let Ok(mut term)=signal(SignalKind::terminate())else{return};hyprv_core::tokio::select!{_=hyprv_core::tokio::signal::ctrl_c()=>(),_=term.recv()=>(),_=stop.changed()=>return}let _=signal_send.send(());});
    app.event_loop_handle()
        .insert_source(signal_receive, |event, _, engine| {
            if matches!(event, Event::Msg(_)) {
                engine.quit();
            }
        })?;

    let state = assembly.clone();
    app.event_loop_handle()
        .insert_source(changes, move |_, _, engine| {
            let mut a = state.borrow_mut();
            if a.hub.take_stats_only() {
                hyprv_ui::diagnostics::mark("stats.tick.begin","");
                a.stats_dirty = true;
            } else { a.dirty = true; }
            drop(a);
            engine.request_redraw();
        })?;
    let (send, receive) = ServiceRuntime::bridge::<ipc::UiRequest>();
    let stop = runtime.shutdown_signal();
    // `desktop` IPC awaits service results off the UI thread; everything else goes to the UI.
    let ui = ipc::ui_handler(send);
    let desktop_hub = hub.clone();
    let handler: ipc::Handler = Arc::new(move |request: Request| {
        if request.target == "desktop" {
            let hub = desktop_hub.clone();
            Box::pin(async move { hub.desktop_ipc(request).await })
        } else {
            ui(request)
        }
    });
    let task = runtime.spawn(async move { server.run(handler, stop).await });
    let state = assembly.clone();
    app.event_loop_handle()
        .insert_source(receive, move |event, _, engine| {
            if let Event::Msg(message) = event {
                let response = if state.borrow().hub.is_mock() && message.request.target == "e2e" {
                    functional::dispatch(&state, engine, message.request)
                } else {
                    state.borrow_mut().dispatch(message.request)
                };
                let _ = message.reply.send(response);
                engine.request_redraw();
            }
        })?;
    let time = hyprv_core::store::Topic::new(hyprv_core::clock::unix_seconds() / 60);
    let (clock_send, clock_recv) = ServiceRuntime::bridge();
    let clock_topic = time.clone();
    let stop = runtime.shutdown_signal();
    runtime.spawn(async move {
        let _ = hyprv_core::clock::run_clock(clock_topic, clock_send, stop).await;
    });
    runtime.spawn(crate::services::forward(
        time,
        hub.visibility.gate(Consumer::Clock),
        runtime.shutdown_signal(),
        changed,
    ));
    let state = assembly.clone();
    app.event_loop_handle()
        .insert_source(clock_recv, move |event, _, engine| {
            if matches!(event, Event::Msg(_)) {
                state.borrow_mut().dirty = true;
                engine.request_redraw();
            }
        })?;
    if read_only && assembly.borrow().fixture.is_none() {
        let state = assembly.clone();
        app.event_loop_handle().insert_source(
            Timer::from_duration(Duration::from_millis(300)),
            move |_, _, engine| {
                let mut a = state.borrow_mut();
                if a.primary.is_none() && engine.now() < Duration::from_secs(8) {
                    return TimeoutAction::ToDuration(Duration::from_millis(50));
                }
                if let Some(output) = a.primary.clone() {
                    let action = match std::env::var("HYPRV_SMOKE_SCENE").as_deref() {
                        Ok("calendar") => Some(bar::model::HostAction::ToggleCalendar),
                        Ok("control") => Some(bar::model::HostAction::ToggleControlPanel {
                            anchor: Rect::new(1200., 10., 38., 38.),
                        }),
                        Ok("wifi") => Some(bar::model::HostAction::ToggleWifi {
                            anchor: Rect::new(1150., 10., 38., 38.),
                        }),
                        Ok("stats") => Some(bar::model::HostAction::ToggleStats {
                            anchor: Rect::new(10., 10., 38., 38.),
                        }),
                        Ok("volume") => Some(bar::model::HostAction::ShowVolume),
                        _ => None,
                    };
                    if let Some(action) = action {
                        a.action(bar::Action::Host(action), &output);
                    }
                }
                drop(a);
                engine.request_redraw();
                TimeoutAction::Drop
            },
        )?;
    }
    if functional {
        functional::install(
            &app,
            assembly.clone(),
            dump.clone().ok_or("functional output directory")?,
        )?;
    } else if assembly.borrow().fixture.is_some() {
        replay::install(
            &app,
            assembly.clone(),
            dump.clone()
                .ok_or("fixture capture needs output directory")?,
        )?;
    }
    if let Some(duration) = exit_after.filter(|_| assembly.borrow().fixture.is_none()) {
        app.event_loop_handle().insert_source(Timer::from_duration(duration),move|_,_,engine|{if let Some(path)=&dump{match engine.dump_surfaces(path){Ok(captures)=>{if read_only && captures.len()<2 && engine.now()<duration+Duration::from_secs(6) {return TimeoutAction::ToDuration(Duration::from_millis(100));} let surfaces:Vec<_>=captures.into_iter().map(|capture|serde_json::json!({"file":capture.file,"namespace":capture.config.namespace,"logical_size":[capture.size.0,capture.size.1],"scale":capture.scale,"output":capture.output,"margins":[capture.config.margins.0,capture.config.margins.1,capture.config.margins.2,capture.config.margins.3],"anchors":capture.config.anchor.bits(),"input_only":capture.config.input_only,"source":"retained GPU target"})).collect();let _=std::fs::write(path.join("manifest.json"),serde_json::to_vec_pretty(&serde_json::json!({"surfaces":surfaces})).unwrap());},Err(e)=>eprintln!("surface dump: {e}")}}engine.quit();TimeoutAction::Drop})?;
    }
    crate::allocator::release_startup_scratch();
    let result = app.run();
    runtime.request_shutdown();
    drop(assembly);
    drop(hub);
    let _ = runtime.block_on(task);
    result?;
    Ok(())
}

#[cfg(test)]
mod review_regressions {
    #[test]
    fn completely_translated_stats_release_demand_but_partial_reveal_keeps_it() {
        let mut model = bar::Model::default();
        assert!(bar_stats_painted(&model));
        model.fullscreen_hide = 0.5;
        assert!(bar_stats_painted(&model));
        model.fullscreen_hide = 1.;
        assert!(!bar_stats_painted(&model));
        model.fullscreen_hide = 0.; model.reveal = 0.;
        assert!(!bar_stats_painted(&model));
    }
    #[test]
    fn corner_overlay_keeps_legacy_viewport_and_avoids_retained_copy() {
        for scale in [1.,1.25,1.5,2.] {for reveal in [0.,0.375,1.] {
            let mut model=bar::Model::default();model.output.scale=scale;model.reveal=reveal;
            let (requirement,node)=bar::surfaces::corners(&model).remove(0);
            let views=corner_surfaces(requirement.layer_config(),node,scale);
            assert_eq!(views.len(),1);
            let (config,node)=&views[0];
            assert!(config.full_redraw && config.input_passthrough);
            assert_eq!(config.size,(0,0));
            assert_eq!(config.anchor,Anchor::TOP|Anchor::BOTTOM|Anchor::LEFT|Anchor::RIGHT);
            assert_eq!(config.exclusive_zone,-1);
            assert_eq!(config.layer,Layer::Overlay);
            assert_eq!(node.frame,Rect::new(0.,0.,model.output.width,model.output.height));
            assert_eq!(node.children.len(),4);
            for (i,corner) in node.children.iter().enumerate(){
                assert!(matches!(corner.image_sampling,hyprv_ui::image::ImageSampling::QtCanvasAtlas {corner,..} if corner==i as u8));
            }
        }}
    }

    #[test]
    fn late_dismiss_surface_events_keep_the_removed_output_identity() {
        let role = Role::Dismiss(Box::new(Role::Panel(
            "gone".into(),
            panels::PanelKind::ControlPanel,
        )));
        assert_eq!(role_output(&role), Some("gone"));
    }
    #[test]
    fn preview_deduplication_is_limited_to_one_pointer_gesture() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let hub = Hub::mocked(runtime.clone(), Arc::new(StateStore::default()));
        let (send, _) = hyprv_core::calloop::channel::channel();
        let mut a = Assembly::new(hub.clone(), true, send, panels::fixtures::default_assets());
        a.input_epoch = true;
        a.panel_action(panels::Action::SetVolume(75), "TEST");
        a.panel_action(panels::Action::SetVolume(75), "TEST");
        runtime.block_on(async {
            hyprv_core::tokio::time::sleep(Duration::from_millis(10)).await;
        });
        assert_eq!(hub.recorded(false).len(), 1);
        a.input_epoch = false;
        a.panel_action(panels::Action::SetVolume(75), "TEST");
        runtime.block_on(async {
            hyprv_core::tokio::time::sleep(Duration::from_millis(10)).await;
        });
        assert_eq!(hub.recorded(false).len(), 2);
        runtime.request_shutdown();
    }

    use super::*;
    #[test]
    fn settled_frames_preserve_the_due_wifi_blink_deadline() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let hub = Hub::mocked(runtime.clone(), Arc::new(StateStore::default()));
        let (send, _) = hyprv_core::calloop::channel::channel();
        let mut a = Assembly::new(hub, true, send, panels::fixtures::default_assets());
        let mut panel = panels::Panels::default();
        panel.wifi.m.mapped = true;
        panel.wifi.expand_now("secure", "", true);
        a.owners.panels.insert("test".into(), Rc::new(RefCell::new(panel)));
        a.dirty = false;
        a.scheduled_deadline = a.compute_next_deadline();
        assert_eq!(a.scheduled_deadline, Some(Duration::from_millis(500)));
        let state = Rc::new(RefCell::new(a));
        let adapter = Adapter(state.clone());
        assert!(!adapter.update_after_frame());
        state.borrow_mut().now = Duration::from_millis(500);
        // Recomputing from the new frame time would incorrectly roll the due
        // blink forward to1000ms without painting its500ms phase.
        assert_eq!(state.borrow().compute_next_deadline(), Some(Duration::from_millis(1000)));
        assert_eq!(adapter.next_deadline(), Some(Duration::from_millis(500)));
        assert!(adapter.update_after_frame());
        runtime.request_shutdown();
    }

    #[test]
    fn fixture_refresh_never_accesses_native_tray_providers() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let hub = Hub::mocked(runtime.clone(), Arc::new(StateStore::default()));
        let (send, _) = hyprv_core::calloop::channel::channel();
        let mut a = Assembly::new(hub.clone(), true, send, panels::fixtures::default_assets());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/functional/idle.json");
        a.fixture = Some(crate::fixture::Fixture::load(&path, true, 1., a.assets.clone()).unwrap());
        a.refresh();
        assert!(!a.dirty);
        assert!(hub.recorded(false).is_empty());
        runtime.request_shutdown();
    }

    #[test]
    fn fractional_bar_clip_is_normalized_before_reconciliation() {
        let mut model = bar::Model::default();
        model.output.width = 3072.;
        model.output.height = 1728.;
        model.output.scale = 2.;
        model.island.width = 120.15625;
        model.island.height = 38.;
        let extent = Rect::new(0., 0., model.output.width, 58.);
        let mut scene = Scene::new(bar_tree(&model));
        // A genuine first update used to finalize the retained tree after diffing.
        panels::qrect::finalize(&mut scene, model.output.scale);
        scene.take_damage(extent);
        for _ in 0..3 {
            assert_eq!(update_bar_scene(&mut scene, &model), 0);
            assert!(
                !scene.is_dirty(),
                "equal normalized clips must not restart frames"
            );
        }
        model.stats.cpu_percent = Some(8.);
        assert!(update_bar_scene(&mut scene, &model) > 0);
        assert!(scene.is_dirty(), "a genuine stats change must still redraw");
    }

    #[test]
    fn stats_text_fast_path_matches_fresh_tree_and_preserves_noops_and_fallbacks(){
        for scale in [1.,1.5,2.] {for dark in [false,true]{
            let mut model=bar::Model::default();model.output.width=1600.;model.output.scale=scale;model.theme.dark=dark;
            model.stats.cpu_percent=Some(8.);model.stats.memory_percent=Some(42.);model.stats.temperature_c=Some(45.);
            model.stats.iface=Some(Arc::from("br0"));model.stats.rx_bytes_per_s=Some(2048.);
            let extent=Rect::new(0.,0.,1600.,model.surface_height());
            let mut scene=Scene::new(bar_tree(&model));scene.take_damage(extent);
            let before=stats_labels(&model);let style=stats_style(&model);
            model.stats.cpu_percent=Some(8.01);
            assert_eq!(patch_stats_text(&mut scene,&before,style,&model),Some(0));assert!(!scene.is_dirty());
            model.stats.cpu_percent=Some(9.);model.stats.memory_percent=Some(43.);model.stats.rx_bytes_per_s=Some(3072.);
            assert_eq!(patch_stats_text(&mut scene,&before,style,&model),Some(3));
            let regions=scene.take_damage_regions(extent);
            assert!(regions.iter().all(|r|r.width<extent.width/2.));
            let mut renderer=hyprv_ui::render::Renderer::new();
            let mut a=renderer.raster(&scene,extent.width,extent.height,scale as f64).unwrap();
            let mut b=renderer.raster(&Scene::new(bar_tree(&model)),extent.width,extent.height,scale as f64).unwrap();
            assert_eq!(a.image_snapshot().encode(None,hyprv_ui::skia::EncodedImageFormat::PNG,None).unwrap().as_bytes(),b.image_snapshot().encode(None,hyprv_ui::skia::EncodedImageFormat::PNG,None).unwrap().as_bytes());
            let before=stats_labels(&model);let style=stats_style(&model);model.stats.cpu_percent=Some(100.);
            assert_eq!(patch_stats_text(&mut scene,&before,style,&model),None);assert!(!scene.is_dirty());
            model.stats.cpu_percent=Some(9.);model.stats.temperature_c=Some(70.);
            assert_eq!(patch_stats_text(&mut scene,&before,style,&model),None);assert!(!scene.is_dirty());
        }}
    }
    #[test]
    fn scheduled_bar_updates_retain_modules_and_match_fresh_trees() {
        let mut model = bar::Model::default();
        model.output.width = 1600.;
        model.output.scale = 1.5;
        model.stats.cpu_percent = Some(8.);
        let extent = Rect::new(0., 0., model.output.width, model.surface_height());
        let mut scene = Scene::new(bar_tree(&model));
        let root = scene.root().id();
        let mut view = bar::view::RetainedView::default();
        let update = |view: &mut bar::view::RetainedView, scene: &mut Scene, model: &bar::Model| {
            view.update(scene, model, |tree| panels::qrect::convert(tree, (0., 0.), model.output.scale))
        };
        update(&mut view, &mut scene, &model);
        scene.take_damage(extent);
        // Callback time, sampler generations/history and sub-label changes do
        // not rebuild any visual module when formatted output is unchanged.
        model.now = Duration::from_secs(60);
        model.monotonic_us = 60_000_000;
        model.stats.generation += 1;
        model.stats.cpu_percent = Some(8.01);
        let idle = update(&mut view, &mut scene, &model);
        assert_eq!(idle.built_modules, 0);
        assert!(idle.retained_modules > 10);
        assert_eq!(idle.edited_nodes, 0);
        assert!(!scene.is_dirty());
        model.stats.cpu_percent = Some(9.);
        let changed = update(&mut view, &mut scene, &model);
        assert!(changed.built_modules > 0);
        assert!(changed.retained_modules > changed.built_modules);
        assert!(changed.edited_nodes > 0);
        let damage = scene.take_damage(extent).unwrap();
        assert!(damage.width < extent.width / 2., "stats must not damage the whole bar: {damage:?}");
        for step in 0..9 {
            match step {
                0 => model.hovered = Some(bar::model::Target::Cpu),
                1 => model.theme.dark = true,
                2 => model.output.scale = 2.,
                3 => model.output.width = 1100.,
                4 => model.calendar_open = true,
                5 => model.network_widget_visible = Some(true),
                6 => {
                    model.network.hardware_enabled = true;
                    model.network.enabled = true;
                    model.network.connected = true;
                    model.network.signal = 90;
                }
                7 => model.network.signal = 10,
                _ => model.notifications.alt = "notification".into(),
            }
            update(&mut view, &mut scene, &model);
            assert_eq!(scene.root().id(), root);
            let mut r = hyprv_ui::render::Renderer::new();
            r.set_offscreen_backend(hyprv_ui::offscreen::Backend::Cpu);
            let retained = r.image(&scene, model.output.width, model.surface_height(), 1.).unwrap();
            let fresh = r.image(&Scene::new(bar_tree(&model)), model.output.width, model.surface_height(), 1.).unwrap();
            let info = hyprv_ui::skia::ImageInfo::new_n32_premul(retained.dimensions(), None);
            let mut a = vec![0; retained.width() as usize * retained.height() as usize * 4];
            let mut b = a.clone();
            assert!(retained.read_pixels(&info, &mut a, retained.width() as usize * 4, (0, 0), hyprv_ui::skia::image::CachingHint::Disallow));
            assert!(fresh.read_pixels(&info, &mut b, retained.width() as usize * 4, (0, 0), hyprv_ui::skia::image::CachingHint::Disallow));
            assert!(a == b, "incremental/fresh mismatch at transition {step}");
        }
    }

    #[test]
    fn output_removal_drops_open_and_closing_controllers_and_deadlines() {
        for closing in [false, true] {
            let mut owners = OutputOwners::default();
            let mut panel = panels::Panels::default();
            panel.control.toggle_for(
                panels::model::Anchor::default(),
                "unplugged",
                Duration::ZERO,
            );
            if closing {
                panel.control.close(Duration::from_millis(20));
            }
            owners
                .panels
                .insert("unplugged".into(), Rc::new(RefCell::new(panel)));
            owners.islands.insert(
                "unplugged".into(),
                Rc::new(RefCell::new(hyprv_ui_island::Island::new(
                    Default::default(),
                    1280.,
                    Duration::ZERO,
                ))),
            );
            owners
                .bars
                .insert("unplugged".into(), bar::Controller::new(Default::default()));
            owners
                .brightness_preview
                .insert("unplugged".into(), (26, Duration::from_millis(90)));
            owners.brightness_probe.insert("unplugged".into(), Duration::from_millis(90));
            owners.remove("unplugged");
            assert!(
                owners.panels.is_empty()
                    && owners.islands.is_empty()
                    && owners.bars.is_empty()
                    && owners.brightness_preview.is_empty()
                    && owners.brightness_probe.is_empty()
            );
            assert_eq!(owners.next_deadline(Duration::from_secs(1)), None);
            assert!(!owners
                .panels
                .values()
                .any(|p| p.borrow().control.is_mapped() || p.borrow().stats.wants_samples()));
        }
    }
    #[test]
    fn native_close_and_immutable_binding_change_recreate_mounts() {
        let role = Role::BarPopup(bar::popups::Kind::Battery);
        let old = LayerConfig::default();
        let mounted = [(7, &role, &old)];
        let desired = [(&role, &old)];
        assert_eq!(
            stale_mounts(mounted.into_iter(), [].into_iter(), desired.into_iter()),
            vec![7]
        );
        assert!(stale_mounts(mounted.into_iter(), [7].into_iter(), desired.into_iter()).is_empty());
        let rebound = LayerConfig {
            anchor: Anchor::BOTTOM,
            ..old.clone()
        };
        assert_eq!(
            stale_mounts(
                mounted.into_iter(),
                [7].into_iter(),
                [(&role, &rebound)].into_iter()
            ),
            vec![7]
        );
        assert_eq!(
            stale_mounts(mounted.into_iter(), [7].into_iter(), [].into_iter()),
            vec![7]
        );
    }
    #[test]
    fn primary_migration_changes_native_output_binding() {
        use wayland_client::{
            protocol::{wl_output, wl_registry},
            Connection, Dispatch, QueueHandle,
        };
        struct Mock;
        impl Dispatch<wl_registry::WlRegistry, ()> for Mock {
            fn event(
                _: &mut Self,
                _: &wl_registry::WlRegistry,
                _: wl_registry::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
        wayland_client::delegate_noop!(Mock:ignore wl_output::WlOutput);
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let conn = Connection::from_socket(socket).unwrap();
        let queue = conn.new_event_queue::<Mock>();
        let qh = queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let first = registry.bind::<wl_output::WlOutput, _, _>(1, 4, &qh, ());
        let second = registry.bind::<wl_output::WlOutput, _, _>(2, 4, &qh, ());
        let a = LayerConfig {
            output: Some(first),
            ..Default::default()
        };
        let b = LayerConfig {
            output: Some(second),
            ..a.clone()
        };
        let role = Role::BarPopup(bar::popups::Kind::Menu);
        assert_eq!(
            stale_mounts(
                [(1, &role, &a)].into_iter(),
                [1].into_iter(),
                [(&role, &b)].into_iter()
            ),
            vec![1]
        );
    }
    #[test]
    fn removed_output_cancels_popup_timers_without_closing_other_output() {
        let mut popups = bar::popups::Popups::default();
        let m = bar::Model {
            output: bar::model::Output {
                name: "gone".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        popups.command(
            &m,
            bar::PopupCommand::Battery {
                anchor: Rect::default(),
                source: bar::model::Target::Battery,
            },
            Duration::ZERO,
        );
        assert!(popups.next_deadline().is_some());
        remove_output_popups(&mut popups, "other");
        assert!(popups.next_deadline().is_some());
        remove_output_popups(&mut popups, "gone");
        assert_eq!(popups.next_deadline(), None);
        assert!(!popups.surface(bar::popups::Kind::Battery).mapped);
    }
    #[test]
    fn music_position_clock_only_runs_for_expanded_consumer() {
        let mut clock = PositionClock::default();
        assert!(!clock.step(Duration::ZERO, false, 10.));
        assert_eq!(clock.deadline, None);
        assert!(clock.step(Duration::ZERO, true, 10.));
        assert!(!clock.step(Duration::from_millis(500), true, 10.5));
        assert_eq!(clock.seconds, 10.);
        assert!(clock.step(Duration::from_secs(1), true, 11.));
        assert!(!clock.step(Duration::from_secs(2), false, 12.));
        assert_eq!(clock.deadline, None);
    }
    #[test]
    fn brightness_notifications_only_produce_snapped_preview_values() {
        let mut controller = panels::Panels::default();
        let model = panels::Model::default();
        for function in [
            "showBrightness",
            "showBrightnessLevel",
            "showBrightnessIncrease",
            "showBrightnessDecrease",
        ] {
            let (actions, _) = quick_adjust_ipc(&mut controller, function, &["25".into()], &model);
            assert!(
                actions.iter().all(|a| matches!(a, panels::Action::Host(_))),
                "{function} emitted a mutating service action: {actions:?}"
            );
        }
        assert_eq!(
            preview_brightness("showBrightnessLevel", &["25".into()], 50),
            Some(26)
        );
        assert_eq!(
            preview_brightness("showBrightnessIncrease", &[], 50),
            Some(52)
        );
        assert_eq!(
            preview_brightness("showBrightnessDecrease", &[], 50),
            Some(48)
        );
        assert_eq!(preview_brightness("showBrightness", &[], 50), None);
        assert_eq!(
            preview_brightness("showBrightnessLevel", &["-30".into()], 50),
            Some(0)
        );
        assert_eq!(
            preview_brightness("showBrightnessLevel", &["200".into()], 50),
            Some(100)
        );
    }
    #[test]
    fn popup_crop_preserves_device_phase_and_outside_input_coverage() {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            let crop = popup_crop(Rect::new(421.3, 57.8, 278., 422.), scale, 24.);
            for x in [crop.x, crop.y, crop.width, crop.height] {
                assert_eq!(x.fract(), 0.);
                assert!((x * scale - (x * scale).round()).abs() < 0.0001);
            }
            let regions = outside_regions((1280., 720.), crop);
            for y in 0..720 {
                for x in 0..1280 {
                    let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
                    assert_eq!(
                        regions.iter().filter(|r| r.contains(x, y)).count(),
                        usize::from(!crop.contains(x, y))
                    );
                }
            }
        }
    }
    #[test]
    fn cropped_panel_updates_the_original_retained_scene() {
        let mut controller = panels::Panels::default();
        let mut model = panels::Model::default();
        model.output.width = 1280.;
        model.output.height = 720.;
        controller
            .control
            .toggle_for(Default::default(), "test", Duration::ZERO);
        let content = controller.build(panels::PanelKind::ControlPanel, &model);
        let id = content.id();
        let mut scene = wrap_surface(content, &model.output);
        let before = scene.revision();
        for ms in [50, 100, 300] {
            model.now = Duration::from_millis(ms);
            controller.tick(&model);
            controller.update(panels::PanelKind::ControlPanel, &mut scene, &model);
            let crop = popup_crop(surface_card(&scene).unwrap(), 1., 24.);
            position_crop(&mut scene, crop, 1.);
            assert_eq!(scene.root().children[0].id(), id);
        }
        assert!(scene.revision() > before);
    }

    #[test]
    fn idle_island_and_calendar_share_trees_during_container_motion() {
        use hyprv_ui::scene::Content;
        let island = hyprv_ui_island::Island::new(Default::default(), 1280., Duration::ZERO);
        let mut cache = IslandTree::default();
        let a = cache.get(&island);
        let b = cache.get(&island);
        let (Content::Shared(a), Content::Shared(b)) = (a.content, b.content) else {
            panic!()
        };
        assert!(Rc::ptr_eq(&a, &b));
        let mut calendar = panels::calendar::CalendarPanel::new(520., 500.);
        let mut model = panels::Model::default();
        let mut cache = CalendarTree::default();
        let a = cache.get(&mut calendar, &model, (380., 10.));
        model.now = Duration::from_millis(16);
        let b = cache.get(&mut calendar, &model, (380., 10.));
        let (Content::Shared(a), Content::Shared(b)) = (a.content, b.content) else {
            panic!()
        };
        assert!(Rc::ptr_eq(&a, &b));
        let c = cache.get(&mut calendar, &model, (381., 10.));
        let Content::Shared(c) = c.content else {
            panic!()
        };
        assert!(!Rc::ptr_eq(&a, &c));
        cache.prepared = Some((2_f32.to_bits(),true));
        cache.release_hidden();
        assert!(cache.tree.is_none() && cache.key.is_none());
        assert_eq!(cache.prepared,Some((2_f32.to_bits(),true)));
    }
}

#[path = "replay.rs"]
mod replay;

#[path = "functional.rs"]
mod functional;

#[cfg(test)]
mod fixture_clock_tests {
    use super::*;
    #[test]
    fn qt_pause_only_restart_matches_independent_unified_timer_trace() {
        // tools/qt_clock_trace.cpp, unmodified original full-screen control fixture.
        // Raw times are vclock's rounded microseconds, logical times are integer ms.
        let mut driver = AnimationDriver::default();
        for (wall, pause, logical) in [
            (2_516_667, true, 2516), (2_532_667, true, 2532),
            (2_549_333, true, 2548), (2_566_000, true, 2565),
            (2_576_667, false, 2565), (2_593_333, false, 2592),
            (2_610_000, false, 2609), (2_626_667, false, 2625),
            (2_636_667, false, 2635), (2_653_333, false, 2652),
            (2_670_000, false, 2669), (2_686_667, false, 2685),
            (2_703_333, false, 2702), (2_720_000, false, 2719),
            (2_736_667, false, 2735),
        ] {
            assert_eq!(driver.advance(Duration::from_micros(wall), true, pause),
                Duration::from_millis(logical), "at {wall}us");
        }
        assert_eq!(driver.advance(Duration::from_micros(2_916_667), false, false),
            Duration::from_millis(2916));
    }
}
