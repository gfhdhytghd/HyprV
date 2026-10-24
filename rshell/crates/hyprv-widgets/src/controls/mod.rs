//! Controls (components.md §3, W1–W14). Owner: spec-panels.
//!
//! Every control is a plain state struct with the same shape:
//! - public fields = QML properties (inputs); `frame` = logical rect in the parent;
//! - [`Control::build`] renders a fresh subtree for the current state at time `now`
//!   (pure: same state + `now` → same pixels);
//! - [`Control::pointer`] feeds widget-local pointer events (QML MouseArea semantics)
//!   and returns an optional action plus whether a redraw/animation is needed;
//! - [`Control::animating`] is true while a `Behavior` is running; the host keeps
//!   requesting frames only while it is true (no timers inside widgets).
//!
//! Mount a control under a container node with [`Mounted`] and re-sync it after
//! input or animation ticks; the container `NodeId` stays stable.
pub(crate) mod kit;

mod action_chip;
mod badge;
mod circle_glyph_button;
mod dialog_button;
mod glyph_button;
mod menu_row;
mod pill_button;
mod segmented_row;
mod slider_tile;
mod split_tile;
mod switch;
mod text_module;
mod toggle_tile;
mod tray_button;

#[cfg(test)]
mod shell_ref_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod testutil;

pub use action_chip::{ActionChip, ChipContent};
pub use badge::Badge;
pub use circle_glyph_button::CircleGlyphButton;
pub use dialog_button::{DialogButton, DialogKey, DIALOG_FONT};
pub use glyph_button::{GlyphAnchor, GlyphBadge, GlyphButton};
pub use kit::{Metrics, Tok};
pub use menu_row::{MenuIndicator, MenuRow, MenuRowKind};
pub use pill_button::{PillButton, PillStyle};
pub use segmented_row::SegmentedRow;
pub use slider_tile::{SliderAction, SliderTile};
pub use split_tile::{SplitTile, SplitZone};
pub use switch::Switch;
pub use text_module::{TextModule, TextModuleAction};
pub use toggle_tile::{ToggleIcon, ToggleTile};
pub use tray_button::{TrayButton, TrayButtonAction};

use hyprv_ui::{
    geometry::Rect,
    scene::{Content, Node, NodeId, Scene},
};
use std::time::Duration;

/// Measure text exactly like Qt 6.12 on this machine (components.md §0.2).
pub fn measure_text(text: &str, pixel_size: f32, weight: i32) -> Metrics {
    kit::measure(text, pixel_size, weight)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Widget-local pointer events (origin = the control's top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Enter {
        x: f32,
        y: f32,
    },
    Move {
        x: f32,
        y: f32,
    },
    Leave,
    Press {
        x: f32,
        y: f32,
        button: Button,
    },
    Release {
        x: f32,
        y: f32,
        button: Button,
    },
    /// Grab lost (QML `onCanceled`).
    Cancel,
    /// `angle_*` in Qt angleDelta units (120 per notch), `pixel_*` from smooth scrolling.
    Wheel {
        angle_x: f32,
        angle_y: f32,
        pixel_x: f32,
        pixel_y: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Pointer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response<A> {
    pub action: Option<A>,
    /// The control's pixels changed (or an animation started): re-sync and redraw.
    pub redraw: bool,
    pub cursor: Option<Cursor>,
}

impl<A> Default for Response<A> {
    fn default() -> Self {
        Self {
            action: None,
            redraw: false,
            cursor: None,
        }
    }
}

impl<A> Response<A> {
    pub(crate) fn redraw(redraw: bool) -> Self {
        Self {
            redraw,
            ..Default::default()
        }
    }
    pub(crate) fn with_cursor(mut self, c: Cursor) -> Self {
        self.cursor = Some(c);
        self
    }
    pub(crate) fn act(mut self, a: A) -> Self {
        self.action = Some(a);
        self.redraw = true;
        self
    }
}

pub trait Control {
    type Action;
    /// QML implicit size.
    fn implicit_size(&self, tok: &Tok) -> (f32, f32);
    fn frame(&self) -> Rect;
    fn build(&self, tok: &Tok, now: Duration) -> Node;
    fn pointer(&mut self, ev: PointerEvent, now: Duration) -> Response<Self::Action>;
    /// Re-evaluate bindings after the host changed public fields or the theme at `now`
    /// (retargets `Behavior`s exactly like QML would). Cheap; call it freely.
    fn update(&mut self, _tok: &Tok, _now: Duration) {}
    fn animating(&self, _now: Duration) -> bool {
        false
    }
}

/// QML `MouseArea` press/hover/click bookkeeping.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: Option<Button>,
}

/// Outcome of feeding one event to an [`Interaction`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Feed {
    pub changed: bool,
    pub clicked: Option<Button>,
    pub pressed_at: Option<(f32, f32, Button)>,
    pub moved_to: Option<(f32, f32)>,
    pub released_at: Option<(f32, f32, Button)>,
    pub canceled: bool,
}

impl Interaction {
    /// `size` = MouseArea size (hit rect is `[0,w)×[0,h)`). `buttons` = acceptedButtons.
    pub(crate) fn feed(&mut self, ev: PointerEvent, size: (f32, f32), buttons: &[Button]) -> Feed {
        let inside = |x: f32, y: f32| x >= 0. && y >= 0. && x < size.0 && y < size.1;
        let before = *self;
        let mut out = Feed::default();
        match ev {
            PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => {
                self.hovered = inside(x, y);
                if self.pressed.is_some() {
                    out.moved_to = Some((x, y));
                }
            }
            PointerEvent::Leave => self.hovered = false,
            PointerEvent::Press { x, y, button } => {
                if inside(x, y) && buttons.contains(&button) && self.pressed.is_none() {
                    self.pressed = Some(button);
                    self.hovered = true;
                    out.pressed_at = Some((x, y, button));
                }
            }
            PointerEvent::Release { x, y, button } => {
                if self.pressed == Some(button) {
                    self.pressed = None;
                    out.released_at = Some((x, y, button));
                    if inside(x, y) {
                        out.clicked = Some(button);
                    }
                    self.hovered = inside(x, y);
                }
            }
            PointerEvent::Cancel => {
                if self.pressed.is_some() {
                    out.canceled = true;
                }
                self.pressed = None;
            }
            PointerEvent::Wheel { .. } => {}
        }
        out.changed = before != *self;
        out
    }
}

/// A control mounted under a stable container node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mounted {
    pub container: NodeId,
}

impl Mounted {
    /// Add an empty container (positioned at the control's frame origin later by `sync`).
    pub fn attach(parent: &mut Node) -> Self {
        let n = Node::new(Rect::default(), Content::Group);
        Self {
            container: parent.add(n),
        }
    }
    /// Replace the container's subtree with the control's current rendering.
    pub fn sync<C: Control>(&self, scene: &mut Scene, control: &C, tok: &Tok, now: Duration) {
        let built = control.build(tok, now);
        scene.edit(self.container, |n| {
            n.frame = Rect::new(0., 0., 0., 0.);
            n.children = vec![built];
        });
    }
}
