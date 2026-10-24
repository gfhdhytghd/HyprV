use hyprv_ui::geometry::Rect;
use hyprv_ui_bar::{
    input::PopupCommand,
    model::*,
    popups::{Kind, Popups},
};
use hyprv_widgets::controls::{Button, PointerEvent};
use std::time::Duration;
fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
fn opened() -> (Model, Popups) {
    let mut m = Model::default();
    m.tray.items = vec![TrayItem {
        key: "app".into(),
        has_menu: true,
        ..Default::default()
    }];
    m.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
        item_key: "app".into(),
        root: TrayMenuEntry {
            children: (0..30)
                .map(|id| TrayMenuEntry {
                    id,
                    text: format!("Entry {id}"),
                    visible: true,
                    enabled: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        revision: 1,
    }];
    let mut p = Popups::default();
    p.command(
        &m,
        PopupCommand::Menu {
            item: "app".into(),
            anchor: Rect::new(200., 10., 18., 38.),
            source: Target::Tray("app".into()),
        },
        ms(0),
    );
    for t in [0, 16, 216] {
        p.update(&m, ms(t));
    }
    (m, p)
}
#[test]
fn drag_flick_cancels_menu_click_and_stops_frames() {
    let (m, mut p) = opened();
    let r = p.card_rect(&m, Kind::Menu);
    let (x, y) = (r.x + 30., r.y + 250.);
    p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Press {
            x,
            y,
            button: Button::Left,
        },
        ms(320),
    );
    for i in 1..=12 {
        p.pointer(
            &m,
            Kind::Menu,
            PointerEvent::Move {
                x,
                y: y - i as f32 * 10.,
            },
            ms(320 + i * 16),
        );
    }
    let out = p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Release {
            x,
            y: y - 120.,
            button: Button::Left,
        },
        ms(528),
    );
    assert!(out.actions.is_empty());
    assert!(p.menu.scroll > 0.);
    assert!(p.animating());
    let released = p.menu.scroll;
    for t in (544..=3000).step_by(16) {
        p.update(&m, ms(t));
    }
    assert!(p.menu.scroll > released);
    assert!(!p.animating());
    assert!(p.next_deadline().is_none());
    p.escape(Kind::Menu, ms(3020));
    p.update(&m, ms(3300));
    p.update(&m, ms(3500));
    assert!(!p.menu.surface.mapped);
    assert!(!p.animating());
    assert!(p.next_deadline().is_none());
}
#[test]
fn wheel_uses_qt_easing_instead_of_a_fixed_jump() {
    let (m, mut p) = opened();
    p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Wheel {
            angle_y: -120.,
            angle_x: 0.,
            pixel_y: 0.,
            pixel_x: 0.,
        },
        ms(320),
    );
    assert_eq!(p.menu.scroll, 0.);
    assert!(p.animating());
    for t in (336..=656).step_by(16) {
        p.update(&m, ms(t));
    }
    assert_eq!(p.menu.scroll, 72.);
    assert!(!p.animating());
}
#[test]
fn ordinary_menu_click_still_dispatches_exactly_once() {
    let (m, mut p) = opened();
    let r = p.card_rect(&m, Kind::Menu);
    let (x, y) = (r.x + 20., r.y + 20.);
    p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Press {
            x,
            y,
            button: Button::Left,
        },
        ms(320),
    );
    let out = p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Release {
            x,
            y,
            button: Button::Left,
        },
        ms(336),
    );
    assert!(matches!(
        out.actions.as_slice(),
        [Action::Tray(TrayAction::MenuEvent { entry: 0, .. })]
    ));
    assert!(p
        .pointer(
            &m,
            Kind::Menu,
            PointerEvent::Release {
                x,
                y,
                button: Button::Left
            },
            ms(352)
        )
        .actions
        .is_empty());
}
