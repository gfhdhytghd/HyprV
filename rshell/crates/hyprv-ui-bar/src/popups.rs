use crate::{
    format,
    input::PopupCommand,
    layout::{self, contains},
    model::*,
    surfaces, view,
};
use hyprv_ui::{
    geometry::Rect,
    scene::{Border, Content, Node},
};
use hyprv_widgets::flickable::{Flickable, Wheel};
use hyprv_widgets::{
    containers::{
        glass_panel as widget_glass_panel, popup_position, GlassStyle, PopupPlacement, Reveal,
        Transition,
    },
    controls::{Button, Cursor, PointerEvent},
    foundations::{hcenter, rgb, with_alpha, Palette},
    primitives::{clip_box, rounded_rect, FontSpec, Label},
};
use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Menu,
    Overflow,
    Battery,
}
#[derive(Clone, Debug)]
pub struct SurfaceState {
    pub output: Option<Output>,
    pub mapped: bool,
    pub requested: bool,
    pub closing: bool,
    pub anchor: Rect,
    pub source: Option<Target>,
    pub reveal: Reveal,
    pub open_at: Option<Duration>,
    waiting_ready: bool,
}
impl SurfaceState {
    fn new(height: f32) -> Self {
        Self {
            output: None,
            mapped: false,
            requested: false,
            closing: false,
            anchor: Rect::default(),
            source: None,
            reveal: Reveal::new(height),
            open_at: None,
            waiting_ready: false,
        }
    }
    fn open(&mut self, output: &Output, anchor: Rect, source: Target, now: Duration) {
        self.output = Some(output.clone());
        self.anchor = anchor;
        self.source = Some(source);
        self.mapped = true;
        self.requested = true;
        self.closing = false;
        self.reveal.prepare_open();
        self.waiting_ready = false;
        self.open_at = Some(now + Duration::from_millis(16));
    }
    fn close(&mut self, now: Duration) {
        if self.mapped {
            self.requested = false;
            self.closing = true;
            self.open_at = None;
            self.waiting_ready = false;
            self.reveal.play_close(now);
        }
    }
    fn unmap(&mut self) {
        self.mapped = false;
        self.requested = false;
        self.closing = false;
        self.source = None;
        self.open_at = None;
        self.waiting_ready = false;
        self.reveal.reset();
    }
    fn advance(&mut self, now: Duration, ready: bool) -> bool {
        // Readiness changes arrive through the service subscription, never a
        // repeating timer. Keep one staging turn when hydration finally arrives.
        if self.waiting_ready && ready && self.requested {
            self.waiting_ready = false;
            self.open_at = Some(now + Duration::from_millis(16));
        }
        if self.open_at.is_some_and(|t| now >= t) {
            if ready {
                self.open_at = None;
                self.reveal.play_open(now);
            } else {
                self.open_at = None;
                self.waiting_ready = true;
            }
        }
        if self.reveal.advance(now) == Some(Transition::Close) && self.closing {
            self.mapped = false;
            self.closing = false;
            self.reveal.reset();
            self.source = None;
            true
        } else {
            false
        }
    }
}
#[derive(Clone, Debug)]
pub struct Menu {
    pub surface: SurfaceState,
    pub item: Option<String>,
    pub stack: Vec<i32>,
    pub scroll: f32,
    scrolling: Flickable,
    pub hovered: Option<i32>,
    pressed: Option<i32>,
    activate_close_at: Option<Duration>,
    clear_at: Option<Duration>,
}
#[derive(Clone, Debug)]
pub struct Overflow {
    pub surface: SurfaceState,
    pub hovered: Option<String>,
    pressed: Option<(String, Button)>,
}
#[derive(Clone, Debug)]
pub struct Battery {
    pub surface: SurfaceState,
}
pub struct Popups {
    pub menu: Menu,
    pub overflow: Overflow,
    pub battery: Battery,
}
impl Default for Popups {
    fn default() -> Self {
        let mut menu = SurfaceState::new(18.);
        menu.reveal.timing.close_reveal = 200;
        menu.reveal.timing.close_offset = -6.;
        let mut battery = SurfaceState::new(200.);
        battery.reveal.timing.open_pause = 20;
        battery.reveal.timing.close_pause = 30;
        Self {
            menu: Menu {
                surface: menu,
                item: None,
                stack: vec![],
                scroll: 0.,
                scrolling: Flickable::default(),
                hovered: None,
                pressed: None,
                activate_close_at: None,
                clear_at: None,
            },
            overflow: Overflow {
                surface: SurfaceState::new(54.),
                hovered: None,
                pressed: None,
            },
            battery: Battery { surface: battery },
        }
    }
}
#[derive(Default)]
pub struct PopupInput {
    pub actions: Vec<Action>,
    pub redraw: bool,
    pub consumed: bool,
    pub cursor: Option<Cursor>,
    pub command: Option<PopupCommand>,
}
fn style(theme: &Theme) -> GlassStyle {
    let p = Palette::new(theme.dark);
    GlassStyle {
        fill: p.glass_fill,
        stroke: p.glass_stroke,
        shadow: with_alpha(
            rgb(if theme.dark { "#000000" } else { "#111111" }),
            if theme.dark { 0.45 } else { 0.18 },
        ),
        ..Default::default()
    }
}
// AnimatedGlassPanel.qml:184-214: opacity belongs to the source rectangle,
// before MultiEffect's shadow composition, rather than to quantized color tokens.
fn glass_panel(
    frame: Rect,
    reveal: &Reveal,
    style: &GlassStyle,
    children: Vec<Node>,
    scale: f32,
) -> Node {
    let mut root = widget_glass_panel(frame, reveal, style, children);
    if let Some(chrome) = root
        .children
        .first_mut()
        .and_then(|n| n.children.first_mut())
        .and_then(|n| n.children.first_mut())
    {
        // AnimatedGlassPanel.qml:185: explicit unpadded layer.textureSize is
        // retained even when MultiEffect expands the captured source rectangle.
        if let Some(effect) = chrome.effects.multi_effect.as_mut() {
            effect.source_texture_size = Some((
                (chrome.frame.width * scale).round().max(1.) as u32,
                (chrome.frame.height * scale).round().max(1.) as u32,
            ));
        }
        let mut source = Node::new(
            Rect::new(0., 0., chrome.frame.width, chrome.frame.height),
            chrome.content.clone(),
        );
        source.opacity = style.surface_opacity;
        if let Content::Rect(r) = &mut source.content {
            r.fill = hyprv_ui::scene::Fill::Solid(style.fill);
            if let Some(b) = &mut r.border {
                b.color = style.stroke;
            }
        }
        chrome.content = Content::Group;
        chrome.children = vec![source];
    }
    root
}
/// Settled bar glass (tray-menu tokens) for other bar-owned popups such as the tooltip.
pub(crate) fn glass(frame: Rect, theme: &Theme, children: Vec<Node>, scale: f32) -> Node {
    glass_panel(frame, &Reveal::new(frame.height), &style(theme), children, scale)
}
fn position(m: &Model, size: (f32, f32), anchor: Rect) -> Rect {
    popup_position(
        (m.output.width, m.output.height),
        size,
        anchor,
        PopupPlacement::AnchorCentered,
        10.,
    )
}
fn root(m: &Model) -> Node {
    let mut n = view::group(Rect::new(0., 0., m.output.width, m.output.height));
    n.interactive = true;
    n.native_materials=Some(true);
    n
}
fn find_entry(node: &TrayMenuEntry, id: i32) -> Option<&TrayMenuEntry> {
    if node.id == id {
        Some(node)
    } else {
        node.children.iter().find_map(|n| find_entry(n, id))
    }
}
impl Popups {
    pub fn command(&mut self, m: &Model, command: PopupCommand, now: Duration) -> Vec<Action> {
        match command {
            PopupCommand::Menu {
                item,
                anchor,
                source,
            } => {
                let mut actions = vec![];
                if let Some(previous) = &self.menu.item {
                    if previous != &item {
                        actions.push(Action::Tray(TrayAction::MenuClose {
                            item: previous.clone(),
                        }));
                    }
                }
                self.menu.item = Some(item.clone());
                self.menu.hovered = None;
                self.menu.pressed = None;
                self.menu.stack.clear();
                self.menu.scroll = 0.;
                self.menu.scrolling.reset(0.);
                self.menu.activate_close_at = None;
                self.menu.clear_at = None;
                self.menu.surface.open(&m.output, anchor, source, now);
                actions.push(Action::Tray(TrayAction::MenuOpen { item }));
                actions
            }
            PopupCommand::Overflow { anchor, source } => {
                self.overflow.pressed = None;
                self.overflow.hovered = None;
                if self.overflow.surface.mapped
                    && self.overflow.surface.source.as_ref() == Some(&source)
                    && self.overflow.surface.output.as_ref().map(|o| &o.name)
                        == Some(&m.output.name)
                {
                    self.close_overflow();
                } else {
                    self.overflow.surface.open(&m.output, anchor, source, now);
                    self.overflow.surface.open_at = None;
                    self.overflow.surface.reveal.reset();
                }
                vec![]
            }
            PopupCommand::Battery { anchor, source } => {
                if self.battery.surface.mapped
                    && self.battery.surface.source.as_ref() == Some(&source)
                    && self.battery.surface.output.as_ref().map(|o| &o.name) == Some(&m.output.name)
                {
                    self.battery.surface.close(now);
                    vec![Action::AcquireBatteryDetails(false)]
                } else {
                    self.battery.surface.open(&m.output, anchor, source, now);
                    vec![Action::AcquireBatteryDetails(true)]
                }
            }
        }
    }
    pub fn surface(&self, kind: Kind) -> &SurfaceState {
        match kind {
            Kind::Menu => &self.menu.surface,
            Kind::Overflow => &self.overflow.surface,
            Kind::Battery => &self.battery.surface,
        }
    }
    fn bound_model(&self, m: &Model, kind: Kind) -> Model {
        let mut bound = m.clone();
        if let Some(output) = &self.surface(kind).output {
            bound.output = output.clone();
        }
        bound
    }
    pub fn entries<'a>(&self, m: &'a Model) -> Vec<&'a TrayMenuEntry> {
        let Some(menu) = m
            .tray
            .menus
            .iter()
            .find(|v| Some(&v.item_key) == self.menu.item.as_ref())
        else {
            return vec![];
        };
        let root = self
            .menu
            .stack
            .last()
            .and_then(|id| find_entry(&menu.root, *id))
            .unwrap_or(&menu.root);
        root.children.iter().filter(|e| e.visible).collect()
    }
    pub fn menu_height(&self, m: &Model) -> f32 {
        let entries = self.entries(m);
        let n = entries.len() + usize::from(!self.menu.stack.is_empty());
        let sum = entries
            .iter()
            .map(|e| if e.is_separator { 1. } else { 34. })
            .sum::<f32>()
            + if !self.menu.stack.is_empty() { 34. } else { 0. };
        (sum + n.saturating_sub(1) as f32 + 18.).min(420.)
    }
    pub fn menu_content_height(&self, m: &Model) -> f32 {
        let entries = self.entries(m);
        let n = entries.len() + usize::from(!self.menu.stack.is_empty());
        entries
            .iter()
            .map(|e| if e.is_separator { 1. } else { 34. })
            .sum::<f32>()
            + if !self.menu.stack.is_empty() { 34. } else { 0. }
            + n.saturating_sub(1) as f32
    }
    pub fn overflow_items(&self, m: &Model) -> Vec<TrayItem> {
        let bound = self.bound_model(m, Kind::Overflow);
        let l = layout::compute(&bound, &m.theme);
        l.tray_items.into_iter().skip(l.tray.visible).collect()
    }
    pub fn overflow_size(count: usize) -> (f32, f32) {
        let columns = count.clamp(1, 6);
        let rows = count.div_ceil(columns).max(1);
        (
            16. + columns as f32 * 18. + columns.saturating_sub(1) as f32 * 7.,
            16. + rows as f32 * 38.,
        )
    }
    fn close_menu(&mut self, now: Duration) {
        self.menu.pressed = None;
        self.menu.hovered = None;
        self.menu.activate_close_at = None;
        self.menu.surface.close(now);
    }
    fn close_overflow(&mut self) {
        self.overflow.surface.unmap();
        self.overflow.pressed = None;
        self.overflow.hovered = None;
    }
    pub fn update(&mut self, m: &Model, now: Duration) -> Vec<Action> {
        if self
            .menu
            .item
            .as_ref()
            .is_some_and(|key| !m.tray.items.iter().any(|i| &i.key == key))
        {
            self.menu.surface.unmap();
            self.menu.item = None;
            self.menu.pressed = None;
            self.menu.hovered = None;
            self.menu.stack.clear();
            self.menu.activate_close_at = None;
            self.menu.clear_at = None;
        }
        for surface in [
            &mut self.menu.surface,
            &mut self.overflow.surface,
            &mut self.battery.surface,
        ] {
            if surface
                .output
                .as_ref()
                .is_some_and(|o| o.name == m.output.name)
            {
                surface.output = Some(m.output.clone());
            }
        }
        let mut actions = vec![];
        let menu_height = self.menu_height(m);
        let height = menu_height;
        if self.menu.surface.reveal.full_height != height {
            let pending = self.menu.surface.open_at.is_some() || self.menu.surface.waiting_ready;
            self.menu.surface.reveal.set_full_height(height);
            if pending {
                self.menu.surface.reveal.prepare_open();
                self.menu.surface.open_at = Some(now + Duration::from_millis(16));
            }
        }
        if self.battery.surface.mapped {
            let pending = self.battery.surface.open_at.is_some();
            let height = battery_height(m);
            if self.battery.surface.reveal.full_height != height {
                self.battery.surface.reveal.set_full_height(height);
                if pending {
                    self.battery.surface.reveal.prepare_open();
                    self.battery.surface.open_at = Some(now + Duration::from_millis(16));
                }
            }
        }
        if self.menu.activate_close_at.is_some_and(|t| now >= t) {
            self.menu.activate_close_at = None;
            self.close_menu(now);
        }
        let menu_ready = !self.entries(m).is_empty() || !self.menu.stack.is_empty();
        if self.menu.surface.advance(now, menu_ready) {
            if let Some(item) = &self.menu.item {
                actions.push(Action::Tray(TrayAction::MenuClose { item: item.clone() }));
            }
            self.menu.clear_at = Some(now + Duration::from_millis(120));
        }
        if self.menu.clear_at.is_some_and(|t| now >= t) && !self.menu.surface.mapped {
            self.menu.clear_at = None;
            self.menu.item = None;
            self.menu.stack.clear();
            self.menu.scroll = 0.;
            self.menu.scrolling.reset(0.);
        }
        if self.battery.surface.advance(now, true) {
            actions.push(Action::AcquireBatteryDetails(false));
        }
        if self.overflow.surface.mapped && self.overflow_items(m).is_empty() {
            self.close_overflow();
        }
        let content_height = self.menu_content_height(m);
        self.menu
            .scrolling
            .set_extents((menu_height - 18.) as f64, content_height as f64);
        if self.menu.surface.mapped {
            self.menu.scrolling.frame(now);
        } else {
            self.menu.scrolling.reset(self.menu.scroll as f64);
        }
        self.menu.scroll = self.menu.scrolling.content_y() as f32;
        if let Some(id) = self.menu.pressed {
            let panel = self.card_rect(m, Kind::Menu);
            let bottom = panel.y + self.menu.surface.reveal.values().height;
            if !self.menu.surface.mapped
                || !self
                    .menu_rows(m)
                    .iter()
                    .any(|(row, r)| *row == id && r.y < bottom && r.y + r.height > panel.y)
            {
                self.menu.pressed = None;
            }
        }
        actions
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        [
            self.menu.surface.open_at,
            self.battery.surface.open_at,
            self.menu.activate_close_at,
            self.menu.clear_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub fn animating(&self) -> bool {
        self.menu.surface.reveal.is_running()
            || self.battery.surface.reveal.is_running()
            || self.menu.scrolling.animating()
    }
    /// xkb Escape keysym; repeat/release events need not trigger another action.
    pub fn keyboard(
        &mut self,
        kind: Kind,
        keysym: u32,
        pressed: bool,
        now: Duration,
    ) -> Vec<Action> {
        if pressed && keysym == 0xff1b {
            self.escape(kind, now)
        } else {
            vec![]
        }
    }
    pub fn update_scene(&self, scene: &mut hyprv_ui::scene::Scene, m: &Model, kind: Kind) -> usize {
        let id = scene.root().id();
        view::reconcile(scene, id, self.build(m, &m.theme, kind))
    }
    pub fn escape(&mut self, kind: Kind, now: Duration) -> Vec<Action> {
        match kind {
            Kind::Menu => {
                self.menu.pressed = None;
                self.menu.hovered = None;
                if self.menu.stack.pop().is_some() {
                    self.menu.scroll = 0.;
                    self.menu.scrolling.reset(0.);
                } else {
                    self.close_menu(now);
                }
                vec![]
            }
            Kind::Overflow => {
                self.close_overflow();
                vec![]
            }
            Kind::Battery => {
                self.battery.surface.close(now);
                vec![Action::AcquireBatteryDetails(false)]
            }
        }
    }
    pub fn requirements(&self, m: &Model) -> Vec<(Kind, surfaces::Requirement)> {
        [
            (
                Kind::Menu,
                "shell:hyprv-tray-menu",
                self.menu.surface.mapped,
            ),
            (
                Kind::Overflow,
                "shell:hyprv-tray-overflow",
                self.overflow.surface.mapped,
            ),
            (
                Kind::Battery,
                "shell:hyprv-battery-info",
                self.battery.surface.mapped,
            ),
        ]
        .into_iter()
        .map(|(k, ns, mapped)| {
            let bound = self.bound_model(m, k);
            (k, surfaces::popup(&bound, ns, mapped))
        })
        .collect()
    }
    pub fn build(&self, m: &Model, theme: &Theme, kind: Kind) -> Node {
        let bound = self.bound_model(m, kind);
        let m = &bound;
        match kind {
            Kind::Menu => self.build_menu(m, theme),
            Kind::Overflow => self.build_overflow(m, theme),
            Kind::Battery => self.build_battery(m, theme),
        }
    }
    pub fn card_rect(&self, m: &Model, kind: Kind) -> Rect {
        let bound = self.bound_model(m, kind);
        let m = &bound;
        match kind {
            Kind::Menu => position(m, (300., self.menu_height(m)), self.menu.surface.anchor),
            Kind::Overflow => position(
                m,
                Self::overflow_size(self.overflow_items(m).len()),
                self.overflow.surface.anchor,
            ),
            Kind::Battery => position(m, (324., battery_height(m)), self.battery.surface.anchor),
        }
    }
    fn menu_rows(&self, m: &Model) -> Vec<(i32, Rect)> {
        let panel = self.card_rect(m, Kind::Menu);
        let mut y = panel.y + 9. - self.menu.scroll;
        let mut rows = vec![];
        if !self.menu.stack.is_empty() {
            rows.push((i32::MIN, Rect::new(panel.x + 9., y, 282., 34.)));
            y += 35.;
        }
        for entry in self.entries(m) {
            let h = if entry.is_separator { 1. } else { 34. };
            rows.push((entry.id, Rect::new(panel.x + 9., y, 282., h)));
            y += h + 1.;
        }
        rows
    }
    fn build_menu(&self, m: &Model, theme: &Theme) -> Node {
        let mut root = root(m);
        root.visible = self.menu.surface.mapped;
        let p = Palette::new(theme.dark);
        let panel = self.card_rect(m, Kind::Menu);
        let mut rows = vec![];
        for (id, r) in self.menu_rows(m) {
            let f = Rect::new(0., r.y - panel.y - 9., r.width, r.height);
            let entry = self.entries(m).into_iter().find(|e| e.id == id);
            if entry.is_some_and(|e| e.is_separator) {
                rows.push(rounded_rect(f, 0., p.glass_stroke, None));
                continue;
            }
            let enabled = entry.map(|e| e.enabled).unwrap_or(true);
            let hovered = self.menu.hovered == Some(id) && enabled;
            let mut row = rounded_rect(
                f,
                11.,
                if hovered {
                    with_alpha(p.primary_text, if theme.dark { 0.10 } else { 0.12 })
                } else {
                    Default::default()
                },
                None,
            );
            row.interactive = enabled;
            if id == i32::MIN {
                row.add(view::text(
                    "< Back",
                    Rect::new(
                        9.,
                        hcenter(34.) - hcenter(layout::measure("< Back", 14., 400).1),
                        264.,
                        layout::measure("< Back", 14., 400).1,
                    ),
                    14.,
                    400,
                    p.primary_text,
                    false,
                ));
            } else if let Some(e) = entry {
                let mut left = 9.;
                let indicator = match e.button_type {
                    ButtonType::CheckBox => {
                        if e.check_state != CheckState::Unchecked {
                            "[x]"
                        } else {
                            "[ ]"
                        }
                    }
                    ButtonType::RadioButton => {
                        if e.check_state != CheckState::Unchecked {
                            "(o)"
                        } else {
                            "( )"
                        }
                    }
                    ButtonType::None => "",
                };
                if !indicator.is_empty() {
                    let (w, h) = layout::measure(indicator, 13., 400);
                    row.add(view::text(
                        indicator,
                        Rect::new(left, hcenter(34.) - hcenter(h), w, h),
                        13.,
                        400,
                        p.primary_text,
                        false,
                    ));
                    left += w + 8.;
                }
                if !e.icon_name.is_empty() || !e.icon_data.is_empty() {
                    if let Some(image) = m.assets.images.get(&format!(
                        "menu:{}:{}",
                        self.menu.item.as_deref().unwrap_or(""),
                        e.id
                    )) {
                        row.add(Node::new(
                            Rect::new(left, 9., 16., 16.),
                            Content::Image(image.clone()),
                        ));
                    }
                    left += 24.;
                }
                let mut right = 273.;
                if e.has_children {
                    let (w, h) = layout::measure(">", 14., 400);
                    row.add(view::text(
                        ">",
                        Rect::new(273. - w, hcenter(34.) - hcenter(h), w, h),
                        14.,
                        400,
                        p.primary_text,
                        false,
                    ));
                    right -= w + 8.;
                }
                let h = layout::measure(&e.text, 14., 400).1;
                row.add(view::text(
                    &e.text,
                    Rect::new(left, hcenter(34.) - hcenter(h), (right - left).max(0.), h),
                    14.,
                    400,
                    if enabled {
                        p.primary_text
                    } else {
                        with_alpha(p.primary_text, 0.55)
                    },
                    true,
                ));
            }
            rows.push(row);
        }
        let mut viewport = clip_box(Rect::new(9., 9., 282., (panel.height - 18.).max(0.)), rows);
        if !QUIRK_GLASS_ANCHORED_CONTENT_IGNORES_SLIDE {
            viewport.frame.y += self.menu.surface.reveal.values().offset;
        }
        root.add(glass_panel(
            panel,
            &self.menu.surface.reveal,
            &style(theme),
            vec![viewport],
            m.output.scale,
        ));
        root
    }
    fn build_overflow(&self, m: &Model, theme: &Theme) -> Node {
        let mut root = root(m);
        root.visible = self.overflow.surface.mapped;
        let p = Palette::new(theme.dark);
        let items = self.overflow_items(m);
        let panel = self.card_rect(m, Kind::Overflow);
        let mut card = rounded_rect(
            panel,
            18.,
            p.glass_fill,
            Some(Border {
                width: 1.,
                color: p.glass_stroke,
            }),
        );
        let columns = items.len().clamp(1, 6);
        for (index, item) in items.iter().enumerate() {
            let module = layout::Module {
                target: Target::Tray(item.key.clone()),
                label: item
                    .id
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".into()),
                frame: Rect::new(
                    8. + (index % columns) as f32 * 25.,
                    8. + (index / columns) as f32 * 38.,
                    18.,
                    38.,
                ),
                px: 10.,
                color: p.primary_text,
                padding: (0., 0.),
                hoverable: false,
                interactive: true,
                highlighted: false,
                highlight: p.active_workspace,
                highlighted_text: p.active_workspace_text,
            };
            card.add(view::module_node(&module, m, theme));
        }
        root.add(card);
        root
    }
    fn build_battery(&self, m: &Model, theme: &Theme) -> Node {
        let mut root = root(m);
        root.visible = self.battery.surface.mapped;
        let p = Palette::new(theme.dark);
        let panel = self.card_rect(m, Kind::Battery);
        let muted = with_alpha(p.primary_text, if theme.dark { 0.72 } else { 0.68 });
        let accent = if matches!(
            m.battery.mode,
            BatteryMode::Charging | BatteryMode::Full | BatteryMode::Plugged
        ) {
            p.battery
        } else if m.battery.mode == BatteryMode::Discharging && m.battery_percent() <= 20. {
            p.critical
        } else {
            p.primary_text
        };
        let mut children = vec![];
        let mut y = 12.;
        let title = if m.battery_available() {
            format::power_label(m)
        } else {
            "电源".into()
        };
        let title = title.as_str();
        let h = layout::measure(title, 15., 700).1;
        children.push(view::text(
            title,
            Rect::new(12., y, 300., h),
            15.,
            700,
            accent,
            false,
        ));
        y += h + 10.;
        let status = format::battery_status(&m.battery);
        let h = layout::measure(&status, 12., 600).1;
        children.push(view::text(
            &status,
            Rect::new(12., y, layout::measure(&status, 12., 600).0, h),
            12.,
            600,
            muted,
            false,
        ));
        y += h + 10.;
        children.push(rounded_rect(
            Rect::new(12., y, 300., 1.),
            1.,
            p.glass_stroke,
            None,
        ));
        y += 11.;
        for (index, (title, value)) in battery_lines(m).into_iter().enumerate() {
            let height = battery_line_height(&title, &value);
            children.push(wrapped(
                &title,
                Rect::new(12., y, 118., height),
                13.,
                600,
                muted,
                false,
            ));
            children.push(wrapped(
                &value,
                Rect::new(142., y, 170., height),
                13.,
                700,
                if index == 0 { accent } else { p.primary_text },
                true,
            ));
            y += height + 10.;
        }
        let sample = format::sample_window(&m.battery);
        if !sample.is_empty() {
            let h = wrapped_height(&sample, 11., 500, 300.);
            children.push(wrapped(
                &sample,
                Rect::new(12., y, 300., h),
                11.,
                500,
                muted,
                true,
            ));
        }
        let mut content = view::group(Rect::new(
            0.,
            if QUIRK_GLASS_ANCHORED_CONTENT_IGNORES_SLIDE {
                0.
            } else {
                self.battery.surface.reveal.values().offset
            },
            324.,
            panel.height,
        ));
        content.children = children;
        root.add(glass_panel(
            panel,
            &self.battery.surface.reveal,
            &style(theme),
            vec![content],
            m.output.scale,
        ));
        root
    }
    /// Native menu wheel input, preserving Qt touchpad phases/pixel deltas.
    /// The legacy `pointer` adapter remains available for unphased input.
    pub fn menu_wheel(&mut self, m: &Model, event: Wheel, now: Duration) -> PopupInput {
        if !self.menu.surface.mapped {
            return PopupInput::default();
        }
        let height = self.menu_height(m) - 18.;
        let content = self.menu_content_height(m);
        self.menu
            .scrolling
            .set_extents(height as f64, content as f64);
        let accepted = self.menu.scrolling.wheel(event, now);
        self.menu.scroll = self.menu.scrolling.content_y() as f32;
        PopupInput {
            consumed: accepted,
            redraw: accepted,
            ..Default::default()
        }
    }

    pub fn pointer(
        &mut self,
        m: &Model,
        kind: Kind,
        event: PointerEvent,
        now: Duration,
    ) -> PopupInput {
        let bound = self.bound_model(m, kind);
        let m = &bound;
        let mut result = PopupInput {
            consumed: true,
            ..Default::default()
        };
        if !self.surface(kind).mapped {
            match kind {
                Kind::Menu => {
                    self.menu.pressed = None;
                    self.menu.hovered = None;
                }
                Kind::Overflow => {
                    self.overflow.pressed = None;
                    self.overflow.hovered = None;
                }
                Kind::Battery => {}
            }
            result.consumed = false;
            return result;
        }
        let panel = self.card_rect(m, kind);
        match kind {
            Kind::Menu => {
                let viewport = Rect::new(panel.x + 9., panel.y + 9., 282., panel.height - 18.);
                let content_height = self.menu_content_height(m);
                self.menu
                    .scrolling
                    .set_extents(viewport.height as f64, content_height as f64);
                let before_scroll = self.menu.scroll;
                let mut scroll_consumed = false;
                match event {
                    PointerEvent::Press {
                        x,
                        y,
                        button: Button::Left,
                    } if contains(viewport, x, y)
                        && y < panel.y + self.menu.surface.reveal.values().height =>
                    {
                        self.menu.scrolling.press(y as f64, now);
                        scroll_consumed = self.menu.scrolling.grabbed();
                    }
                    PointerEvent::Move { y, .. } => {
                        scroll_consumed = self.menu.scrolling.drag_to(y as f64, None, now);
                    }
                    PointerEvent::Release {
                        y,
                        button: Button::Left,
                        ..
                    } => {
                        scroll_consumed = self.menu.scrolling.release(y as f64, now);
                    }
                    PointerEvent::Cancel => self.menu.scrolling.cancel(),
                    _ => {}
                }
                self.menu.scroll = self.menu.scrolling.content_y() as f32;
                result.redraw = before_scroll != self.menu.scroll;
                if scroll_consumed {
                    self.menu.pressed = None;
                    self.menu.hovered = None;
                    result.redraw = true;
                    return result;
                }
                let rows = self.menu_rows(m);
                let reveal = Rect::new(
                    panel.x,
                    panel.y,
                    panel.width,
                    self.menu
                        .surface
                        .reveal
                        .values()
                        .height
                        .min(panel.height)
                        .max(0.),
                );
                let hit = |x, y| {
                    if contains(viewport, x, y) && contains(reveal, x, y) {
                        rows.iter()
                            .find(|(id, r)| {
                                contains(*r, x, y)
                                    && (*id == i32::MIN
                                        || self
                                            .entries(m)
                                            .iter()
                                            .any(|e| e.id == *id && e.enabled && !e.is_separator))
                            })
                            .map(|(id, _)| *id)
                    } else {
                        None
                    }
                };
                match event {
                    PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => {
                        let h = hit(x, y);
                        result.redraw |= h != self.menu.hovered;
                        self.menu.hovered = h;
                        result.cursor = Some(if h.is_some() {
                            Cursor::Pointer
                        } else {
                            Cursor::Default
                        });
                    }
                    PointerEvent::Leave => {
                        result.redraw = self.menu.hovered.take().is_some();
                    }
                    PointerEvent::Cancel => self.menu.pressed = None,
                    PointerEvent::Press {
                        x,
                        y,
                        button: Button::Left,
                    } => self.menu.pressed = hit(x, y),
                    PointerEvent::Release {
                        x,
                        y,
                        button: Button::Left,
                    } => {
                        let h = hit(x, y);
                        if let Some(id) = h {
                            if self.menu.pressed.take() == Some(id) {
                                if id == i32::MIN {
                                    self.menu.stack.pop();
                                    // Repeater replaces the page's MouseAreas. A hover
                                    // belongs to the old delegate, even if ids are reused.
                                    self.menu.hovered = None;
                                    self.menu.scroll = 0.;
                                    self.menu.scrolling.reset(0.);
                                    result.redraw = true;
                                } else if let Some(entry) =
                                    self.entries(m).into_iter().find(|e| e.id == id)
                                {
                                    if entry.enabled && !entry.is_separator {
                                        let has_children = entry.has_children;
                                        let item = self.menu.item.clone().unwrap_or_default();
                                        if has_children {
                                            self.menu.stack.push(id);
                                            self.menu.hovered = None;
                                            self.menu.scroll = 0.;
                                            self.menu.scrolling.reset(0.);
                                            result.actions.push(Action::Tray(
                                                TrayAction::MenuAboutToShow { item, entry: id },
                                            ));
                                        } else {
                                            result.actions.push(Action::Tray(
                                                TrayAction::MenuEvent {
                                                    item,
                                                    entry: id,
                                                    event: "clicked".into(),
                                                },
                                            ));
                                            self.menu.activate_close_at =
                                                Some(now + Duration::from_millis(80));
                                        }
                                        result.redraw = true;
                                    }
                                }
                            }
                        } else {
                            self.menu.pressed = None;
                            self.close_menu(now);
                            result.redraw = true;
                        }
                    }
                    PointerEvent::Release { button, .. } if button != Button::Left => {
                        self.menu.pressed = None;
                        self.close_menu(now);
                        result.redraw = true;
                    }
                    PointerEvent::Wheel {
                        angle_y, pixel_y, ..
                    } => {
                        result.redraw |= self
                            .menu
                            .scrolling
                            .wheel(Wheel::deltas(angle_y as f64, pixel_y as f64), now);
                        self.menu.scroll = self.menu.scrolling.content_y() as f32;
                    }
                    _ => (),
                }
            }
            Kind::Battery => {
                if let PointerEvent::Release { x, y, .. } = event {
                    if !contains(panel, x, y) {
                        self.battery.surface.close(now);
                        result.actions.push(Action::AcquireBatteryDetails(false));
                        result.redraw = true;
                    }
                }
            }
            Kind::Overflow => {
                let items = self.overflow_items(m);
                let columns = items.len().clamp(1, 6);
                let hit = |x, y| {
                    items
                        .iter()
                        .enumerate()
                        .find(|(i, _)| {
                            contains(
                                Rect::new(
                                    panel.x + 8. + (*i % columns) as f32 * 25.,
                                    panel.y + 8. + (*i / columns) as f32 * 38.,
                                    18.,
                                    38.,
                                ),
                                x,
                                y,
                            )
                        })
                        .map(|(_, item)| item)
                };
                match event {
                    PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => {
                        self.overflow.hovered = hit(x, y).map(|i| i.key.clone())
                    }
                    PointerEvent::Leave => self.overflow.hovered = None,
                    PointerEvent::Press { x, y, button } => {
                        self.overflow.pressed = hit(x, y).map(|i| (i.key.clone(), button))
                    }
                    PointerEvent::Release { x, y, button } => {
                        let pressed = self.overflow.pressed.take();
                        if let Some(item) = hit(x, y) {
                            if pressed == Some((item.key.clone(), button)) {
                                let index = items.iter().position(|i| i.key == item.key).unwrap();
                                let anchor = Rect::new(
                                    panel.x + 8. + (index % columns) as f32 * 25.,
                                    panel.y + 8. + (index / columns) as f32 * 38.,
                                    18.,
                                    38.,
                                );
                                if (button == Button::Right || item.only_menu) && item.has_menu {
                                    result.command = Some(PopupCommand::Menu {
                                        item: item.key.clone(),
                                        anchor,
                                        source: Target::Tray(item.key.clone()),
                                    });
                                } else {
                                    let (x, y) = (
                                        (m.output.global_x + anchor.x).round() as i32,
                                        (m.output.global_y + anchor.y + 38.).round() as i32,
                                    );
                                    if button == Button::Left && !item.only_menu {
                                        result.actions.push(Action::Tray(TrayAction::Activate {
                                            item: item.key.clone(),
                                            x,
                                            y,
                                        }));
                                    } else if button == Button::Right {
                                        result.actions.push(Action::Tray(
                                            TrayAction::SecondaryActivate {
                                                item: item.key.clone(),
                                                x,
                                                y,
                                            },
                                        ));
                                    }
                                }
                            }
                        } else {
                            self.close_overflow();
                            result.redraw = true;
                        }
                    }
                    PointerEvent::Wheel {
                        angle_y,
                        pixel_y,
                        angle_x,
                        pixel_x,
                    } => {
                        if let Some(item) = &self.overflow.hovered {
                            let y = if angle_y != 0. { angle_y } else { pixel_y };
                            if y != 0. || angle_x != 0. || pixel_x != 0. {
                                result.actions.push(Action::Tray(TrayAction::Scroll {
                                    item: item.clone(),
                                    delta: if y > 0. {
                                        1
                                    } else if y < 0. || QUIRK_HORIZONTAL_TRAY_WHEEL_SENDS_MINUS_ONE
                                    {
                                        -1
                                    } else {
                                        0
                                    },
                                    horizontal: false,
                                }));
                            }
                        }
                    }
                    PointerEvent::Cancel => self.overflow.pressed = None,
                }
            }
        }
        result
    }
}
fn battery_lines(m: &Model) -> Vec<(String, String)> {
    vec![
        (
            "当前净功率".into(),
            if m.battery.available {
                format::watts(m.battery.power_w)
            } else {
                "--".into()
            },
        ),
        (
            "半小时平均功率".into(),
            if m.battery.available {
                format::watts(m.battery.average_power_w)
            } else {
                "--".into()
            },
        ),
        (
            if matches!(
                m.battery.mode,
                BatteryMode::Charging | BatteryMode::Full | BatteryMode::Plugged
            ) {
                "预计剩余充电时间"
            } else {
                "预计剩余使用时间"
            }
            .into(),
            format::estimate(&m.battery),
        ),
    ]
}
fn wrapped(
    text: &str,
    frame: Rect,
    px: f32,
    weight: i32,
    color: hyprv_ui::animation::Color,
    right: bool,
) -> Node {
    let mut label = Label::new(
        text,
        FontSpec {
            pixel_size: px,
            weight,
            ..Default::default()
        },
        color,
    );
    label.max_lines = None;
    label.elide = false;
    if right {
        label.align = hyprv_ui::skia::textlayout::TextAlign::Right;
    }
    let mut node=label.node(frame);
    node.native_text_mode=hyprv_ui::scene::NativeTextMode::QtGl;
    node
}
fn wrapped_height(text: &str, px: f32, weight: i32, width: f32) -> f32 {
    let text = hyprv_ui::scene::Text {
        text: text.into(),
        families: vec![hyprv_widgets::foundations::BASE_FONT.into(), "HarmonyOS Sans SC".into()],
        size: px, weight, max_lines: None, elide: false,
        ..Default::default()
    };
    hyprv_ui::text::measure_wrapped(&text, width)
        .expect("wrapped label measurement")
        .height.ceil().max(layout::measure(&text.text, px, weight).1)
}
fn battery_line_height(title: &str, value: &str) -> f32 {
    wrapped_height(title, 13., 600, 118.).max(wrapped_height(value, 13., 700, 170.))
}
pub fn battery_height(m: &Model) -> f32 {
    let title = if m.battery_available() {
        format::power_label(m)
    } else {
        "电源".into()
    };
    let title = title.as_str();
    let status = format::battery_status(&m.battery);
    let mut height = 24.
        + layout::measure(title, 15., 700).1
        + 10.
        + layout::measure(&status, 12., 600).1
        + 10.
        + 1.;
    for (title, value) in battery_lines(m) {
        height += 10. + battery_line_height(&title, &value);
    }
    let sample = format::sample_window(&m.battery);
    if !sample.is_empty() {
        height += 10. + wrapped_height(&sample, 11., 500, 300.);
    }
    height
}
