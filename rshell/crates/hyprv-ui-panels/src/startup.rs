//! StartupConfirm (ui-panels-calendar.md §4): a one-shot dialog. The host writes
//! `launch\n` to `$HYPRV_STARTUP_APPROVAL` on [`HostAction::StartupDecision`]`(true)`
//! and quits on either decision.
use crate::{
    model::*,
    qtext::{Align, Font, TextBox, MEDIUM, NORMAL},
    ui::*,
};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Node, NodeId, Scene},
};
use std::time::Duration;

pub const WIDTH: f32 = 360.;
const HARMONY: &str = "HarmonyOS Sans SC";
const READY_MS: u64 = 150;
/// StartupConfirm.qml colours follow `SystemPalette`, not the shell theme. The shell's
/// Qt platform theme (qt6ct) yields a light window palette in both shell themes, so the
/// dialog is always light (matches parity/reference/panels startup-confirm--dark).
pub const QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Z {
    Skip,
    Launch,
}

pub struct StartupConfirm {
    /// `SystemPalette.window.hslLightness < 0.5` (platform palette, not the shell theme).
    pub dark: bool,
    pub decided: bool,
    started: Duration,
    keyboard_ready: bool,
    /// Focused button; `visual` = focus came from the keyboard (focus ring shown).
    focus: Z,
    visual: bool,
    inter: Interaction<Z>,
    zones: Vec<Zone<Z>>,
    root: Option<NodeId>,
    key: String,
}

impl StartupConfirm {
    pub fn new(dark: bool, now: Duration) -> Self {
        // Component.onCompleted: skip.forceActiveFocus(Qt.TabFocusReason) → ring visible
        let dark = dark && !QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT;
        Self { dark, decided: false, started: now, keyboard_ready: false, focus: Z::Skip, visual: true, inter: Interaction::default(), zones: Vec::new(), root: None, key: String::new() }
    }

    fn colors(&self) -> (Color, Color) {
        (rgb(if self.dark { "#cdd6f4" } else { "#2b2b2c" }), rgb(if self.dark { "#89b4fa" } else { "#1e66f5" }))
    }

    fn title(&self) -> TextBox {
        TextBox::new("启动用户应用", Font::qt(16., MEDIUM).family(HARMONY), self.colors().0).width(WIDTH - 48.).line_height(24.)
    }
    fn body(&self) -> TextBox {
        TextBox::new("启动常用应用并恢复工作区布局。", Font::qt(14., NORMAL).family(HARMONY), rgb(if self.dark { "#a6adc8" } else { "#5e5e63" })).width(WIDTH - 48.).wrap().line_height(22.)
    }
    pub fn height(&self) -> f32 {
        let t = text_size(&self.title()).1;
        let b = text_size(&self.body()).1;
        24. + t + 6. + b + 24. + 32. + 24.
    }

    /// Show/hide the keyboard focus ring (`visualFocus`). Offscreen captures of the QML
    /// run in an unfocused window, where Qt draws no ring.
    pub fn set_focus_ring(&mut self, on: bool) {
        self.visual = on;
    }

    pub fn surface(&self) -> SurfaceSpec {
        SurfaceSpec {
            namespace: "hyprv-startup-confirm",
            layer: Layer::Overlay,
            anchors: [false; 4],
            exclusive_zone: -1, // ExclusionMode.Ignore
            keyboard: KeyboardMode::Exclusive,
            size: Some((WIDTH, self.height())),
            input_region: None,
            output: String::new(),
        }
    }

    pub fn tick(&mut self, now: Duration) {
        if !self.keyboard_ready && now >= self.started + ms(READY_MS) {
            self.keyboard_ready = true;
            self.focus = Z::Skip;
            self.visual = true;
        }
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        (!self.keyboard_ready).then(|| self.started + ms(READY_MS))
    }

    pub fn build(&mut self, dpr: f32) -> Node {
        let mut n = self.build_raw(dpr);
        crate::qrect::convert(&mut n, (0., 0.), dpr);
        n.native_materials = Some(true);
        n
    }
    fn build_raw(&mut self, dpr: f32) -> Node {
        let (fg, accent) = self.colors();
        let h = self.height();
        let mut zones = Vec::new();
        let mut card = rect(
            Rect::new(0., 0., WIDTH, h),
            12.,
            rgb(if self.dark { "#1e1e2e" } else { "#fafafb" }),
            Some((1., if self.dark { hyprv_widgets::foundations::parse_color("#20ffffff").unwrap() } else { hyprv_widgets::foundations::parse_color("#14000000").unwrap() })),
        );
        let title = self.title();
        let body = self.body();
        let (_, th) = text_size(&title);
        let (_, bh) = text_size(&body);
        card.children.push(text(&title, Rect::new(24., 24., WIDTH - 48., th), (0., 0.), dpr));
        card.children.push(text(&body, Rect::new(24., 24. + th + 6., WIDTH - 48., bh), (0., 0.), dpr));
        let by = 24. + th + 6. + bh + 24.;
        let row_x = WIDTH - 24. - (88. * 2. + 8.);
        for (i, (label, id)) in [("本次跳过", Z::Skip), ("启动", Z::Launch)].into_iter().enumerate() {
            let f = Rect::new(row_x + i as f32 * 96., by, 88., 32.);
            let down = self.inter.is_pressed(&id);
            let hovered = self.inter.is_hovered(&id);
            let bg = if self.dark {
                rgb(if down { "#585b70" } else if hovered { "#45475a" } else { "#313244" })
            } else {
                rgb(if down { "#d9d9dd" } else if hovered { "#e3e3e6" } else { "#ededed" })
            };
            let mut b = rect(f, 8., bg, None);
            if self.focus == id && self.visual {
                b.children.push(rect(Rect::new(-3., -3., 94., 38.), 11., Color::default(), Some((2., accent))));
            }
            // Basic Button: padding 6, horizontalPadding 8 → content 72×20, text centred
            let lt = TextBox::new(label, Font::qt(13., MEDIUM).family(HARMONY), fg).width(72.).align(Align::Center);
            let lh = text_size(&lt).1;
            b.children.push(text(&lt, Rect::new(8., 6. + (20. - lh) / 2., 72., lh), (f.x, f.y), dpr));
            card.children.push(b);
            zones.push(Zone { id, rect: f, cursor: Cursor::Default, enabled: true });
        }
        self.zones = zones;
        let root = group_with(Rect::new(0., 0., WIDTH, h), vec![card]);
        self.root = Some(root.id());
        self.key = self.key_of();
        root
    }
    fn key_of(&self) -> String {
        format!("{}{:?}{}{:?}{:?}", self.dark, self.focus, self.visual, self.inter.hovered, self.inter.pressed)
    }
    pub fn update(&mut self, scene: &mut Scene, dpr: f32) -> bool {
        let Some(root) = self.root else { return false };
        if self.key_of() == self.key {
            return false;
        }
        let n = self.build(dpr);
        let ch = n.children;
        self.root = Some(root);
        scene.edit(root, |r| r.children = ch)
    }

    fn finish(&mut self, launch: bool) -> Vec<Action> {
        if self.decided {
            return vec![];
        }
        self.decided = true;
        vec![Action::Host(HostAction::StartupDecision(launch))]
    }

    pub fn key(&mut self, ev: &KeyEvent) -> Reply<Action> {
        let mut r = Reply::default();
        if !self.keyboard_ready || ev.autorepeat {
            return r; // swallowed
        }
        if !ev.pressed {
            return r;
        }
        r.redraw = true;
        match ev.key {
            Key::Escape | Key::AltF4 => r.actions = self.finish(false),
            Key::Return | Key::Enter | Key::Space => r.actions = self.finish(self.focus == Z::Launch),
            Key::Right if self.focus == Z::Skip => {
                self.focus = Z::Launch;
                self.visual = true;
            }
            Key::Left if self.focus == Z::Launch => {
                self.focus = Z::Skip;
                self.visual = true;
            }
            Key::Tab | Key::BackTab => {
                self.focus = if self.focus == Z::Skip { Z::Launch } else { Z::Skip };
                self.visual = true;
            }
            _ => r.redraw = false,
        }
        r
    }

    pub fn pointer(&mut self, ev: Pointer) -> Reply<Action> {
        let mut r = Reply::default();
        let zones = self.zones.clone();
        match self.inter.feed(ev, &zones) {
            Hit::Hover => r.redraw = true,
            Hit::Pressed(id, Button::Left, ..) => {
                // Button focusPolicy StrongFocus: mouse focus (no ring)
                self.focus = id;
                self.visual = false;
                r.redraw = true;
            }
            Hit::Released { id, button: Button::Left, inside: true, .. } => {
                r.redraw = true;
                r.actions = self.finish(id == Z::Launch);
            }
            Hit::Released { .. } => r.redraw = true,
            _ => {}
        }
        r.cursor = Some(Cursor::Default);
        r
    }
}
