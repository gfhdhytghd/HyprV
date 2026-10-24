use super::*;
include!("common.rs");
#[test]
fn qt_bluetooth_scroll_sequences() {
    for name in sequences("bluetooth") {
        let (events, csv) = sequence("bluetooth", &name);
        let mut m = crate::fixtures::bt_devices(false);
        let d = m.bluetooth.devices[0].clone();
        m.bluetooth.devices = (0..10)
            .map(|i| {
                let mut d = d.clone();
                d.address = format!("device-{i}");
                d
            })
            .collect();
        let mut c = ControlPanel::new();
        c.m.mapped = true;
        c.set_page(Page::Bluetooth);
        c.last_card = Rect::new(0., 0., 520., 500.);
        c.sync_bt_scroll(&m);
        let mut clicks = 0;
        for (time, y, _, qt_clicks) in samples(&csv) {
            m.now = ms(time);
            c.tick(m.now);
            let view = bt_list_extent(&m).1;
            c.zones = vec![Zone {
                id: Z::BtList,
                rect: Rect::new(0., 0., 364., view),
                cursor: Cursor::Default,
                enabled: true,
            }];
            for i in 0..m.bluetooth.devices.len() {
                let top = i as f32 * 68. - c.bt_scroll;
                let lo = top.max(0.);
                let hi = (top + 58.).min(view);
                if hi > lo {
                    c.zones.push(Zone {
                        id: Z::BtDevice(i),
                        rect: Rect::new(0., lo, 364., hi - lo),
                        cursor: Cursor::Pointer,
                        enabled: true,
                    });
                }
            }
            for e in events.iter().filter(|e| n(e, "t") as u64 == time) {
                if e["type"] == "set" {
                    match e["key"].as_str().unwrap() {
                        "contentY" => c.bt_flick.set_content_y(n(e, "value")),
                        "rows" => m.bluetooth.devices.truncate(n(e, "value") as usize),
                        _ => unreachable!(),
                    }
                    c.sync_bt_scroll(&m);
                } else {
                    let r = if e["type"] == "wheel" {
                        c.bluetooth_wheel(100., 200., native_wheel(e, false), &m)
                    } else {
                        c.pointer(pointer(e), &m)
                    };
                    clicks += r
                        .actions
                        .iter()
                        .filter(|a| {
                            matches!(
                                a,
                                Action::BluetoothConnect { .. }
                                    | Action::BluetoothDisconnect { .. }
                            )
                        })
                        .count();
                }
            }
            check(&name, time, y, c.bt_scroll as f64);
            assert_eq!(clicks, qt_clicks, "{name}@{time}: click dispatch");
        }
        assert!(!c.animating(m.now, &m), "{name}: idle frame request");
        assert_eq!(c.next_deadline(), None);
        c.bt_flick.wheel(Wheel::deltas(-120., 0.), m.now);
        c.set_page(Page::Main);
        assert!(!c.bt_flick.moving());
    }
    report("bluetooth", sequences("bluetooth").len());
}
