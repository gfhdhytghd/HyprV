mod common;
use hyprv_ui::geometry::Rect;
use hyprv_ui_bar::{model::*, PopupCommand, PopupKind, Popups};
use hyprv_widgets::controls::{Button, PointerEvent};
use std::time::Duration;
fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
fn menu(m: &Model, p: &mut Popups, t: u64) {
    p.command(
        m,
        PopupCommand::Menu {
            item: "tray-0".into(),
            anchor: Rect::new(200., 10., 18., 38.),
            source: Target::Tray("tray-0".into()),
        },
        ms(t),
    );
    p.update(m, ms(t));
}
fn populated() -> Model {
    let mut m = common::baseline();
    m.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
        item_key: "tray-0".into(),
        root: TrayMenuEntry {
            children: common::menu_entries(),
            ..Default::default()
        },
        revision: 1,
    }];
    m
}
fn event(
    p: &mut Popups,
    m: &Model,
    kind: PopupKind,
    press: bool,
    x: f32,
    y: f32,
    t: u64,
) -> Vec<Action> {
    p.pointer(
        m,
        kind,
        if press {
            PointerEvent::Press {
                x,
                y,
                button: Button::Left,
            }
        } else {
            PointerEvent::Release {
                x,
                y,
                button: Button::Left,
            }
        },
        ms(t),
    )
    .actions
}
#[test]
fn changing_menu_page_destroys_delegate_hover_until_next_pointer_move() {
    let m = populated();
    let mut p = Popups::default();
    menu(&m, &mut p, 0);
    p.update(&m, ms(16));
    p.update(&m, ms(616));
    let card = p.card_rect(&m, PopupKind::Menu);
    // Five preceding rows: four ordinary rows and one separator.
    let (x, y) = (card.x + 30., card.y + 9. + 4. * 35. + 2. + 17.);
    p.pointer(&m, PopupKind::Menu, PointerEvent::Move { x, y }, ms(617));
    assert_eq!(p.menu.hovered, Some(6));
    event(&mut p, &m, PopupKind::Menu, true, x, y, 618);
    let actions = event(&mut p, &m, PopupKind::Menu, false, x, y, 619);
    assert!(matches!(
        actions.as_slice(),
        [Action::Tray(TrayAction::MenuAboutToShow { entry: 6, .. })]
    ));
    p.update(&m, ms(1219));
    assert_eq!(p.menu.stack, vec![6]);
    assert_eq!(p.menu.hovered, None);
    let card = p.card_rect(&m, PopupKind::Menu);
    let (x, y) = (card.x + 30., card.y + 9. + 17.);
    p.pointer(&m, PopupKind::Menu, PointerEvent::Move { x, y }, ms(1220));
    assert_eq!(p.menu.hovered, Some(i32::MIN));
    event(&mut p, &m, PopupKind::Menu, true, x, y, 1221);
    assert!(event(&mut p, &m, PopupKind::Menu, false, x, y, 1222).is_empty());
    assert!(p.menu.stack.is_empty());
    assert_eq!(p.menu.hovered, None);
}
#[test]
fn empty_menu_waits_for_service_change_without_idle_polling() {
    let mut m = common::baseline();
    let mut p = Popups::default();
    menu(&m, &mut p, 0);
    for frame in 1..=100 {
        p.update(&m, ms(frame * 16));
    }
    assert!(!p.animating());
    assert_eq!(p.next_deadline(), None);
    assert_eq!(p.menu.surface.reveal.values().height, 0.);
    m.tray.menus = populated().tray.menus;
    p.update(&m, ms(1700));
    assert_eq!(p.next_deadline(), Some(ms(1716)));
    assert_eq!(p.menu.surface.reveal.values().height, 0.);
    p.update(&m, ms(1716));
    p.update(&m, ms(1916));
    assert_eq!(p.menu.surface.reveal.values().opacity, 1.);
    m.tray.items.clear();
    p.update(&m, ms(2000));
    assert!(!p.menu.surface.mapped);
    assert!(!p.animating());
    assert_eq!(p.next_deadline(), None);
}
#[test]
fn menu_input_follows_reveal_and_unmap() {
    let m = populated();
    let mut p = Popups::default();
    menu(&m, &mut p, 0);
    let r = p.card_rect(&m, PopupKind::Menu);
    let (x, y) = (r.x + 20., r.y + 20.);
    assert_eq!(p.menu.surface.reveal.values().height, 0.);
    assert!(event(&mut p, &m, PopupKind::Menu, true, x, y, 1).is_empty());
    assert!(event(&mut p, &m, PopupKind::Menu, false, x, y, 2).is_empty());
    menu(&m, &mut p, 20);
    p.update(&m, ms(36));
    p.update(&m, ms(56));
    let invisible_y = r.y + r.height - 20.;
    assert!(invisible_y > r.y + p.menu.surface.reveal.values().height);
    event(&mut p, &m, PopupKind::Menu, true, x, invisible_y, 57);
    assert!(event(&mut p, &m, PopupKind::Menu, false, x, invisible_y, 58).is_empty());
    menu(&m, &mut p, 100);
    p.update(&m, ms(116));
    p.update(&m, ms(316));
    event(&mut p, &m, PopupKind::Menu, true, x, y, 320);
    let actions = event(&mut p, &m, PopupKind::Menu, false, x, y, 321);
    assert!(matches!(
        actions.as_slice(),
        [Action::Tray(TrayAction::MenuEvent { .. })]
    ));
    // A grab accepted while visible cannot survive a close/unmap.
    event(&mut p, &m, PopupKind::Menu, true, x, y, 322);
    p.escape(PopupKind::Menu, ms(330));
    p.update(&m, ms(530));
    assert!(!p.menu.surface.mapped);
    assert!(event(&mut p, &m, PopupKind::Menu, false, x, y, 531).is_empty());
}
#[test]
fn overflow_press_cannot_cross_release_close_or_remap() {
    let mut m = common::baseline();
    m.output.width = 1000.;
    m.tray.items = (0..30)
        .map(|i| TrayItem {
            key: format!("app{i}"),
            id: format!("app{i}"),
            ..Default::default()
        })
        .collect();
    let mut p = Popups::default();
    let command = || PopupCommand::Overflow {
        anchor: Rect::new(700., 10., 22., 38.),
        source: Target::Overflow,
    };
    for close in 0..3 {
        p.command(&m, command(), ms(0));
        let r = p.card_rect(&m, PopupKind::Overflow);
        let (x, y) = (r.x + 17., r.y + 20.);
        event(&mut p, &m, PopupKind::Overflow, true, x, y, 1);
        match close {
            0 => {
                event(&mut p, &m, PopupKind::Overflow, false, 0., 0., 2);
            }
            1 => {
                p.escape(PopupKind::Overflow, ms(2));
            }
            _ => {
                p.command(&m, command(), ms(2));
            }
        }
        assert!(!p.overflow.surface.mapped);
        p.command(&m, command(), ms(3));
        assert!(event(&mut p, &m, PopupKind::Overflow, false, x, y, 4).is_empty());
        event(&mut p, &m, PopupKind::Overflow, true, x, y, 5);
        assert!(matches!(
            event(&mut p, &m, PopupKind::Overflow, false, x, y, 6).as_slice(),
            [Action::Tray(TrayAction::Activate { .. })]
        ));
        p.escape(PopupKind::Overflow, ms(7));
    }
}

#[test]
fn battery_live_discovery_quirk_requires_detail_availability() {
    let mut m = common::baseline();
    m.power.battery_present = true;
    m.power.battery_percent = Some(64.);
    m.power.on_battery = false;
    if QUIRK_BATTERY_POPUP_NEVER_OPENS {
        assert!(!m.battery_available());
        assert!(
            !hyprv_ui_bar::layout::compute(&m, &m.theme)
                .modules
                .iter()
                .find(|n| n.target == Target::Battery)
                .unwrap()
                .interactive
        );
        assert!(!hyprv_ui_bar::format::power_label(&m).contains('%'));
    }
    m.battery.available = true;
    m.battery.capacity_percent = Some(64.);
    m.battery.mode = BatteryMode::Charging;
    assert!(m.battery_available());
    assert!(hyprv_ui_bar::format::power_label(&m).contains("64%"));
}

#[test]
fn cancelled_hydration_does_not_reopen_on_late_service_data() {
    let mut m = common::baseline();
    let mut p = Popups::default();
    menu(&m, &mut p, 0);
    p.update(&m, ms(16));
    assert_eq!(p.next_deadline(), None);
    p.escape(PopupKind::Menu, ms(20));
    p.update(&m, ms(220));
    p.update(&m, ms(400));
    assert!(!p.menu.surface.mapped);
    m.tray.menus = populated().tray.menus;
    for t in [500, 516, 716] {
        p.update(&m, ms(t));
        assert!(!p.menu.surface.mapped);
        assert!(!p.animating());
        assert_eq!(p.next_deadline(), None);
    }
}

#[test]
fn closing_menu_cancels_grab_before_unmap_and_empty_overflow_clears_grab() {
    let mut m = populated();
    let mut p = Popups::default();
    menu(&m, &mut p, 0);
    p.update(&m, ms(16));
    p.update(&m, ms(216));
    let r = p.card_rect(&m, PopupKind::Menu);
    event(&mut p, &m, PopupKind::Menu, true, r.x + 20., r.y + 20., 220);
    p.escape(PopupKind::Menu, ms(230));
    p.update(&m, ms(250));
    assert!(p.menu.surface.mapped);
    assert!(event(
        &mut p,
        &m,
        PopupKind::Menu,
        false,
        r.x + 20.,
        r.y + 20.,
        251
    )
    .is_empty());

    m.output.width = 1000.;
    m.tray.items = (0..30)
        .map(|i| TrayItem {
            key: format!("app{i}"),
            id: format!("app{i}"),
            ..Default::default()
        })
        .collect();
    let items = m.tray.items.clone();
    let command = || PopupCommand::Overflow {
        anchor: Rect::new(700., 10., 22., 38.),
        source: Target::Overflow,
    };
    p.command(&m, command(), ms(500));
    let r = p.card_rect(&m, PopupKind::Overflow);
    let (x, y) = (r.x + 17., r.y + 20.);
    event(&mut p, &m, PopupKind::Overflow, true, x, y, 501);
    m.tray.items.clear();
    p.update(&m, ms(502));
    assert!(!p.overflow.surface.mapped);
    m.tray.items = items;
    p.command(&m, command(), ms(503));
    assert!(event(&mut p, &m, PopupKind::Overflow, false, x, y, 504).is_empty());
}

#[test]
fn large_tray_preserves_promoted_right_row_basis_without_clipping() {
    fn roots(n: &hyprv_ui::scene::Node) -> Vec<&hyprv_ui::scene::Node> {
        let mut out=if n.native_batch_root {vec![n]}else{vec![]};
        for child in &n.children {out.extend(roots(child));} out
    }
    let mut m=common::baseline();
    m.output.width=3072.;m.output.height=1728.;
    assert!(roots(&hyprv_ui_bar::build(&m,&m.theme)).is_empty());
    m.tray.items=(0..96).map(|i|TrayItem{key:format!("large-{i}"),id:format!("large-{i}"),..Default::default()}).collect();
    let layout=hyprv_ui_bar::layout::compute(&m,&m.theme);
    let tree=hyprv_ui_bar::build(&m,&m.theme);
    let scopes=roots(&tree);assert_eq!(scopes.len(),2);
    for scope in scopes {
        assert_eq!((scope.frame.x,scope.frame.y),(layout.pills[2].x,10.));
        assert!(scope.clip.is_none(),"batch roots must not add a painted-pixel clip");
    }
    let mut scene=hyprv_ui::scene::Scene::new(tree);
    let mut view=hyprv_ui_bar::view::RetainedView::default();
    view.update(&mut scene,&m, |_|{});
    assert_eq!(view.update(&mut scene,&m, |_|{}).edited_nodes,0);
    assert_eq!(roots(scene.root()).len(),2,"retained module placeholders must not demote the root");
    m.tray.items.truncate(6);
    view.update(&mut scene,&m, |_|{});
    assert_eq!(roots(scene.root()).len(),2,"QSG promotion persists for this transform-node lifetime");
}
