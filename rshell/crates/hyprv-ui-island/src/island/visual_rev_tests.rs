//! `visual_rev` soundness: whenever it reports "unchanged", `build()` must be identical.
use super::*;
use crate::{Agent, Media};
use hyprv_ui::scene::{Content, Node};

fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

fn same_content(a: &Content, b: &Content) -> bool {
    match (a, b) {
        (Content::Group, Content::Group) => true,
        (Content::Rect(a), Content::Rect(b)) => a == b,
        (Content::Text(a), Content::Text(b)) => a == b,
        (Content::Image(a), Content::Image(b)) => a.unique_id() == b.unique_id(),
        (Content::Picture(a), Content::Picture(b)) => a.serialize().as_bytes() == b.serialize().as_bytes(),
        // engine Canvas (ProgressRing): same paints, same pixels
        (Content::Canvas(a), Content::Canvas(b)) => {
            a.operations.len() == b.operations.len()
                && a.operations.iter().zip(&b.operations).all(|(x, y)| {
                    x.path.elements() == y.path.elements()
                        && x.fill == y.fill
                        && match (&x.stroke, &y.stroke) {
                            (Some(s), Some(t)) => s.width == t.width && s.color == t.color && s.cap == t.cap && s.join == t.join,
                            (None, None) => true,
                            _ => false,
                        }
                })
        }
        _ => false,
    }
}

fn same(a: &Node, b: &Node) -> bool {
    a.frame == b.frame
        && a.opacity == b.opacity
        && a.visible == b.visible
        && a.clip == b.clip
        && a.transform == b.transform
        && same_content(&a.content, &b.content)
        && a.children.len() == b.children.len()
        && a.children.iter().zip(&b.children).all(|(x, y)| same(x, y))
}

/// Drives `script` frame by frame (16 ms); checks the contract at every frame and
/// returns how many frames were skippable.
fn check(mut inputs: Inputs, frames: u64, mut script: impl FnMut(&mut Island, &mut Inputs, u64)) -> usize {
    let mut i = Island::new(inputs.clone(), 640., ms(0));
    i.clip_hover_to_input_region = false;
    let (mut rev, mut node) = (i.visual_rev(), i.build());
    let mut skipped = 0;
    for k in 1..=frames {
        let now = k * 16;
        script(&mut i, &mut inputs, now);
        i.frame(ms(now));
        let (r, n) = (i.visual_rev(), i.build());
        if r.is_some() && r == rev {
            assert!(same(&n, &node), "frame {k}: visual_rev unchanged but build() differs");
            skipped += 1;
        }
        (rev, node) = (r, n);
    }
    skipped
}

fn music(playing: bool) -> Inputs {
    Inputs {
        now_hh_mm: "09:41".into(),
        media: Media { available: true, playing, title: "Song".into(), artist: "Band".into(), player: "spotify".into(), length_s: 200., position_s: 50., ..Default::default() },
        spectrum: if playing { vec![50.; 16] } else { Vec::new() },
        ..Default::default()
    }
}

#[test]
fn idle_frames_are_skippable() {
    let skipped = check(Inputs { now_hh_mm: "09:41".into(), ..Default::default() }, 120, |_, _, _| {});
    assert!(skipped >= 100, "skipped {skipped}");
}

#[test]
fn music_hover_seek_wave_and_inputs() {
    let skipped = check(music(false), 400, |i, inputs, now| {
        let (ox, oy) = i.origin();
        match now / 16 {
            // Paused: the 66 ms idle wave advances the phase.
            40 => { i.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, ms(now)); }
            80 => { i.pointer(Pointer::Press { x: ox + 196., y: oy + 101. }, ms(now)); }
            84 => { i.pointer(Pointer::Motion { x: ox + 250., y: oy + 101. }, ms(now)); }
            90 => { i.pointer(Pointer::Release { x: ox + 250., y: oy + 101. }, ms(now)); }
            150 => {
                inputs.media.playing = true;
                inputs.spectrum = vec![40.; 16];
                i.set_inputs(inputs.clone(), ms(now));
            }
            151..=200 if now % 48 == 0 => {
                inputs.spectrum[(now / 48 % 16) as usize] += 1.;
                i.set_inputs(inputs.clone(), ms(now));
            }
            // Identical snapshot: no change.
            201..=260 => i.set_inputs(inputs.clone(), ms(now)),
            270 => { i.pointer(Pointer::Motion { x: 1., y: 1. }, ms(now)); }
            300 => {
                inputs.calendar_open = true;
                inputs.calendar_progress = 1.;
                i.set_inputs(inputs.clone(), ms(now));
            }
            _ => {}
        }
    });
    assert!(skipped > 100, "skipped {skipped}");
}

#[test]
fn agent_list_wheel_and_reorder() {
    let agent = |k: usize| Agent { id: format!("s{k}"), source: "codex".into(), project: format!("p{k}"), status: "running".into(), progress: Some(0.1 * k as f64), ..Default::default() };
    let inputs = Inputs { now_hh_mm: "09:41".into(), agents: (0..12).map(agent).collect(), ..Default::default() };
    check(inputs, 300, |i, inputs, now| {
        let (ox, oy) = i.origin();
        match now / 16 {
            20 => { i.pointer(Pointer::Motion { x: ox + 6., y: oy + 19. }, ms(now)); }
            60..=62 => { i.pointer(Pointer::Wheel { x: ox + 100., y: oy + 200., angle_y: -120. }, ms(now)); }
            100 => {
                inputs.agents.swap(0, 3);
                i.set_inputs(inputs.clone(), ms(now));
            }
            150 => { i.pointer(Pointer::Leave, ms(now)); }
            _ => {}
        }
    });
}

#[test]
fn set_calendar_matches_set_inputs() {
    let mut a = Island::new(music(true), 640., ms(0));
    let mut b = Island::new(music(true), 640., ms(0));
    for k in 1..60u64 {
        let now = ms(k * 16);
        let (open, p) = (k >= 5, ((k as f32 - 5.) / 13.).clamp(0., 1.));
        a.set_calendar(open, p, now);
        let mut i = b.inputs().clone();
        i.calendar_open = open;
        i.calendar_progress = p;
        b.set_inputs(i, now);
        a.frame(now);
        b.frame(now);
        assert!(same(&a.build(), &b.build()), "frame {k}");
        assert_eq!(a.visual_rev().is_some(), b.visual_rev().is_some());
    }
}
