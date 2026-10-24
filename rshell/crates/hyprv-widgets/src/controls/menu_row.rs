//! W13 MenuRow — tray menu rows (`shell.qml:1011-1133`, ui-bar-popups.md §3.2).
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Node},
    skia,
};
use std::time::Duration;

/// DBusMenu toggle indicator (`entryIndicator`, `shell.qml:664-675`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuIndicator {
    None,
    Checkbox(bool),
    Radio(bool),
}

impl MenuIndicator {
    fn text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Checkbox(true) => "[x]",
            Self::Checkbox(false) => "[ ]",
            Self::Radio(true) => "(o)",
            Self::Radio(false) => "( )",
        }
    }
}

#[derive(Clone, Debug)]
pub enum MenuRowKind {
    /// "< Back" row shown while a submenu is open.
    Back,
    Separator,
    Entry {
        text: String,
        enabled: bool,
        indicator: MenuIndicator,
        /// Decoded 16×16 icon (sourceSize 16), PreserveAspectFit.
        icon: Option<skia::Image>,
        has_children: bool,
    },
}

#[derive(Clone, Debug)]
pub struct MenuRow {
    pub frame: Rect,
    pub kind: MenuRowKind,
    /// `trayMenuTextPixelSize` (14).
    pub text_pixel_size: f32,
    pub state: Interaction,
}

impl MenuRow {
    /// Width = menu content width (300 − 2·9 = 282).
    pub fn new(kind: MenuRowKind, width: f32) -> Self {
        let mut r = Self {
            frame: Rect::default(),
            kind,
            text_pixel_size: 14.,
            state: Interaction::default(),
        };
        let h = r.row_height();
        r.frame = Rect::new(0., 0., width, h);
        r
    }
    pub fn row_height(&self) -> f32 {
        match self.kind {
            MenuRowKind::Separator => 1.,
            _ => (self.text_pixel_size + 18.).max(34.),
        }
    }
    fn enabled(&self) -> bool {
        match &self.kind {
            MenuRowKind::Separator => false,
            MenuRowKind::Back => true,
            MenuRowKind::Entry { enabled, .. } => *enabled,
        }
    }
}

impl Control for MenuRow {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, self.row_height())
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let p = tok.pal.primary_text;
        let hover = with_alpha(p, if tok.dark() { 0.10 } else { 0.12 });
        let mut root = kit::group(f);
        let font = Font {
            px: self.text_pixel_size,
            weight: kit::NORMAL,
        };
        match &self.kind {
            MenuRowKind::Separator => {
                root.add(kit::qt_rect(
                    Rect::new(0., 0., f.width, 1.),
                    0.,
                    tok.pal.glass_stroke,
                    None,
                ));
            }
            MenuRowKind::Back => {
                let bg = if self.state.hovered {
                    hover
                } else {
                    Color::rgba(0., 0., 0., 0.)
                };
                root.add(kit::qt_rect(Rect::new(0., 0., f.width, f.height), 11., bg, None));
                let m = kit::measure("< Back", font.px, font.weight);
                root.add(kit::text(
                    "< Back",
                    9.,
                    kit::vcenter(f.height, m.height),
                    None,
                    &font,
                    p,
                ));
            }
            MenuRowKind::Entry {
                text,
                enabled,
                indicator,
                icon,
                has_children,
            } => {
                let bg = if self.state.hovered && *enabled {
                    hover
                } else {
                    Color::rgba(0., 0., 0., 0.)
                };
                root.add(kit::qt_rect(Rect::new(0., 0., f.width, f.height), 11., bg, None));
                // Inner Item: anchors.fill with left/right margins 9.
                let (x0, iw) = (9., f.width - 18.);
                let mut left = x0;
                let ind = indicator.text();
                if !ind.is_empty() {
                    let ifont = Font {
                        px: (self.text_pixel_size - 1.).max(11.),
                        weight: kit::NORMAL,
                    };
                    let m = kit::measure(ind, ifont.px, ifont.weight);
                    root.add(kit::text(ind, x0, kit::vcenter(f.height, m.height), None, &ifont, p));
                    left = x0 + m.width;
                }
                if let Some(img) = icon {
                    let ix = if ind.is_empty() { x0 } else { left + 8. };
                    // PreserveAspectFit into 16×16.
                    let (w, h) = (img.width() as f32, img.height() as f32);
                    let s = (16. / w).min(16. / h);
                    let (dw, dh) = (w * s, h * s);
                    let mut n = Node::new(
                        Rect::new(
                            ix + (16. - dw) / 2.,
                            kit::vcenter(f.height, 16.) + (16. - dh) / 2.,
                            dw,
                            dh,
                        ),
                        Content::Image(img.clone()),
                    );
                    n.interactive = false;
                    root.add(n);
                    left = ix + 16.;
                }
                let text_x = if icon.is_some() || !ind.is_empty() {
                    left + 8.
                } else {
                    x0
                };
                let mut right = x0 + iw;
                if *has_children {
                    let m = kit::measure(">", font.px, font.weight);
                    root.add(kit::text(
                        ">",
                        x0 + iw - m.width,
                        kit::vcenter(f.height, m.height),
                        None,
                        &font,
                        p,
                    ));
                    right = x0 + iw - m.width - 8.;
                }
                let color = if *enabled { p } else { with_alpha(p, 0.55) };
                let m = kit::measure(text, font.px, font.weight);
                root.add(kit::text(
                    text,
                    text_x,
                    kit::vcenter(f.height, m.height),
                    Some(right - text_x),
                    &font,
                    color,
                ));
            }
        }
        root
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        if !self.enabled() {
            self.state = Interaction::default();
        }
    }
    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<()> {
        if !self.enabled() {
            self.state = Interaction::default();
            return Response::redraw(false).with_cursor(Cursor::Default);
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        let mut r = Response::redraw(feed.changed).with_cursor(Cursor::Pointer);
        if feed.clicked == Some(Button::Left) {
            r = r.act(());
        }
        r
    }
}
