//! Offscreen PNGs for every "visual state worth a reference screenshot" in ui-panels.md §5
//! (written to target-png/states/), with geometry assertions taken from the specs.
mod common;
use hyprv_ui::{
    geometry::Rect,
    scene::{Content, Node, Scene},
};
use hyprv_ui_panels::{
    calendar::CalendarPanel,
    control_panel::{ControlPanel, Page},
    fixtures,
    model::*,
    quick_adjust::{Mode, QuickAdjust},
    stats::StatsPopup,
    startup::StartupConfirm,
    ui::{Button, Pointer},
    wifi::WifiPanel,
};
use std::time::Duration;

fn shot(name: &str, node: Node, w: f32, h: f32, scale: f64) -> (usize, usize, Vec<u8>) {
    let mut root = Node::new(Rect::new(0., 0., w, h), Content::Group);
    root.children.push(node);
    let scene = Scene::new(root);
    let dir = common::out_dir().join("states");
    std::fs::create_dir_all(&dir).unwrap();
    let px = common::render(&scene, w, h, scale);
    common::write_rgba(&dir.join(format!("{name}@{scale}.png")), px.0, px.1, &px.2);
    px
}
fn themes() -> [(bool, &'static str); 2] {
    [(false, "light"), (true, "dark")]
}
fn opaque_pixels(px: &(usize, usize, Vec<u8>)) -> usize {
    px.2.chunks(4).filter(|p| p[3] > 0).count()
}

#[test]
fn control_panel_states() {
    for (dark, t) in themes() {
        for scale in [1., 2.] {
            for (name, f) in [("idle", fixtures::control_idle as fixtures::Fixture), ("off", fixtures::control_off)] {
                let mut m = f(dark);
                m.output.scale = scale as f32;
                let mut cp = ControlPanel::new();
                let (n, w, h) = cp.page_preview(&m, Page::Main);
                assert_eq!((w, h), (278., 422.), "main page card");
                let px = shot(&format!("control-{name}--{t}"), n, w, h, scale);
                assert!(opaque_pixels(&px) > 1000);
            }
        }
        // 3. hover/press states
        let m = fixtures::control_idle(dark);
        let mut cp = ControlPanel::new();
        cp.open_for(Anchor { x: 3000., y: 10., width: 28., height: 38. }, "DP-4", Duration::ZERO);
        let mut mm = m.clone();
        mm.now = Duration::from_millis(400);
        cp.tick(mm.now);
        let _ = cp.build(&mm);
        // pointer over the DND toggle (card x = 3072-288 = 2784, y = 58)
        let (cx, cy) = (2784., 58.);
        cp.pointer(Pointer::Motion { x: cx + 216. + 30., y: cy + 144. + 30. }, &mm);
        let (n, w, h) = cp.page_preview(&mm, Page::Main);
        shot(&format!("control-hover-dnd--{t}"), n, w, h, 2.);
        cp.pointer(Pointer::Press { x: cx + 9. + 20., y: cy + 9. + 20., button: Button::Left }, &mm);
        let (n, w, h) = cp.page_preview(&mm, Page::Main);
        shot(&format!("control-pressed-wifi-well--{t}"), n, w, h, 2.);
        // 4. slider mid-drag at 0/37/100
        for v in [0., 37., 100.] {
            let mut cp = ControlPanel::new();
            cp.open_for(Anchor { x: 3000., y: 10., width: 28., height: 38. }, "DP-4", Duration::ZERO);
            let _ = cp.build(&mm);
            let x = cx + 134. * v / 100.;
            cp.pointer(Pointer::Motion { x: cx + 10., y: cy + 300. }, &mm);
            cp.pointer(Pointer::Press { x: cx + 10., y: cy + 300., button: Button::Left }, &mm);
            let r = cp.pointer(Pointer::Motion { x, y: cy + 300. }, &mm);
            assert!(r.actions.iter().any(|a| matches!(a, Action::BrightnessSet(_))));
            let (n, w, h) = cp.page_preview(&mm, Page::Main);
            shot(&format!("control-slider-{v}--{t}"), n, w, h, 2.);
        }
        // 5. VM busy / running
        for (name, running, busy) in [("vm-busy", false, true), ("vm-running", true, false)] {
            let mut m = fixtures::control_idle(dark);
            m.controls.vm_running = running;
            m.controls.vm_busy = busy;
            let (n, w, h) = ControlPanel::new().page_preview(&m, Page::Main);
            shot(&format!("control-{name}--{t}"), n, w, h, 2.);
        }
        // 7. Bluetooth page states
        for (name, f) in [("bt-off", fixtures::bt_off as fixtures::Fixture), ("bt-scanning", fixtures::bt_scanning), ("bt-devices", fixtures::bt_devices)] {
            let m = f(dark);
            let (n, w, h) = ControlPanel::new().page_preview(&m, Page::Bluetooth);
            assert_eq!(w, 384.);
            shot(&format!("{name}--{t}"), n, w, h, 2.);
        }
        // list > 420 px: 8 devices → clipped at 420
        let mut m = fixtures::bt_devices(dark);
        let d = m.bluetooth.devices[1].clone();
        for i in 0..6 {
            let mut x = d.clone();
            x.address = format!("00:00:00:00:00:0{i}");
            x.display_name = format!("Device {i}");
            m.bluetooth.devices.push(x);
        }
        let (n, w, h) = ControlPanel::new().page_preview(&m, Page::Bluetooth);
        // header 40 + status 63 + chips 34 + message (16+18) + list 420 + 4×10 spacing + 20 padding
        assert_eq!(h, 10. + 40. + 10. + 63. + 10. + 34. + 10. + 34. + 10. + 420. + 10.);
        shot(&format!("bt-list-scroll--{t}"), n, w, h, 2.);
        // 9. Mihomo page online / offline
        for (name, online) in [("mihomo-online", true), ("mihomo-offline", false)] {
            let mut m = fixtures::control_idle(dark);
            m.controls.mihomo = Mihomo { online, service: "user".into(), mode: "rule".into(), connections: 42, upload: 2400, download: 1_250_000, dashboard: "http://127.0.0.1:9090/ui".into(), ..Default::default() };
            let (n, w, h) = ControlPanel::new().page_preview(&m, Page::Mihomo);
            shot(&format!("control-{name}--{t}"), n, w, h, 2.);
        }
    }
}

#[test]
fn control_panel_open_close_frames() {
    // 6. open frames at 16/60/120/220 ms; close at 60/150/210 ms (from the spec timeline)
    let m0 = fixtures::control_idle(false);
    let mut cp = ControlPanel::new();
    cp.open_for(Anchor { x: 3000., y: 10., width: 28., height: 38. }, "DP-4", Duration::ZERO);
    let _ = cp.build(&m0); // the host builds the surface on map
    let mut heights = Vec::new();
    for t in [16u64, 60, 120, 220, 236, 300] {
        let mut m = m0.clone();
        m.now = Duration::from_millis(t);
        cp.tick(m.now);
        let _ = cp.build(&m);
        heights.push((t, cp.m.reveal.values().height, cp.m.reveal.values().opacity));
    }
    // open timer fires at 16 ms, then a 20 ms pause; reveal OutCubic over 200 ms → done at 236 ms
    assert_eq!(heights[0].1, 0.);
    assert!(heights[2].1 > 200. && heights[2].1 < 422.);
    assert_eq!(heights[4].1, 422.);
    assert_eq!(heights[5].2, 1.);
    let mut m = m0.clone();
    m.now = Duration::from_millis(1000);
    cp.close(m.now);
    let mut unmapped_at = None;
    for t in (1000..1300).step_by(5) {
        m.now = Duration::from_millis(t);
        let a = cp.tick(m.now);
        if a.iter().any(|a| matches!(a, Action::Host(HostAction::Unmapped(PanelKind::ControlPanel)))) {
            unmapped_at = Some(t - 1000);
            break;
        }
    }
    // close: reveal holds 30 ms then 180 ms InCubic → unmap at 210 ms
    assert_eq!(unmapped_at, Some(210));
}

#[test]
fn wifi_states() {
    for (dark, t) in themes() {
        for (name, f) in [
            ("wifi-connected", fixtures::wifi_connected as fixtures::Fixture),
            ("wifi-ethernet", fixtures::wifi_ethernet),
            ("wifi-off", fixtures::wifi_off),
            ("wifi-blocked", fixtures::wifi_blocked),
            ("wifi-many", fixtures::wifi_many),
        ] {
            let m = f(dark);
            let mut w = WifiPanel::new();
            let (n, ww, h) = w.preview(&m);
            assert_eq!(ww, 384.);
            shot(&format!("{name}--{t}"), n, ww, h, 2.);
        }
        // 11. secure network expanded, empty and with text, focused
        for (name, pw) in [("wifi-password-empty", ""), ("wifi-password-text", "hunter22")] {
            let m = fixtures::wifi_connected(dark);
            let mut w = WifiPanel::new();
            w.preview(&m);
            w.expand_now("Neighbour", pw, true);
            let (n, ww, h) = w.preview(&m);
            shot(&format!("{name}--{t}"), n, ww, h, 2.);
        }
    }
}

#[test]
fn wifi_geometry() {
    let m = fixtures::wifi_connected(false);
    let mut w = WifiPanel::new();
    let (_, _, h_collapsed) = w.preview(&m);
    w.expand_now("Neighbour", "", true);
    let (_, _, h_expanded) = w.preview(&m);
    // collapsed card 59 → expanded 151 (ui-panels-wifi.md §4 item 6)
    assert_eq!(h_expanded - h_collapsed, 151. - 59.);
}

#[test]
fn quick_adjust_states() {
    for (dark, t) in themes() {
        for (mode, name, vals) in [(Mode::Brightness, "brightness", [0, 50, 100]), (Mode::Volume, "volume", [0, 30, 100])] {
            for v in vals {
                let mut m = fixtures::control_idle(dark);
                m.controls.brightness_percent = v;
                m.audio.volume_percent = v;
                m.audio.muted = v == 0 && mode == Mode::Volume;
                let mut q = QuickAdjust::new();
                let (n, w, h) = q.preview(&m, mode);
                assert_eq!((w, h), (278., 62.));
                shot(&format!("quick-{name}-{v}--{t}"), n, w, h, 2.);
            }
        }
    }
    // show/hide timeline: auto-hide after 3000 ms, unmap 140 ms later
    let mut q = QuickAdjust::new();
    let a = q.show(Mode::Volume, Duration::ZERO);
    assert!(a.iter().any(|a| matches!(a, Action::Host(HostAction::Mapped(PanelKind::QuickAdjust)))));
    assert!(q.tick(Duration::from_millis(2999)).is_empty());
    assert!(q.tick(Duration::from_millis(3000)).is_empty());
    let a = q.tick(Duration::from_millis(3140));
    assert!(a.iter().any(|a| matches!(a, Action::Host(HostAction::Unmapped(PanelKind::QuickAdjust)))));
}

#[test]
fn stats_states() {
    for (dark, t) in themes() {
        for (name, f) in [("stats-full", fixtures::stats_full as fixtures::Fixture), ("stats-one", fixtures::stats_one), ("stats-empty", fixtures::stats_empty)] {
            let m = f(dark);
            let (n, w, h) = StatsPopup::preview(&m);
            assert_eq!(w, 548.);
            assert_eq!(h, 572., "ui-panels-stats.md §2.3 total height");
            shot(&format!("{name}--{t}"), n, w, h, 2.);
        }
    }
}

#[test]
fn calendar_states() {
    for (dark, t) in themes() {
        for (name, f) in [("calendar-week", fixtures::calendar_week as fixtures::Fixture), ("calendar-connecting", fixtures::calendar_connecting)] {
            let m = f(dark);
            let mut c = CalendarPanel::new(520., 500.);
            let n = c.build(&m, (0., 0.));
            shot(&format!("{name}--{t}"), n, 520., 500., 2.);
        }
        // 18. tooltip on an event + header hover
        let m = fixtures::calendar_week(dark);
        let mut c = CalendarPanel::new(520., 500.);
        c.build(&m, (0., 0.));
        // hover the Standup event (Friday = buffer day 11 → visible column 4)
        let dw = c.day_width();
        let tl_y = 58. + 48. + (12f32 + 2. * 24.).clamp(34., 106.);
        let ev_x = 58. + 4. * dw + 3. + 4.;
        let r = c.pointer(Pointer::Motion { x: ev_x, y: tl_y + (9.5 - 12.5) * 44. + 10. }, &m, (0., 0.));
        let _ = r;
        let n = c.build(&m, (0., 0.));
        shot(&format!("calendar-tooltip--{t}"), n, 520., 500., 2.);
    }
}

#[test]
fn startup_states() {
    for (dark, t) in themes() {
        let mut s = StartupConfirm::new(dark, Duration::ZERO);
        assert_eq!(s.height(), 156., "card height (ui-panels-calendar.md §4.3)");
        let n = s.build(2.);
        shot(&format!("startup--{t}"), n, 360., 156., 2.);
        s.pointer(Pointer::Motion { x: 360. - 24. - 44., y: 156. - 24. - 16. });
        let n = s.build(2.);
        shot(&format!("startup-hover-launch--{t}"), n, 360., 156., 2.);
    }
}
