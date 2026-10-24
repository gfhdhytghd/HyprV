//! W1 TextModule — bar text button (`shell.qml:523-609`, ui-bar-layout.md §3).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextModuleAction {
    LeftClicked,
    RightClicked,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Debug)]
pub struct TextModule {
    pub frame: Rect,
    pub label: String,
    /// `None` = `root.primaryText`.
    pub text_color: Option<Color>,
    pub font_pixel_size: f32,
    pub font_weight: i32,
    pub padding_left: f32,
    pub padding_right: f32,
    pub minimum_width: f32,
    pub module_height: f32,
    pub interactive: bool,
    pub wheel_interactive: bool,
    pub hoverable: bool,
    pub highlighted: bool,
    pub collapse_when_hidden: bool,
    pub visible: bool,
    /// `None` = `activeWorkspaceBackground`.
    pub highlight_color: Option<Color>,
    /// `None` = `activeWorkspaceText`.
    pub highlighted_text_color: Option<Color>,
    pub highlight_inset: f32,
    pub state: Interaction,
}

impl Default for TextModule {
    fn default() -> Self {
        Self {
            frame: Rect::default(),
            label: String::new(),
            text_color: None,
            font_pixel_size: 16.,
            font_weight: kit::BOLD,
            padding_left: 8.,
            padding_right: 8.,
            minimum_width: 0.,
            module_height: 38.,
            interactive: false,
            wheel_interactive: false,
            hoverable: false,
            highlighted: false,
            collapse_when_hidden: false,
            visible: true,
            highlight_color: None,
            highlighted_text_color: None,
            highlight_inset: 0.,
            state: Interaction::default(),
        }
    }
}

impl TextModule {
    pub fn label_implicit_width(&self) -> f32 {
        kit::measure(&self.label, self.font_pixel_size, self.font_weight).width
    }
    /// `naturalImplicitWidth` (computed even when hidden, ui-bar-layout.md §2.3).
    pub fn natural_implicit_width(&self) -> f32 {
        (self.label_implicit_width() + self.padding_left.max(0.) + self.padding_right.max(0.)).max(self.minimum_width)
    }
    fn mouse_enabled(&self) -> bool {
        self.interactive || self.wheel_interactive || self.hoverable
    }
}

impl Control for TextModule {
    type Action = TextModuleAction;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        let w = if self.collapse_when_hidden && !self.visible {
            0.
        } else {
            self.natural_implicit_width()
        };
        (w, self.module_height)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let mut root = kit::group(f);
        root.visible = self.visible;
        let pal = &tok.pal;
        let chip_visible = self.highlighted || (self.hoverable && self.state.hovered);
        if chip_visible {
            let i = self.highlight_inset;
            let color = if self.highlighted {
                self.highlight_color.unwrap_or(pal.active_workspace)
            } else {
                pal.workspace_hover
            };
            root.add(kit::qt_rect(
                Rect::new(i, i, f.width - 2. * i, f.height - 2. * i),
                19.,
                color,
                None,
            ));
        }
        let color = if self.highlighted {
            self.highlighted_text_color.unwrap_or(pal.active_workspace_text)
        } else {
            self.text_color.unwrap_or(pal.primary_text)
        };
        let (pl, pr) = (self.padding_left.max(0.), self.padding_right.max(0.));
        let font = Font {
            px: self.font_pixel_size,
            weight: self.font_weight,
        };
        root.add(kit::hcentered_text(
            &self.label,
            pl,
            f.width - pl - pr,
            f.height,
            &font,
            color,
        ));
        root
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        if !self.mouse_enabled() || !self.visible {
            self.state = Interaction::default();
        }
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<TextModuleAction> {
        if !self.mouse_enabled() || !self.visible {
            // A disabled/hidden MouseArea loses its grab (QML onCanceled): forget the press so
            // a later release cannot complete a stale click (review WIDGETS-1 pattern).
            self.state = Interaction::default();
            return Response::default();
        }
        let cursor = if self.interactive || self.wheel_interactive {
            Cursor::Pointer
        } else {
            Cursor::Default
        };
        if let PointerEvent::Wheel { angle_y, .. } = ev {
            if !self.wheel_interactive {
                return Response::default();
            }
            let mut r = Response::default().with_cursor(cursor);
            if angle_y > 0. {
                r = r.act(TextModuleAction::WheelUp);
            } else if angle_y < 0. {
                r = r.act(TextModuleAction::WheelDown);
            }
            r.redraw = false;
            return r;
        }
        let size = (self.frame.width, self.frame.height);
        let feed = self.state.feed(ev, size, &[Button::Left, Button::Right]);
        // Only the hover chip depends on interaction state.
        let mut r = Response::redraw(feed.changed && self.hoverable).with_cursor(cursor);
        match feed.clicked {
            Some(Button::Left) => r.action = Some(TextModuleAction::LeftClicked),
            Some(Button::Right) => r.action = Some(TextModuleAction::RightClicked),
            _ => {}
        }
        r
    }
}
