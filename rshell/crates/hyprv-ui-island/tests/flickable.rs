use hyprv_ui_island::{Agent, Inputs, Island, Pointer};
use hyprv_widgets::flickable::{ScrollPhase, Wheel};
use std::time::Duration;
fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
fn settle(i: &mut Island, start: u64, end: u64) {
    for t in (start..=end).step_by(16) {
        i.frame(ms(t));
    }
}
fn expanded() -> Island {
    let inputs = Inputs {
        now_hh_mm: "09:41".into(),
        agents: (0..15)
            .map(|id| Agent {
                id: format!("s{id}"),
                status: "running".into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut i = Island::new(inputs, 1000., ms(0));
    let (x, y) = i.origin();
    i.pointer(
        Pointer::Motion {
            x: x + 6.,
            y: y + 19.,
        },
        ms(0),
    );
    settle(&mut i, 0, 1504);
    assert!(i.derived().agent_expanded);
    i
}
#[test]
fn drag_flick_never_focuses_a_row_and_hiding_cancels_motion() {
    let mut i = expanded();
    let (x, y) = i.origin();
    let (x, y) = (x + 30., y + 300.);
    assert!(i
        .pointer(Pointer::Press { x, y }, ms(1520))
        .actions
        .is_empty());
    for n in 1..=12 {
        assert!(i
            .pointer(
                Pointer::Motion {
                    x,
                    y: y - n as f32 * 10.
                },
                ms(1520 + n * 16)
            )
            .actions
            .is_empty());
    }
    assert!(i
        .pointer(Pointer::Release { x, y: y - 120. }, ms(1728))
        .actions
        .is_empty());
    assert!(i.animating(ms(1728)));
    let mut hidden = i.inputs().clone();
    hidden.calendar_open = true;
    hidden.calendar_progress = 1.;
    i.set_inputs(hidden, ms(1744));
    settle(&mut i, 1744, 3408);
    assert!(!i.animating(ms(3408)));
    assert!(i.next_deadline().is_none());
}
#[test]
fn native_pixel_scroll_phase_finishes_without_a_click_or_idle_timer() {
    let mut i = expanded();
    let (x, y) = i.origin();
    let (x, y) = (x + 30., y + 300.);
    for n in 0..10 {
        let phase = if n == 0 {
            ScrollPhase::Begin
        } else {
            ScrollPhase::Update
        };
        let out = i.wheel(
            x,
            y,
            Wheel {
                angle_delta: 0.,
                pixel_delta: if n == 0 { 0. } else { -10. },
                phase,
                synthesized: true,
            },
            ms(1520 + n * 16),
        );
        assert!(out.actions.is_empty());
    }
    assert!(i
        .wheel(
            x,
            y,
            Wheel {
                phase: ScrollPhase::End,
                synthesized: true,
                ..Default::default()
            },
            ms(1680)
        )
        .actions
        .is_empty());
    settle(&mut i, 1696, 3200);
    assert!(!i.animating(ms(3200)));
    assert!(i.next_deadline().is_none());
}
#[test]
fn ordinary_session_click_still_dispatches_exactly_once() {
    let mut i = expanded();
    let (x, y) = i.origin();
    let (x, y) = (x + 30., y + 100.);
    i.pointer(Pointer::Press { x, y }, ms(1520));
    let out = i.pointer(Pointer::Release { x, y }, ms(1536));
    assert_eq!(
        out.actions,
        vec![hyprv_ui_island::IslandAction::FocusAgent("s0".into())]
    );
    assert!(i
        .pointer(Pointer::Release { x, y }, ms(1552))
        .actions
        .is_empty());
}
