//! Regression tests for review findings ISLAND-1 and ISLAND-3 (docs/reviews/hyprv-ui-island.md).
use super::*;
use crate::{Agent, Media};

const W: f32 = 640.;
fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}
fn settle(i: &mut Island, from: u64) -> u64 {
    let mut t = from;
    for _ in 0..40 {
        t += 16;
        i.frame(ms(t));
    }
    t
}
fn host(i: &Island, x: f32, y: f32) -> (f32, f32) {
    let (ox, oy) = i.origin();
    (ox + x, oy + y)
}
fn session(id: &str) -> Agent {
    Agent { id: id.into(), source: "codex".into(), source_label: "Codex".into(), project: id.into(), status: "running".into(), ..Default::default() }
}
fn agents(ids: &[&str]) -> Inputs {
    Inputs { now_hh_mm: "09:41".into(), agents: ids.iter().map(|id| session(id)).collect(), ..Default::default() }
}
/// Expands the agent list and returns (time, host-space centre of row `k`).
fn expand_agents(i: &mut Island) -> (u64, impl Fn(&Island, usize) -> (f32, f32)) {
    let t = settle(i, 0);
    let p = host(i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    assert!(i.derived().agent_expanded);
    let t = settle(i, t);
    let row = |i: &Island, k: usize| {
        let d = i.derived();
        let (eb, l) = (i.eagent_rect(&d), i.list_rect(&d));
        let y = eb.y + l.y + k as f32 * (ROW_HEIGHT + ROW_SPACING) - i.scroll_y + ROW_HEIGHT / 2.;
        host(i, eb.x + l.width / 2., y)
    };
    (t, row)
}

#[test]
fn island1_reordered_row_focuses_the_pressed_session() {
    let mut i = Island::new(agents(&["a", "b"]), W, ms(0));
    let (t, row) = expand_agents(&mut i);
    let p0 = row(&i, 0);
    i.pointer(Pointer::Motion { x: p0.0, y: p0.1 }, ms(t));
    assert!(i.pointer(Pointer::Press { x: p0.0, y: p0.1 }, ms(t)).actions.is_empty());
    // Status/activity update reorders: "a" moves to row 1.
    i.set_inputs(agents(&["b", "a"]), ms(t + 10));
    // Released where "a" now is → still "a".
    let p1 = row(&i, 1);
    let out = i.pointer(Pointer::Release { x: p1.0, y: p1.1 }, ms(t + 20));
    assert_eq!(out.actions, vec![IslandAction::FocusAgent("a".into())]);

    // Released at the original position (now "b") → outside the pressed delegate: no click.
    let p0 = row(&i, 0);
    i.pointer(Pointer::Press { x: p0.0, y: p0.1 }, ms(t + 30)); // presses "b"
    i.set_inputs(agents(&["a", "b"]), ms(t + 40));
    let out = i.pointer(Pointer::Release { x: p0.0, y: p0.1 }, ms(t + 50));
    assert!(!out.actions.contains(&IslandAction::FocusAgent("a".into())), "{:?}", out.actions);
    assert!(out.actions.is_empty(), "{:?}", out.actions);
}

#[test]
fn island1_removed_row_cancels_the_click() {
    let mut i = Island::new(agents(&["a", "b", "c"]), W, ms(0));
    let (t, row) = expand_agents(&mut i);
    let p = row(&i, 0);
    i.pointer(Pointer::Press { x: p.0, y: p.1 }, ms(t));
    i.set_inputs(agents(&["b", "c"]), ms(t + 10));
    assert_eq!(i.grab, None);
    let out = i.pointer(Pointer::Release { x: p.0, y: p.1 }, ms(t + 20));
    assert!(out.actions.is_empty(), "{:?}", out.actions);
}

fn music() -> Inputs {
    Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing: true, title: "Song".into(), artist: "Band".into(), player: "spotify".into(), length_s: 200., position_s: 50., ..Default::default() },
        spectrum: vec![50.; 16],
        ..Default::default()
    }
}

/// Starts a seek in the expanded music view; returns (time, seek point).
fn start_seek(i: &mut Island) -> (u64, (f32, f32)) {
    let t = settle(i, 0);
    let p = host(i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    let t = settle(i, t);
    let s = host(i, 13. + 183., 13. + 88.);
    i.pointer(Pointer::Press { x: s.0, y: s.1 }, ms(t));
    assert!(i.seeking && i.buttons_down && i.grab == Some(Area::Seek));
    (t, s)
}

#[test]
fn island3_hiding_during_seek_cancels_and_restarts_preview_reset() {
    let mut i = Island::new(music(), W, ms(0));
    let (t, _) = start_seek(&mut i);
    let mut inputs = i.inputs().clone();
    inputs.calendar_open = true;
    inputs.calendar_progress = 1.;
    i.set_inputs(inputs, ms(t + 10));
    i.pointer(Pointer::Leave, ms(t + 11));
    assert!(!i.buttons_down && i.grab.is_none() && i.drag.is_none());
    assert_eq!(i.next_deadline(), Some(ms(t + 10) + SEEK_PREVIEW_RESET), "700 ms preview reset restarted");
    i.frame(ms(t + 10 + 700));
    assert!(!i.seeking, "preview cleared");
    assert_eq!(i.next_deadline(), None);
    // Restored: ordinary hover expands again (no stale button-down suppression).
    let mut inputs = i.inputs().clone();
    inputs.calendar_open = false;
    inputs.calendar_progress = 0.;
    i.set_inputs(inputs, ms(t + 800));
    let t = settle(&mut i, t + 800);
    let p = host(&i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    assert!(i.derived().music_expanded);
}

#[test]
fn island3_pointer_cancel_ends_seek_without_seeking() {
    let mut i = Island::new(music(), W, ms(0));
    let (t, s) = start_seek(&mut i);
    let out = i.pointer(Pointer::Cancel, ms(t + 10));
    assert!(out.actions.is_empty());
    assert!(!i.buttons_down && i.grab.is_none());
    assert_eq!(i.next_deadline(), Some(ms(t + 10) + SEEK_PREVIEW_RESET));
    // A stray release afterwards does not seek.
    let out = i.pointer(Pointer::Release { x: s.0, y: s.1 }, ms(t + 20));
    assert!(out.actions.is_empty(), "{:?}", out.actions);
}

#[test]
fn island3_ordinary_leave_keeps_the_seek_grab() {
    let mut i = Island::new(music(), W, ms(0));
    let (t, s) = start_seek(&mut i);
    // Dragging outside the surface: Leave is not a cancellation; release still seeks.
    i.pointer(Pointer::Leave, ms(t + 10));
    assert!(i.seeking && i.grab == Some(Area::Seek));
    let out = i.pointer(Pointer::Release { x: s.0, y: s.1 }, ms(t + 20));
    assert_eq!(out.actions, vec![IslandAction::Seek(100.)]);
}
