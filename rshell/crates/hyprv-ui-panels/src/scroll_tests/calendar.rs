use super::*;
include!("common.rs");
#[test]
fn qt_calendar_scroll_sequences() {
    for mode in ["calendar", "calendar-rebase"] {
        for name in sequences(mode) {
            let (events, csv) = sequence(mode, &name);
            let mut m = crate::fixtures::calendar_week(false);
            m.calendar.events.retain(|e| !e.all_day);
            m.wall = m.wall.date().and_hms_opt(9, 0, 0).unwrap();
            m.calendar.bridge = mode == "calendar-rebase" || name.contains("click");
            let mut c = CalendarPanel::new(520., 500.);
            c.ensure_init(&m);
            c.sync_scroll(&m);
            c.zones = vec![
                Zone {
                    id: Z::Timeline,
                    rect: Rect::new(0., 0., 450., 360.),
                    cursor: Cursor::Default,
                    enabled: true,
                },
                Zone {
                    id: Z::TimedEvent(0),
                    rect: Rect::new(0., 0., 450., 360.),
                    cursor: Cursor::Pointer,
                    enabled: true,
                },
            ];
            let mut clicks = 0;
            for (time, y, x, qt_clicks) in samples(&csv) {
                m.now = ms(time);
                c.tick(&m);
                c.ensure_init(&m);
                for e in events.iter().filter(|e| n(e, "t") as u64 == time) {
                    if e["type"] == "set" {
                        match e["key"].as_str().unwrap() {
                            "height" => c.height = n(e, "value") as f32 + 140.,
                            "contentY" => c.scroll_y.set_content_y(n(e, "value")),
                            _ => unreachable!(),
                        }
                        c.sync_scroll(&m);
                        continue;
                    }
                    let r = if e["type"] == "wheel" {
                        c.wheel(
                            100.,
                            200.,
                            native_wheel(e, true),
                            native_wheel(e, false),
                            &m,
                        )
                    } else {
                        c.pointer(pointer(e), &m, (0., 0.))
                    };
                    clicks += r
                        .actions
                        .iter()
                        .filter(|a| matches!(a, Action::CalendarOpenEvent { .. }))
                        .count();
                    c.ensure_init(&m);
                }
                if name == "viewport-during-flick" && time == 608 {
                    // Also exercise the real build path: painting must retain Qt's
                    // transient motion after a viewport extent changes mid-flick.
                    let zones = c.zones.clone();
                    c.build(&m, (0., 0.));
                    c.zones = zones;
                }
                check(&format!("{mode}-{name}-y"), time, y, c.content_y as f64);
                check(&format!("{mode}-{name}-x"), time, x, c.content_x as f64);
                if m.calendar.bridge {
                    assert_eq!(clicks, qt_clicks, "{name}@{time}: click dispatch");
                }
            }
            assert!(!c.animating(m.now), "{name}: idle frame request");
            assert_eq!(c.next_deadline(), None, "{name}: leaked delayed press");
            c.reset();
            assert!(!c.animating(m.now));
            assert_eq!(c.next_deadline(), None);
        }
    }
    report(
        "calendar",
        sequences("calendar").len() + sequences("calendar-rebase").len(),
    );
}

#[test]
fn calendar_delayed_press_deadline_is_cancelled_by_grab_or_reset() {
    let mut m = crate::fixtures::calendar_week(false);
    m.calendar.events.retain(|e| !e.all_day);
    let mut c = CalendarPanel::new(520., 500.);
    c.ensure_init(&m);
    c.sync_scroll(&m);
    c.zones = vec![Zone {
        id: Z::TimedEvent(0),
        rect: Rect::new(0., 0., 450., 360.),
        cursor: Cursor::Pointer,
        enabled: true,
    }];
    c.pointer(
        Pointer::Press {
            x: 100.,
            y: 200.,
            button: Button::Left,
        },
        &m,
        (0., 0.),
    );
    assert_eq!(c.next_deadline(), Some(ms(80)));
    assert_eq!(c.inter.pressed, None);
    m.now = ms(16);
    c.pointer(Pointer::Motion { x: 100., y: 170. }, &m, (0., 0.));
    assert_eq!(c.next_deadline(), None);
    m.now = ms(32);
    c.pointer(Pointer::Motion { x: 100., y: 140. }, &m, (0., 0.));
    let r = c.pointer(
        Pointer::Release {
            x: 100.,
            y: 140.,
            button: Button::Left,
        },
        &m,
        (0., 0.),
    );
    assert!(!r
        .actions
        .iter()
        .any(|a| matches!(a, Action::CalendarOpenEvent { .. })));
    c.reset();
    assert!(!c.animating(m.now));
    assert_eq!(c.next_deadline(), None);
}

#[test]
fn calendar_rebases_once_per_content_x_change() {
    let mut m = crate::fixtures::calendar_week(false);
    m.calendar.bridge = false;
    let mut c = CalendarPanel::new(520., 500.);
    c.ensure_init(&m);
    c.sync_scroll(&m);
    assert_eq!(c.set_content_x(0., &m), None);
    m.calendar.bridge = true;
    assert!(
        c.tick(&m).is_empty(),
        "a service update alone must not page the week"
    );
    assert!(matches!(
        c.set_content_x(1., &m),
        Some(Action::CalendarPreviousWeek)
    ));
    assert!(
        c.tick(&m).is_empty(),
        "a rebase's own assignment must not page twice"
    );
}
