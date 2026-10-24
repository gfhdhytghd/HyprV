//! V5 InverseCorner — `ScreenCornerShade.qml` (ui-bar.md §9.3).
use super::{paint, picture};
use crate::controls::{Control, PointerEvent, Response, Tok};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node, skia as sk};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug)]
pub struct InverseCorner {
    pub frame: Rect,
    pub corner: Corner,
    /// QML default `Qt.rgba(0, 0, 0, 0.9)`; the shell passes opaque `#000000`.
    pub shade_color: Color,
}

impl InverseCorner {
    /// The shell's corner: 27×27 (`screenCornerShadeSize`), `#000000`.
    pub fn shell(corner: Corner) -> Self {
        Self {
            frame: Rect::new(0., 0., 27., 27.),
            corner,
            shade_color: Color::rgba(0., 0., 0., 1.),
        }
    }
}

impl Control for InverseCorner {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (24., 24.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, _: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let (w, h) = (f.width, f.height);
        let corner = self.corner;
        let color = self.shade_color;
        picture(f.x, f.y, w, h, move |c| {
            c.save();
            match corner {
                Corner::TopLeft => {}
                Corner::TopRight => {
                    c.translate((w, 0.));
                    c.scale((-1., 1.));
                }
                Corner::BottomLeft => {
                    c.translate((0., h));
                    c.scale((1., -1.));
                }
                Corner::BottomRight => {
                    c.translate((w, h));
                    c.scale((-1., -1.));
                }
            }
            let r = w.min(h);
            // moveTo(0,0) lineTo(w,0) arc(w,h,r, 1.5π → π, anticlockwise) lineTo(0,0) close
            let mut p = sk::Path::new();
            p.move_to((0., 0.));
            p.line_to((w, 0.));
            p.arc_to(sk::Rect::from_ltrb(w - r, h - r, w + r, h + r), 270., -90., false);
            p.line_to((0., 0.));
            p.close();
            c.draw_path(&p, &paint(color));
            c.restore();
        })
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        Response::default()
    }
}
