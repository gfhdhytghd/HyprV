//! Interaction/state rules of ui-island.md §2, §4, §7 without rendering.
use hyprv_ui_island::{island::*, Agent, Inputs, Island, IslandAction, Media, Pointer};
use std::time::Duration;

const W: f32 = 640.;
fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}
fn music(playing: bool) -> Inputs {
    Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing, title: "Song".into(), artist: "Band".into(), player: "spotify".into(), length_s: 200., position_s: 50., ..Default::default() },
        spectrum: if playing { vec![50.; 16] } else { Vec::new() },
        ..Default::default()
    }
}
fn agent(status: &str) -> Agent {
    Agent { id: "s0".into(), source: "codex".into(), source_label: "Codex".into(), project: "rshell".into(), status: status.into(), progress: Some(0.5), ..Default::default() }
}
fn settle(i: &mut Island, from: u64) -> u64 {
    let mut t = from;
    for _ in 0..40 {
        t += 16;
        i.frame(ms(t));
    }
    t
}
/// Island-local point → host coordinates.
fn at(i: &Island, x: f32, y: f32) -> (f32, f32) {
    let (ox, oy) = i.origin();
    (ox + x, oy + y)
}

#[test]
fn idle_geometry_and_zero_idle_work() {
    let mut i = Island::new(Inputs { now_hh_mm: "09:41".into(), ..Default::default() }, W, ms(0));
    settle(&mut i, 0);
    let g = i.geometry();
    assert!((g.width - 120.16).abs() < 0.05, "idle width {}", g.width);
    assert_eq!((g.height, g.attached_center_offset, g.expanded), (38., 0., false));
    assert!(!i.animating(ms(1000)) && i.next_deadline().is_none(), "idle island must not schedule work");
}

#[test]
fn side_hotspot_expands_music_clock_collapses() {
    let mut i = Island::new(music(true), W, ms(0));
    let mut t = settle(&mut i, 0);
    assert_eq!(i.geometry().width, COMPACT_MUSIC_WIDTH);
    // Playing with spectrum energy: no idle wave timer.
    assert!(i.next_deadline().is_none());
    let p = at(&i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    assert!(i.derived().music_expanded);
    t = settle(&mut i, t);
    assert_eq!((i.geometry().width, i.geometry().height), (EXPANDED_WIDTH, EXPANDED_HEIGHT));
    // Leaving the hover rect collapses.
    i.pointer(Pointer::Motion { x: 2., y: 2. }, ms(t));
    assert!(!i.derived().expanded);
    t = settle(&mut i, t);
    // Entering the clock never expands.
    let c = at(&i, 60., 19.);
    i.pointer(Pointer::Motion { x: c.0, y: c.1 }, ms(t));
    assert!(!i.derived().expanded);
    let out = i.pointer(Pointer::Press { x: c.0, y: c.1 }, ms(t));
    assert!(out.actions.is_empty());
    let out = i.pointer(Pointer::Release { x: c.0, y: c.1 }, ms(t + 50));
    assert_eq!(out.actions, vec![IslandAction::ToggleCalendar]);
}

#[test]
fn paused_compact_music_runs_the_idle_wave() {
    let mut i = Island::new(music(false), W, ms(0));
    let t = settle(&mut i, 0);
    let next = i.next_deadline().expect("paused spectrum wave ticks");
    assert!(next > ms(t) && next <= ms(t) + WAVE_TICK);
}

#[test]
fn agent_offsets_and_dual_keep_timer() {
    let mut inputs = music(true);
    inputs.agents = vec![agent("running")];
    let mut i = Island::new(inputs.clone(), W, ms(0));
    let mut t = settle(&mut i, 0);
    let g = i.geometry();
    assert_eq!((g.width, g.attached_center_offset), (DUAL_COLLAPSED_WIDTH, 7.));
    // Right hotspot in dual → music.
    let p = at(&i, 118., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    assert!(i.derived().music_expanded && i.derived().pfc);
    t = settle(&mut i, t);
    i.pointer(Pointer::Motion { x: 2., y: 2. }, ms(t));
    assert!(i.derived().pfc, "frame stays collapsed during the release delay");
    i.frame(ms(t + 229));
    assert!(i.derived().pfc);
    i.frame(ms(t + 231));
    assert!(!i.derived().pfc);
    // Single agent (no music): offset −2.
    inputs.media = Media::default();
    inputs.spectrum.clear();
    let mut i = Island::new(inputs, W, ms(0));
    settle(&mut i, 0);
    assert_eq!(i.geometry().attached_center_offset, -2.);
}

#[test]
fn agent_expanded_height_table() {
    for (n, h) in [(1, 184.), (2, 196.), (3, 246.), (7, 446.), (9, 490.)] {
        let inputs = Inputs { now_hh_mm: "09:41".into(), agents: (0..n).map(|k| Agent { id: format!("s{k}"), ..agent("running") }).collect(), ..Default::default() };
        let i = Island::new(inputs, W, ms(0));
        assert_eq!(i.derived().agent_expanded_height, h, "n={n}");
    }
}

#[test]
fn seek_scrub_emits_once_and_resets_after_700ms() {
    let mut i = Island::new(music(true), W, ms(0));
    let mut t = settle(&mut i, 0);
    let p = at(&i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    t = settle(&mut i, t);
    // Track starts at (13, 13 + 86); its width is 392 − 26 = 366.
    let s = at(&i, 13. + 183., 13. + 88.);
    assert!(i.pointer(Pointer::Press { x: s.0, y: s.1 }, ms(t)).actions.is_empty());
    let out = i.pointer(Pointer::Release { x: s.0, y: s.1 }, ms(t + 30));
    assert_eq!(out.actions, vec![IslandAction::Seek(100.)]);
    i.frame(ms(t + 30 + 699));
    assert_eq!(i.next_deadline().map(|d| d <= ms(t + 730)), Some(true));
    i.frame(ms(t + 30 + 700));
}

#[test]
fn calendar_clears_hover() {
    let mut i = Island::new(music(true), W, ms(0));
    let t = settle(&mut i, 0);
    let p = at(&i, 6., 19.);
    i.pointer(Pointer::Motion { x: p.0, y: p.1 }, ms(t));
    assert!(i.derived().expanded);
    let mut inputs = i.inputs().clone();
    inputs.calendar_open = true;
    i.set_inputs(inputs, ms(t + 10));
    assert!(!i.derived().expanded);
    assert!(!i.hover_state().0);
}
