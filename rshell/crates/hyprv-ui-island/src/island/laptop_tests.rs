//! laptop-island.md §4: hidden while the laptop bar is compact (`reveal == 0`).
use super::*;
use crate::{Agent, Media};

fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

fn paused_music() -> Inputs {
    Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing: false, title: "Song".into(), artist: "Band".into(), player: "spotify".into(), length_s: 200., position_s: 50., ..Default::default() },
        ..Default::default()
    }
}

fn settle(i: &mut Island, mut now: u64) -> u64 {
    while i.animating(ms(now)) {
        now += 16;
        i.frame(ms(now));
    }
    now
}

#[test]
fn compact_bar_stops_waves_frames_and_rebuilds() {
    let mut i = Island::new(paused_music(), 1440., ms(0));
    let now = settle(&mut i, 0);
    // Paused music: the 66 ms idle wave is armed while visible.
    assert!(i.next_deadline().is_some());
    i.set_bar_reveal(0., ms(now));
    assert!(i.bar_hidden());
    assert_eq!(i.next_deadline(), None);
    assert!(!i.animating(ms(now)));
    let rev = i.visual_rev();
    assert!(rev.is_some());
    // The model keeps updating, but the host sees no new tree and no deadline.
    let mut next = paused_music();
    next.agents = vec![Agent { id: "s1".into(), source: "claude".into(), status: "running".into(), progress: Some(0.4), ..Default::default() }];
    next.now_hh_mm = "09:42".into();
    i.set_inputs(next.clone(), ms(now + 500));
    assert_eq!(i.visual_rev(), rev);
    assert!(!i.animating(ms(now + 500)));
    assert_eq!(i.next_deadline(), None);
    assert_eq!(i.inputs().agents.len(), 1);
    // No input while hidden.
    let (ox, oy) = i.origin();
    let out = i.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, ms(now + 600));
    assert_eq!(out, PointerOutcome::default());
    assert!(!i.hover_state().2);
    // Reveal: a new tree carrying the current state, as if it had never been hidden.
    i.set_bar_reveal(0.25, ms(now + 5000));
    assert!(!i.bar_hidden());
    assert_ne!(i.visual_rev(), rev);
    let mut fresh = Island::new(next, 1440., ms(0));
    let t = settle(&mut fresh, 0);
    fresh.frame(ms(t.max(now + 5000)));
    let mut revealed_now = now + 5000;
    revealed_now = settle(&mut i, revealed_now);
    i.frame(ms(revealed_now));
    assert_eq!(i.geometry(), fresh.geometry());
    assert_eq!(i.derived(), fresh.derived());
    assert_eq!(i.next_deadline().is_some(), fresh.next_deadline().is_some(), "waves follow the revealed state");
    // Back to music only: the idle wave re-arms once revealed.
    i.set_inputs(paused_music(), ms(revealed_now + 16));
    let t = settle(&mut i, revealed_now + 16);
    i.frame(ms(t));
    assert!(i.next_deadline().is_some(), "idle wave re-armed on reveal");
}

#[test]
fn hiding_cancels_a_press() {
    let mut i = Island::new(paused_music(), 1440., ms(0));
    let now = settle(&mut i, 0);
    let (ox, oy) = i.origin();
    i.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, ms(now));
    i.pointer(Pointer::Press { x: ox + 6., y: oy + 19. }, ms(now + 16));
    i.set_bar_reveal(0., ms(now + 32));
    let out = i.pointer(Pointer::Release { x: ox + 6., y: oy + 19. }, ms(now + 48));
    assert!(out.actions.is_empty());
    i.set_bar_reveal(1., ms(now + 64));
    assert_eq!(i.hover_state(), (false, Target::None, false));
}

#[test]
fn full_reveal_is_a_no_op() {
    // Desktop never calls set_bar_reveal; a laptop bar at reveal 1 must equal that.
    let mut a = Island::new(paused_music(), 1440., ms(0));
    let b = Island::new(paused_music(), 1440., ms(0));
    let rev = a.visual_rev();
    a.set_bar_reveal(1., ms(0));
    a.set_bar_reveal(1.04, ms(0));
    assert_eq!(a.visual_rev(), rev);
    assert_eq!(a.next_deadline(), b.next_deadline());
    assert_eq!(a.geometry(), b.geometry());
}
