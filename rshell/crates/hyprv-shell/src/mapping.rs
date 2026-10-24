//! Typed provider snapshots to presentation models and native input translation.
mod tray;
pub(crate) use tray::ordered as tray;
use crate::services::Hub;
use hyprv_ui_panels::model::*;
use std::time::Duration;
/// shell.qml:3189-3195, SystemClock.Minutes: seconds and subsecond fields are zero.
/// Quickshell core/clock.cpp:42-51 applies this at startup and every clock update.
pub(crate) fn minute_wall(wall: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    use chrono::Timelike;
    wall.date().and_hms_opt(wall.hour(), wall.minute(), 0).expect("valid minute")
}
pub fn panels(h: &Hub, output: &hyprv_ui_bar::model::Output, dark: bool, now: Duration) -> Model {
    use hyprv_ui_panels::services as svc;
    let mut model = Model {
        theme: Theme { dark },
        output: Output {
            name: output.name.clone(),
            width: output.width,
            height: output.height,
            scale: output.scale,
        },
        now,
        wall: minute_wall(chrono::Local::now().naive_local()),
        network: svc::network(&h.network.state.snapshot().value),
        bluetooth: svc::bluetooth(&h.bluetooth.state.snapshot().value),
        audio: svc::audio(&h.audio.topic.snapshot().value),
        media: svc::media(&h.mpris.topic.snapshot().value),
        stats: svc::stats(&h.stats.state.snapshot().value),
        calendar: svc::calendar(&h.calendar.state.snapshot().value),
        ..Default::default()
    };
    svc::controls(
        &mut model.controls,
        &h.power.state.snapshot().value,
        &h.backlight.state.snapshot().value,
        &output.name,
        &h.fan.state.snapshot().value,
        &h.inhibit.state.snapshot().value,
        &h.desktop.demo.state(),
        &h.notifications.topic.snapshot().value,
        &h.vm.state.snapshot().value,
        &h.desktop.viewflow.state(),
        &h.mihomo.state.snapshot().value,
    );
    if crate::laptop::active() { model.controls.screen_recording=h.desktop.recording.state().active; }
    model
}
use hyprv_ui::wayland::{InputEvent, KeySym, PointerKind};
use hyprv_widgets::controls::{Button, PointerEvent};
pub fn pointer(event: &InputEvent, position: &mut (f32, f32)) -> Option<PointerEvent> {
    if matches!(event, InputEvent::PointerCancel) {
        return Some(PointerEvent::Cancel);
    }
    if let InputEvent::Scroll { position: p, event } = event {
        *position = (p.0 as f32, p.1 as f32);
        return Some(PointerEvent::Wheel {
            angle_x: event.angle_delta.0 as f32,
            angle_y: event.angle_delta.1 as f32,
            pixel_x: event.pixel_delta.0 as f32,
            pixel_y: event.pixel_delta.1 as f32,
        });
    }
    if let InputEvent::Pointer { event, .. } = event {
        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
        *position = (x, y);
        let button = |b| match b {
            0x110 => Button::Left,
            0x111 => Button::Right,
            _ => Button::Middle,
        };
        Some(match event.kind {
            PointerKind::Enter { .. } => PointerEvent::Enter { x, y },
            PointerKind::Motion { .. } => PointerEvent::Move { x, y },
            PointerKind::Leave { .. } => PointerEvent::Leave,
            PointerKind::Press { button: b, .. } => PointerEvent::Press {
                x,
                y,
                button: button(b),
            },
            PointerKind::Release { button: b, .. } => PointerEvent::Release {
                x,
                y,
                button: button(b),
            },
            PointerKind::Axis {
                horizontal,
                vertical,
                ..
            } => PointerEvent::Wheel {
                angle_x: horizontal.discrete as f32 * 120.,
                angle_y: vertical.discrete as f32 * 120.,
                pixel_x: horizontal.absolute as f32,
                pixel_y: vertical.absolute as f32,
            },
        })
    } else {
        None
    }
}
pub fn panel_pointer(
    event: PointerEvent,
    position: (f32, f32),
) -> Option<hyprv_ui_panels::ui::Pointer> {
    use hyprv_ui_panels::ui::{Button as B, Pointer as P};
    let b = |b| match b {
        Button::Left => B::Left,
        Button::Right => B::Right,
        Button::Middle => B::Middle,
    };
    Some(match event {
        PointerEvent::Move { x, y } | PointerEvent::Enter { x, y } => P::Motion { x, y },
        PointerEvent::Leave | PointerEvent::Cancel => P::Leave,
        PointerEvent::Press { x, y, button } => P::Press {
            x,
            y,
            button: b(button),
        },
        PointerEvent::Release { x, y, button } => P::Release {
            x,
            y,
            button: b(button),
        },
        PointerEvent::Wheel {
            angle_x,
            angle_y,
            pixel_x,
            pixel_y,
        } => P::Wheel {
            x: position.0,
            y: position.1,
            angle_x,
            angle_y,
            pixel_x,
            pixel_y,
        },
    })
}
pub fn key(key: KeySym, text: Option<String>) -> Option<hyprv_ui_panels::ui::Key> {
    use hyprv_ui_panels::ui::Key as K;
    Some(match key {
        KeySym::Escape => K::Escape,
        KeySym::Return => K::Return,
        KeySym::KP_Enter => K::Enter,
        KeySym::BackSpace => K::Backspace,
        KeySym::Delete => K::Delete,
        KeySym::Left => K::Left,
        KeySym::Right => K::Right,
        KeySym::Home => K::Home,
        KeySym::End => K::End,
        KeySym::Tab => K::Tab,
        _ => K::Text(text?),
    })
}

pub fn island_pointer(
    event: PointerEvent,
    position: (f32, f32),
) -> Option<hyprv_ui_island::Pointer> {
    use hyprv_ui_island::Pointer as P;
    Some(match event {
        PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => P::Motion { x, y },
        PointerEvent::Leave => P::Leave,
        PointerEvent::Cancel => P::Cancel,
        PointerEvent::Press {
            x,
            y,
            button: Button::Left,
        } => P::Press { x, y },
        PointerEvent::Release {
            x,
            y,
            button: Button::Left,
        } => P::Release { x, y },
        PointerEvent::Wheel { angle_y, .. } => P::Wheel {
            x: position.0,
            y: position.1,
            angle_y,
        },
        _ => return None,
    })
}

pub fn offset_pointer(event: PointerEvent, origin: (f32, f32)) -> PointerEvent {
    let (dx, dy) = origin;
    match event {
        PointerEvent::Enter { x, y } => PointerEvent::Enter {
            x: x + dx,
            y: y + dy,
        },
        PointerEvent::Move { x, y } => PointerEvent::Move {
            x: x + dx,
            y: y + dy,
        },
        PointerEvent::Press { x, y, button } => PointerEvent::Press {
            x: x + dx,
            y: y + dy,
            button,
        },
        PointerEvent::Release { x, y, button } => PointerEvent::Release {
            x: x + dx,
            y: y + dy,
            button,
        },
        other => other,
    }
}

/// Preserve native phases instead of reconstructing a touchpad from deltas.
pub fn native_wheel(event: &InputEvent) -> Option<hyprv_widgets::flickable::Wheel> {
    use hyprv_ui::scroll::ScrollPhase as S;
    use hyprv_widgets::flickable::{ScrollPhase as W, Wheel};
    let InputEvent::Scroll { event, .. } = event else {
        return None;
    };
    Some(Wheel {
        angle_delta: event.angle_delta.1,
        pixel_delta: event.pixel_delta.1,
        synthesized: event.synthesized,
        phase: match event.phase {
            S::None => W::None,
            S::Begin => W::Begin,
            S::Update => W::Update,
            S::End => W::End,
        },
    })
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    fn native_wheel_retains_phases_pixels_and_high_resolution_angles() {
        use hyprv_ui::scroll::{ScrollEvent, ScrollPhase};
        for phase in [ScrollPhase::Begin, ScrollPhase::Update, ScrollPhase::End] {
            let event = InputEvent::Scroll {
                position: (42., 81.),
                event: ScrollEvent {
                    angle_delta: (-15., -30.),
                    pixel_delta: (-1., -3.),
                    phase,
                    synthesized: true,
                    inverted: true,
                },
            };
            let wheel = native_wheel(&event).unwrap();
            assert_eq!(wheel.angle_delta, -30.);
            assert_eq!(wheel.pixel_delta, -3.);
            assert!(wheel.synthesized);
            let mut pos = (0., 0.);
            assert!(matches!(
                pointer(&event, &mut pos),
                Some(PointerEvent::Wheel {
                    angle_y: -30.,
                    pixel_y: -3.,
                    ..
                })
            ));
            assert_eq!(pos, (42., 81.));
            assert!(matches!(
                (phase, wheel.phase),
                (
                    ScrollPhase::Begin,
                    hyprv_widgets::flickable::ScrollPhase::Begin
                ) | (
                    ScrollPhase::Update,
                    hyprv_widgets::flickable::ScrollPhase::Update
                ) | (ScrollPhase::End, hyprv_widgets::flickable::ScrollPhase::End)
            ));
        }
    }
    #[test]
    fn cancel_and_keypad_enter_are_distinct() {
        assert!(matches!(
            pointer(&InputEvent::PointerCancel, &mut (0., 0.)),
            Some(PointerEvent::Cancel)
        ));
        assert!(matches!(
            island_pointer(PointerEvent::Cancel, (0., 0.)),
            Some(hyprv_ui_island::Pointer::Cancel)
        ));
        assert_eq!(
            key(KeySym::KP_Enter, Some("\r".into())),
            Some(hyprv_ui_panels::ui::Key::Enter)
        );
    }
}

#[cfg(test)]
mod minute_clock_tests {
    use super::minute_wall;
    #[test]
    fn calendar_wall_is_minute_precision_including_rollover() {
        use chrono::{Duration, NaiveDate};
        let t=NaiveDate::from_ymd_opt(2026,12,31).unwrap().and_hms_milli_opt(23,59,37,900).unwrap();
        assert_eq!(minute_wall(t), t.date().and_hms_opt(23,59,0).unwrap());
        let next=t+Duration::milliseconds(22_100);
        assert_eq!(minute_wall(next),NaiveDate::from_ymd_opt(2027,1,1).unwrap().and_hms_opt(0,0,0).unwrap());
    }
}
