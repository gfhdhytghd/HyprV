//! Visualisations (components.md §4, V1–V6). Owner: spec-panels.
//!
//! Same contract as [`crate::controls::Control`]: public fields are the QML inputs,
//! `build(tok, now)` renders a fresh subtree, `pointer` handles input. QML `Canvas`
//! painting is reproduced with recorded Skia pictures (`Content::Picture`) using the same
//! path operations (arcs, quadratic curves, gradients, round caps/joins) as the 2D context.
//! Visualisations own **no timers**: periodic behaviour (the spectrum idle wave) is
//! exposed as `wants_tick()` + `tick()` for the host to schedule.
mod inverse_corner;
mod progress_ring;
mod progress_track;
mod spectrum;
mod timeline;
mod trend_chart;

#[cfg(test)]
mod tests;

pub use inverse_corner::{Corner, InverseCorner};
pub use progress_ring::ProgressRing;
pub use progress_track::{ProgressTrack, TrackAction, SEEK_PREVIEW_HOLD};
pub use spectrum::{SpectrumBars, SPECTRUM_TICK};
pub use timeline::{
    layout_all_day, layout_timed, AllDayInput, AllDayItem, DayLabel, TimedInput, TimedItem, TimelineGrid,
};
pub use trend_chart::TrendChart;

use hyprv_ui::{
    geometry::Rect,
    scene::{Content, Node},
    skia as sk,
};

/// Record Qt-Canvas-like drawing into a picture node of logical size `w×h` at (x, y).
pub(crate) fn picture(x: f32, y: f32, w: f32, h: f32, draw: impl FnOnce(&sk::Canvas)) -> Node {
    let mut rec = sk::PictureRecorder::new();
    let canvas = rec.begin_recording(sk::Rect::from_wh(w.max(1.), h.max(1.)), false);
    draw(canvas);
    let pic = rec.finish_recording_as_picture(None).expect("picture");
    Node::new(Rect::new(x, y, w, h), Content::Picture(pic))
}

pub(crate) fn paint(c: hyprv_ui::animation::Color) -> sk::Paint {
    let mut p = sk::Paint::new(sk::Color4f::new(c.r, c.g, c.b, c.a), None);
    p.set_anti_alias(true);
    p
}

pub(crate) fn stroke_paint(c: hyprv_ui::animation::Color, width: f32, round: bool) -> sk::Paint {
    let mut p = paint(c);
    p.set_style(sk::paint::Style::Stroke).set_stroke_width(width);
    if round {
        p.set_stroke_cap(sk::paint::Cap::Round)
            .set_stroke_join(sk::paint::Join::Round);
    }
    p
}
