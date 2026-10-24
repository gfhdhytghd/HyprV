//! W12 DialogButton — StartupConfirm `ActionButton` (`StartupConfirm.qml:116-157`,
//! ui-panels-calendar.md §4.3): QtQuick.Controls.Basic Button, 88×32, radius 8, focus ring.
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::rgb;
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

pub const DIALOG_FONT: &str = "HarmonyOS Sans SC";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKey {
    Return,
    Enter,
    Space,
    Other,
}

#[derive(Clone, Debug)]
pub struct DialogButton {
    pub frame: Rect,
    pub text: String,
    /// `SystemPalette.window.hslLightness < 0.5` (not the shell's darkMode).
    pub dark: bool,
    /// Keyboard focus obtained with a Tab/keyboard reason (`visualFocus`).
    pub visual_focus: bool,
    /// `root.keyboardReady` (150 ms after start); keys are swallowed before that.
    pub keyboard_ready: bool,
    pub state: Interaction,
    /// Basic Button keeps `down` while a key activation is held.
    pub key_down: bool,
}

impl DialogButton {
    pub fn new(text: impl Into<String>, dark: bool) -> Self {
        Self {
            frame: Rect::new(0., 0., 88., 32.),
            text: text.into(),
            dark,
            visual_focus: false,
            keyboard_ready: true,
            state: Interaction::default(),
            key_down: false,
        }
    }
    pub fn foreground(&self) -> Color {
        rgb(if self.dark { "#cdd6f4" } else { "#2b2b2c" })
    }
    pub fn accent(&self) -> Color {
        rgb(if self.dark { "#89b4fa" } else { "#1e66f5" })
    }
    /// `Keys.onPressed` (`StartupConfirm.qml:123-130`): returns `Some(())` when activated.
    pub fn key_press(&mut self, key: DialogKey, auto_repeat: bool) -> Option<()> {
        if !self.keyboard_ready || auto_repeat {
            return None;
        }
        matches!(key, DialogKey::Return | DialogKey::Enter | DialogKey::Space).then_some(())
    }
}

impl Control for DialogButton {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (88., 32.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, _: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let down = self.state.pressed.is_some() || self.key_down;
        let bg = match (self.dark, down, self.state.hovered) {
            (true, true, _) => "#585b70",
            (true, false, true) => "#45475a",
            (true, false, false) => "#313244",
            (false, true, _) => "#d9d9dd",
            (false, false, true) => "#e3e3e6",
            (false, false, false) => "#ededed",
        };
        let mut root = kit::group(f);
        root.add(kit::qt_rect(Rect::new(0., 0., f.width, f.height), 8., rgb(bg), None));
        if self.visual_focus {
            root.add(kit::qt_rect(
                Rect::new(-3., -3., f.width + 6., f.height + 6.),
                11.,
                Color::rgba(0., 0., 0., 0.),
                Some((2., self.accent())),
            ));
        }
        // contentItem = button minus padding (top/bottom 6, left/right 8); text centred in it
        // by Text alignment (fractional, not the anchor rule).
        let font = Font {
            px: 13.,
            weight: kit::MEDIUM,
        };
        let m = kit::measure_in(DIALOG_FONT, &self.text, 13., kit::MEDIUM, 0.);
        let exact_h = kit::line_height_exact(DIALOG_FONT, 13., kit::MEDIUM);
        let (cw, ch) = (f.width - 16., f.height - 12.);
        let x = 8. + (cw - m.width) / 2.;
        let y = 6. + (ch - exact_h) / 2.;
        root.add(kit::qt_rendering(kit::text_in(
            DIALOG_FONT,
            &self.text,
            x,
            y,
            None,
            &font,
            self.foreground(),
            0.,
            false,
        )));
        root
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<()> {
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        let mut r = Response::redraw(feed.changed).with_cursor(Cursor::Default);
        if feed.clicked == Some(Button::Left) {
            r = r.act(());
        }
        r
    }
}
