//! W10 Switch — Bluetooth power toggle (`ControlPanelBluetoothDetails.qml:224-277`,
//! ui-panels-control.md §3.2).
use super::kit::{self, Anim, Tok};
use super::{Button, Control, Cursor, Interaction, PointerEvent, Response};
use crate::foundations::{rgb, with_alpha};
use hyprv_ui::{
    animation::{Color, Easing},
    geometry::Rect,
    scene::Node,
};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Switch {
    pub frame: Rect,
    pub on: bool,
    /// MouseArea enabled (`present && !busy`).
    pub enabled: bool,
    /// Dimmed look (`!present || busy` → opacity 0.58).
    pub dimmed: bool,
    pub state: Interaction,
    fill: Anim<Color>,
    stroke: Anim<Color>,
    knob_x: Anim<f64>,
}

impl Switch {
    pub fn new(on: bool) -> Self {
        let mut s = Self {
            frame: Rect::new(0., 0., 48., 28.),
            on,
            enabled: true,
            dimmed: false,
            state: Interaction::default(),
            fill: Anim::new(140, Easing::Linear),
            stroke: Anim::new(140, Easing::Linear),
            knob_x: Anim::new(160, kit::in_out_quad()),
        };
        s.knob_x.set(if on { 23. } else { 3. }, Duration::ZERO);
        s
    }
    fn colors(&self, tok: &Tok) -> (Color, Color) {
        let d = tok.dark();
        let p = tok.pal.primary_text;
        if self.on {
            (
                with_alpha(rgb(if d { "#7f99bd" } else { "#8faad1" }), if d { 0.94 } else { 0.9 }),
                with_alpha(rgb(if d { "#98b2d8" } else { "#7f9ec8" }), if d { 0.84 } else { 0.62 }),
            )
        } else {
            (
                with_alpha(p, if d { 0.16 } else { 0.14 }),
                with_alpha(p, if d { 0.18 } else { 0.12 }),
            )
        }
    }
}

impl Control for Switch {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (48., 28.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn update(&mut self, tok: &Tok, now: Duration) {
        if !self.enabled {
            self.state = Interaction::default();
        }
        let (f, s) = self.colors(tok);
        self.fill.set(f, now);
        self.stroke.set(s, now);
        self.knob_x.set(
            if self.on {
                (self.frame.width - 22. - 3.) as f64
            } else {
                3.
            },
            now,
        );
    }

    fn build(&self, tok: &Tok, now: Duration) -> Node {
        let f = self.frame;
        let (cf, cs) = self.colors(tok);
        let mut root = kit::group(f);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            f.height / 2.,
            self.fill.get_or(now, cf),
            Some((1., self.stroke.get_or(now, cs))),
        ));
        let d = tok.dark();
        let knob = rgb(if d { "#f7f7fa" } else { "#ffffff" });
        let knob_stroke = with_alpha(rgb("#000000"), if d { 0.18 } else { 0.1 });
        let x = self.knob_x.get_or(now, 3.) as f32;
        root.add(kit::qt_rect(
            Rect::new(x, kit::vcenter(f.height, 22.), 22., 22.),
            11.,
            knob,
            Some((1., knob_stroke)),
        ));
        kit::fade(root, if self.dimmed { 0.58 } else { 1. })
    }

    fn pointer(&mut self, ev: PointerEvent, _: Duration) -> Response<()> {
        if !self.enabled {
            self.state = Interaction::default();
            return Response::default();
        }
        let feed = self
            .state
            .feed(ev, (self.frame.width, self.frame.height), &[Button::Left]);
        let mut r = Response::default().with_cursor(Cursor::Pointer);
        if feed.clicked == Some(Button::Left) {
            // The host flips `on` when the service reports the new power state, then `update`.
            r = r.act(());
        }
        r
    }

    fn animating(&self, now: Duration) -> bool {
        self.fill.running(now) || self.stroke.running(now) || self.knob_x.running(now)
    }
}
