//! W11 SegmentedRow — Mihomo mode buttons (`MihomoControlPanel.qml:190-204`,
//! ui-panels-control.md §7 item 3). Labels use the application default font (no
//! `font.family` in QML), 10 px, bold only when selected.
use super::kit::{self, Font, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::with_alpha;
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SegmentedRow {
    pub frame: Rect,
    /// Option ids; the label is the id with its first letter capitalised.
    pub options: Vec<String>,
    pub selected: Option<String>,
    pub spacing: f32,
    /// MouseArea enabled (`online && !busy`).
    pub enabled: bool,
    pub states: Vec<Interaction>,
}

impl SegmentedRow {
    pub fn mihomo_modes(width: f32) -> Self {
        Self {
            frame: Rect::new(0., 0., width, 34.),
            options: ["rule", "global", "direct"].map(String::from).to_vec(),
            selected: None,
            spacing: 7.,
            enabled: true,
            states: vec![Interaction::default(); 3],
        }
    }
    fn cell_w(&self) -> f32 {
        let n = self.options.len().max(1) as f32;
        (self.frame.width - self.spacing * (n - 1.)) / n
    }
    fn label(id: &str) -> String {
        let mut c = id.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default()
    }
}

impl Control for SegmentedRow {
    type Action = String;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, 34.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let p = &tok.pal;
        let w = self.cell_w();
        let mut root = kit::group(self.frame);
        for (i, id) in self.options.iter().enumerate() {
            let sel = self.selected.as_deref() == Some(id.as_str());
            let x = i as f32 * (w + self.spacing);
            let (fill, border, fg) = if sel {
                (with_alpha(p.accent, 0.25), p.accent, p.accent)
            } else {
                (p.card_fill, p.card_stroke, p.muted_text)
            };
            let mut cell = kit::group(Rect::new(x, 0., w, 34.));
            cell.add(kit::qt_rect(Rect::new(0., 0., w, 34.), 11., fill, Some((1., border))));
            let label = Self::label(id);
            let font = Font {
                px: 10.,
                weight: if sel { kit::BOLD } else { kit::NORMAL },
            };
            let m = kit::measure_in(kit::DEFAULT_FAMILY, &label, 10., font.weight, 0.);
            let (tx, ty) = kit::center_in(w, 34., m.width, m.height);
            cell.add(kit::qt_rendering(kit::text_in(
                kit::DEFAULT_FAMILY,
                &label,
                tx,
                ty,
                None,
                &font,
                fg,
                0.,
                false,
            )));
            root.add(cell);
        }
        root
    }

    fn update(&mut self, _: &Tok, _: Duration) {
        if !self.enabled {
            self.states.fill(Interaction::default());
        }
    }
    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<String> {
        if !self.enabled {
            self.states.fill(Interaction::default());
            return Response::default();
        }
        let w = self.cell_w();
        self.states.resize(self.options.len(), Interaction::default());
        let mut r = Response::default();
        for i in 0..self.options.len() {
            let dx = i as f32 * (w + self.spacing);
            let local = match ev {
                PointerEvent::Enter { x, y } => PointerEvent::Enter { x: x - dx, y },
                PointerEvent::Move { x, y } => PointerEvent::Move { x: x - dx, y },
                PointerEvent::Press { x, y, button } => PointerEvent::Press { x: x - dx, y, button },
                PointerEvent::Release { x, y, button } => PointerEvent::Release { x: x - dx, y, button },
                other => other,
            };
            let f = self.states[i].feed(local, (w, 34.), &[Button::Left]);
            if f.clicked.is_some() {
                r = r.act(self.options[i].clone());
            }
            if self.states[i].hovered {
                r.cursor = Some(Cursor::Pointer);
            }
        }
        r
    }
}
