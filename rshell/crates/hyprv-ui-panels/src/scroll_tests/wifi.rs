use super::*;
include!("common.rs");
#[test]
fn qt_wifi_scroll_sequences() {
    for name in sequences("wifi") {
        let (events, csv) = sequence("wifi", &name);
        let mut m = crate::fixtures::wifi_connected(false);
        let mut net = m.network.networks[0].clone();
        net.secure = false;
        net.known = true;
        net.active = false;
        let mut w = WifiPanel::new();
        w.m.mapped = true;
        w.popup_visible = true;
        w.displayed = (0..10)
            .map(|i| {
                let mut d = net.clone();
                d.ssid = format!("test-{i}");
                d
            })
            .collect();
        w.last_card = Rect::new(0., 0., 520., 500.);
        let mut clicks = 0;
        for (time, y, _, qt_clicks) in samples(&csv) {
            m.now = ms(time);
            w.tick(&m);
            let view = w.list_extent().1;
            w.zones = vec![Zone {
                id: Z::List,
                rect: Rect::new(0., 0., 364., view),
                cursor: Cursor::Default,
                enabled: true,
            }];
            let mut top = -w.scroll;
            for d in &w.displayed {
                let height = w.card_height(d);
                let lo = top.max(0.);
                let hi = (top + height).min(view);
                if hi > lo {
                    w.zones.push(Zone {
                        id: Z::Connect(d.ssid.clone()),
                        rect: Rect::new(0., lo, 364., hi - lo),
                        cursor: Cursor::Pointer,
                        enabled: true,
                    });
                }
                top += height + 10.;
            }
            for e in events.iter().filter(|e| n(e, "t") as u64 == time) {
                if e["type"] == "set" {
                    match e["key"].as_str().unwrap() {
                        "contentY" => w.scrolling.set_content_y(n(e, "value")),
                        "rows" => w.displayed.truncate(n(e, "value") as usize),
                        "extra" => {
                            w.sections
                                .insert("test-0".into(), Reveal::new(n(e, "value") as f32 - 10.));
                        }
                        _ => unreachable!(),
                    }
                    w.sync_scroll();
                } else {
                    let r = w.pointer(pointer(e), &m);
                    clicks += r
                        .actions
                        .iter()
                        .filter(|a| matches!(a, Action::WifiConnect { .. }))
                        .count();
                }
            }
            check(&name, time, y, w.scroll as f64);
            assert_eq!(clicks, qt_clicks, "{name}@{time}: click dispatch");
        }
        assert!(!w.animating(m.now), "{name}: idle frame request");
        assert_eq!(w.next_deadline(m.now), None, "{name}: idle deadline");
        w.scrolling.press(200., m.now);
        w.m.unmap_now();
        w.scrolling.reset(0.);
        assert!(!w.scrolling.moving());
    }
    report("wifi", sequences("wifi").len());
}

#[test]
fn wifi_close_cancels_scroll_and_frame_requests() {
    let mut m = crate::fixtures::wifi_many(false);
    let mut w = WifiPanel::new();
    w.open(&m);
    w.build(&m);
    m.now = ms(16);
    w.tick(&m);
    m.now = ms(600);
    w.tick(&m);
    w.scrolling.press(300., m.now);
    w.scrolling.drag_to(270., Some(-1200.), ms(616));
    w.scrolling.drag_to(240., Some(-1200.), ms(632));
    w.scrolling.release(240., ms(648));
    m.now = ms(648);
    w.close(&m);
    m.now = ms(1000);
    w.tick(&m);
    assert!(!w.is_mapped());
    assert!(!w.scrolling.moving());
    assert!(!w.animating(m.now));
    assert_eq!(w.next_deadline(m.now), None);
}
