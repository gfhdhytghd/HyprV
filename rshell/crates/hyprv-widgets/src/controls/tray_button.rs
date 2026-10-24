//! W14 TrayButton — `TrayButton.qml` (ui-bar-layout.md §8.5.1).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{
    geometry::Rect,
    scene::{Content, Node},
    skia,
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayButtonAction {
    /// SNI `Activate` (left click, item not menu-only).
    Activate,
    /// SNI `SecondaryActivate` (right click, no menu).
    SecondaryActivate,
    /// Open the DBusMenu at this button.
    OpenMenu,
    /// SNI `Scroll(delta, vertical)`; delta = +1 if angleDelta.y > 0 else −1.
    Scroll(i32),
}

#[derive(Clone, Debug)]
pub struct TrayButton {
    pub frame: Rect,
    pub item_id: String,
    pub only_menu: bool,
    pub has_menu: bool,
    /// Decoded icon (status Ready); `None` → first-letter fallback.
    pub icon: Option<skia::Image>,
    pub state: Interaction,
}

impl TrayButton {
    pub fn new(item_id: impl Into<String>) -> Self {
        Self {
            frame: Rect::new(0., 0., 18., 38.),
            item_id: item_id.into(),
            only_menu: false,
            has_menu: false,
            icon: None,
            state: Interaction::default(),
        }
    }
    pub fn fallback_letter(&self) -> String {
        self.item_id
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_else(|| "?".into())
    }
}

impl Control for TrayButton {
    type Action = TrayButtonAction;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (18., 38.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let mut root = kit::group(f);
        match &self.icon {
            Some(img) => {
                root.add(Node::new(
                    Rect::new(0., kit::vcenter(f.height, 18.), 18., 18.),
                    Content::Image(img.clone()),
                ));
            }
            // The fallback Text sets no renderType → QtRendering.
            None => {
                root.add(kit::qt_rendering(kit::centered_text(
                    &self.fallback_letter(),
                    f.width,
                    f.height,
                    &Font {
                        px: 10.,
                        weight: kit::NORMAL,
                    },
                    tok.pal.primary_text,
                )));
            }
        };
        root
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<TrayButtonAction> {
        let mut r = Response::default().with_cursor(Cursor::Pointer);
        if let PointerEvent::Wheel { angle_y, .. } = ev {
            r.action = Some(TrayButtonAction::Scroll(if angle_y > 0. { 1 } else { -1 }));
            return r;
        }
        let feed = self.state.feed(
            ev,
            (self.frame.width, self.frame.height),
            &[Button::Left, Button::Right],
        );
        r.action = match feed.clicked {
            Some(Button::Left) if !self.only_menu => Some(TrayButtonAction::Activate),
            Some(Button::Left) if self.has_menu => Some(TrayButtonAction::OpenMenu),
            Some(Button::Right) if self.has_menu => Some(TrayButtonAction::OpenMenu),
            Some(Button::Right) => Some(TrayButtonAction::SecondaryActivate),
            _ => None,
        };
        r
    }
}
