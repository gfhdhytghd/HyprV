//! V1 ProgressTrack — island seek bar (`DynamicIsland.qml:1022-1094`, ui-island.md §3.11).
use crate::controls::{
    kit::{self, Anim},
    Button, Control, Cursor, Interaction, PointerEvent, Response, Tok,
};
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackAction {
    /// `seekRequested(seconds)` on release.
    SeekRequested(f64),
}

/// `seekPreviewReset` interval.
pub const SEEK_PREVIEW_HOLD: Duration = Duration::from_millis(700);

#[derive(Clone, Debug)]
pub struct ProgressTrack {
    /// The 5-px track rect (the hit area extends 8 px beyond it on every side).
    pub frame: Rect,
    pub position_s: f64,
    pub length_s: f64,
    pub seeking: bool,
    pub seek_preview_s: f64,
    /// When `seeking` turns false again (host timer); `None` while pressed or idle.
    pub preview_reset_at: Option<Duration>,
    pub state: Interaction,
    fill: Anim<f64>,
}

impl ProgressTrack {
    pub fn new(width: f32) -> Self {
        let mut t = Self {
            frame: Rect::new(0., 0., width, 5.),
            position_s: 0.,
            length_s: 0.,
            seeking: false,
            seek_preview_s: 0.,
            preview_reset_at: None,
            state: Interaction::default(),
            fill: Anim::new(180, kit::out_cubic()),
        };
        t.update(&Tok::new(false), Duration::ZERO);
        t
    }
    /// `displayedPositionSeconds` — what the time labels show.
    pub fn displayed_position(&self) -> f64 {
        if self.seeking {
            self.seek_preview_s
        } else {
            self.position_s
        }
    }
    fn ratio(&self) -> f64 {
        if self.length_s > 0. {
            (self.displayed_position() / self.length_s).clamp(0., 1.)
        } else {
            0.
        }
    }
    fn target_width(&self) -> f64 {
        (self.frame.height as f64).max(self.frame.width as f64 * self.ratio())
    }
    fn seconds_for_x(&self, x: f32) -> f64 {
        if self.length_s <= 0. || self.frame.width <= 0. {
            return 0.;
        }
        ((x / self.frame.width).clamp(0., 1.) as f64 * self.length_s).clamp(0., self.length_s)
    }
    /// The seek MouseArea became disabled (length ≤ 0) while pressed: QML `onCanceled`
    /// restarts `seekPreviewReset`; no seek is requested.
    fn cancel_seek(&mut self, now: Duration) {
        if self.state.pressed.is_some() {
            self.state = Interaction::default();
            self.preview_reset_at = Some(now + SEEK_PREVIEW_HOLD);
        }
        self.state.hovered = false;
    }

    /// Host timer: call at/after `preview_reset_at`.
    pub fn expire(&mut self, now: Duration) -> bool {
        match self.preview_reset_at {
            Some(t) if now >= t => {
                self.preview_reset_at = None;
                self.seeking = false;
                self.update(&Tok::new(false), now);
                true
            }
            _ => false,
        }
    }
}

impl Control for ProgressTrack {
    type Action = TrackAction;

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, 5.)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn update(&mut self, _: &Tok, now: Duration) {
        if self.length_s <= 0. {
            self.cancel_seek(now);
        }
        // Behavior on width { enabled: !seeking }
        self.fill.enabled = !self.seeking;
        let t = self.target_width();
        self.fill.set(t, now);
    }

    fn build(&self, tok: &Tok, now: Duration) -> Node {
        let f = self.frame;
        let r = f.height / 2.;
        let mut root = kit::group(f);
        root.add(kit::qt_rect(
            Rect::new(0., 0., f.width, f.height),
            r,
            tok.island_soft_fill,
            None,
        ));
        if self.length_s > 0. {
            let w = self.fill.get_or(now, self.target_width()) as f32;
            // Parent `clip: true` (rectangular): the fill never exceeds the track.
            let mut clip = crate::primitives::clip_box(Rect::new(0., 0., f.width, f.height), vec![]);
            clip.add(kit::qt_rect(Rect::new(0., 0., w, f.height), r, tok.pal.accent, None));
            root.add(clip);
        }
        root
    }

    fn pointer(&mut self, ev: PointerEvent, now: Duration) -> Response<TrackAction> {
        if self.length_s <= 0. {
            self.cancel_seek(now);
            return Response::default();
        }
        // MouseArea = track grown by 8 px on every side; coordinates mapped back to the track.
        let shift = |ev: PointerEvent| match ev {
            PointerEvent::Enter { x, y } => PointerEvent::Enter { x: x + 8., y: y + 8. },
            PointerEvent::Move { x, y } => PointerEvent::Move { x: x + 8., y: y + 8. },
            PointerEvent::Press { x, y, button } => PointerEvent::Press {
                x: x + 8.,
                y: y + 8.,
                button,
            },
            PointerEvent::Release { x, y, button } => PointerEvent::Release {
                x: x + 8.,
                y: y + 8.,
                button,
            },
            other => other,
        };
        let size = (self.frame.width + 16., self.frame.height + 16.);
        let feed = self.state.feed(shift(ev), size, &[Button::Left]);
        let mut r = Response::default().with_cursor(Cursor::Pointer);
        if let Some((x, _, _)) = feed.pressed_at {
            self.preview_reset_at = None;
            self.seeking = true;
            self.seek_preview_s = self.seconds_for_x(x - 8.);
            r.redraw = true;
        }
        if let Some((x, _)) = feed.moved_to {
            self.seek_preview_s = self.seconds_for_x(x - 8.);
            r.redraw = true;
        }
        if let Some((x, _, _)) = feed.released_at {
            let target = self.seconds_for_x(x - 8.);
            self.seek_preview_s = target;
            self.preview_reset_at = Some(now + SEEK_PREVIEW_HOLD);
            r = r.act(TrackAction::SeekRequested(target));
        }
        if feed.canceled {
            self.preview_reset_at = Some(now + SEEK_PREVIEW_HOLD);
        }
        self.update(&Tok::new(false), now);
        r
    }

    fn animating(&self, now: Duration) -> bool {
        self.fill.running(now)
    }
}
