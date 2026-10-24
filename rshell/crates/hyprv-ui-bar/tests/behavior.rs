mod common;
use common::baseline;
use hyprv_ui::{geometry::Rect, scene::Scene};
use hyprv_ui_bar::{
    input::{Input, PopupCommand},
    layout,
    model::*,
    motion::Motion,
    popups::{Kind, Popups},
    surfaces, *,
};
use hyprv_widgets::controls::{Button, PointerEvent};
use std::time::Duration;
fn click(
    input: &mut Input,
    m: &mut Model,
    target: Target,
    button: Button,
) -> hyprv_ui_bar::input::InputResult {
    let l = layout::compute(m, &m.theme);
    let r = l.target(&target).unwrap();
    let (x, y) = (r.x + r.width / 2., r.y + r.height / 2.);
    input.pointer(m, PointerEvent::Press { x, y, button });
    input.pointer(m, PointerEvent::Release { x, y, button })
}
#[test]
fn metrics_and_adaptive_layout() {
    let m = baseline();
    let l = layout::compute(&m, &m.theme);
    assert!(l.supplementary);
    assert!(
        (l.threshold - 1347.75).abs() < 4.,
        "threshold={}, primitive metrics changed",
        l.threshold
    );
    assert_eq!(l.target(&Target::ControlPanel).unwrap().width, 28.);
    let mut narrow = m.clone();
    narrow.output.width = 1280.;
    let l = layout::compute(&narrow, &narrow.theme);
    assert!(!l.supplementary);
    assert!(l.target(&Target::Volume).is_none());
    assert!(l.target(&Target::Temperature).is_none());
    assert!(l
        .modules
        .iter()
        .all(|n| n.frame.width.is_finite() && n.frame.width >= 0.));
}
#[test]
fn tray_geometry_exhaustive_budget_and_stable_filter() {
    for total in 0..50 {
        for budget in [0., 10., 22., 40., 100., 300., 1000.] {
            let g = layout::tray_geometry(total, budget);
            assert_eq!(g.visible + g.overflow, total);
            assert!(g.spacing >= 0.);
            if total == 0 {
                assert_eq!(g.container_width, 0.);
            } else {
                assert!(g.container_width >= 20.);
            }
        }
    }
    let mut m = baseline();
    m.tray.items[0].id = "nm-applet".into();
    m.tray.items[1].id = "wechat".into();
    m.tray.items[2].id = "discord".into();
    let sorted = layout::sorted_tray(&m);
    assert_eq!(sorted[0].id, "discord");
    assert!(!sorted.iter().any(|i| i.id == "nm-applet"));
}
#[test]
fn equal_model_has_no_scene_edit_or_damage() {
    let m = baseline();
    let mut s = Scene::new(build(&m, &m.theme));
    s.take_damage(Rect::new(0., 0., m.output.width, 58.));
    let revision = s.revision();
    assert_eq!(update(&mut s, &m), 0);
    assert_eq!(s.revision(), revision);
    assert!(!s.is_dirty());
    let mut changed = m.clone();
    changed.stats.cpu_percent = Some(8.);
    assert!(update(&mut s, &changed) > 0);
    assert!(s.is_dirty());
}
#[test]
fn fullscreen_retargers_and_stationary_callbacks_stop() {
    let mut m = baseline();
    let mut motion = Motion::new(&m);
    m.fullscreen = true;
    motion.sync(&m, Duration::ZERO);
    motion.advance(&mut m, Duration::from_millis(100));
    assert!(m.fullscreen_hide > 0.);
    assert_eq!(surfaces::bar(&m).exclusive_zone, 58);
    assert_eq!(surfaces::bar(&m).layer, surfaces::Layer::Overlay);
    let previous = m.fullscreen_hide;
    m.fullscreen = false;
    motion.sync(&m, Duration::from_millis(100));
    motion.advance(&mut m, Duration::from_millis(100));
    assert!((m.fullscreen_hide - previous).abs() < 1e-9);
    motion.advance(&mut m, Duration::from_millis(800));
    assert_eq!(m.fullscreen_hide, 0.);
    assert!(!motion.animating());
    assert!(motion.next_deadline().is_none());
    assert_eq!(surfaces::bar(&m).layer, surfaces::Layer::Top);
}
#[test]
fn calendar_morph_close_snap_and_input_move() {
    let mut m = baseline();
    let mut motion = Motion::new(&m);
    m.calendar_open = true;
    motion.sync(&m, Duration::ZERO);
    motion.advance(&mut m, Duration::from_millis(210));
    assert!(m.calendar_presented);
    assert_eq!(m.calendar_width, 520.);
    assert_eq!(m.calendar_height, 500.);
    m.calendar_open = false;
    motion.sync(&m, Duration::from_millis(300));
    motion.advance(&mut m, Duration::from_millis(509));
    assert!(m.calendar_presented);
    motion.advance(&mut m, Duration::from_millis(510));
    assert!(!m.calendar_presented);
    assert_eq!(m.calendar_width, m.island.width);
    m.fullscreen_hide = 1.;
    let l = layout::compute(&m, &m.theme);
    assert!(l.input_regions[1].y < 0.);
}
#[test]
fn typed_actions_inverted_wheel_and_press_cancel() {
    let mut m = baseline();
    let mut input = Input::default();
    let result = click(&mut input, &mut m, Target::Workspace(2), Button::Left);
    assert!(matches!(
        result.actions.as_slice(),
        [Action::Desktop(DesktopAction::WorkspaceActivate(2))]
    ));
    let r = layout::compute(&m, &m.theme)
        .target(&Target::Volume)
        .unwrap();
    input.pointer(
        &mut m,
        PointerEvent::Move {
            x: r.x + 2.,
            y: r.y + 2.,
        },
    );
    let result = input.pointer(
        &mut m,
        PointerEvent::Wheel {
            angle_x: 0.,
            angle_y: 120.,
            pixel_x: 0.,
            pixel_y: 0.,
        },
    );
    assert!(matches!(
        result.actions.as_slice(),
        [Action::Audio(AudioAction::Decrease)]
    ));
    assert!(input.advance(Duration::from_millis(79)).is_empty());
    assert!(matches!(
        input.advance(Duration::from_millis(80)).as_slice(),
        [Action::Host(HostAction::ShowVolume)]
    ));
    let r = layout::compute(&m, &m.theme)
        .target(&Target::Launcher)
        .unwrap();
    input.pointer(
        &mut m,
        PointerEvent::Press {
            x: r.x + 1.,
            y: r.y + 1.,
            button: Button::Left,
        },
    );
    assert!(input
        .pointer(
            &mut m,
            PointerEvent::Release {
                x: 0.,
                y: 100.,
                button: Button::Left
            }
        )
        .actions
        .is_empty());
}
#[test]
fn calendar_dismiss_layer_blocks_bar_actions() {
    let mut m = baseline();
    m.calendar_open = true;
    m.calendar_presented = true;
    m.calendar_progress = 1.;
    m.calendar_width = 520.;
    m.calendar_height = 500.;
    let mut input = Input::default();
    let result = click(&mut input, &mut m, Target::Launcher, Button::Left);
    assert!(matches!(
        result.actions.as_slice(),
        [Action::Host(HostAction::CloseCalendar { force: false })]
    ));
}
#[test]
fn horizontal_tray_wheel_quirk() {
    let mut m = baseline();
    let mut input = Input::default();
    let r = layout::compute(&m, &m.theme)
        .target(&Target::Tray("tray-0".into()))
        .unwrap();
    input.pointer(
        &mut m,
        PointerEvent::Move {
            x: r.x + 1.,
            y: r.y + 1.,
        },
    );
    let result = input.pointer(
        &mut m,
        PointerEvent::Wheel {
            angle_x: 120.,
            angle_y: 0.,
            pixel_x: 0.,
            pixel_y: 0.,
        },
    );
    assert!(matches!(
        result.actions.as_slice(),
        [Action::Tray(TrayAction::Scroll {
            delta: -1,
            horizontal: false,
            ..
        })]
    ));
}
#[test]
fn popup_hydration_close_and_reopen_cleanup() {
    let mut m = baseline();
    m.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
        item_key: "tray-0".into(),
        root: TrayMenuEntry {
            children: common::menu_entries(),
            ..Default::default()
        },
        revision: 1,
    }];
    let mut p = Popups::default();
    let command = || PopupCommand::Menu {
        item: "tray-0".into(),
        source: Target::Tray("tray-0".into()),
        anchor: Rect::new(200., 10., 18., 38.),
    };
    assert!(matches!(
        p.command(&m, command(), Duration::ZERO).as_slice(),
        [Action::Tray(TrayAction::MenuOpen { .. })]
    ));
    p.update(&m, Duration::ZERO);
    assert_eq!(p.menu.surface.reveal.values().height, 0.);
    p.update(&m, Duration::from_millis(16));
    p.update(&m, Duration::from_millis(216));
    assert_eq!(p.menu.surface.reveal.values().opacity, 1.);
    p.menu.stack.push(6);
    p.escape(Kind::Menu, Duration::from_millis(300));
    assert!(p.menu.stack.is_empty());
    assert!(p.menu.surface.requested);
    p.escape(Kind::Menu, Duration::from_millis(400));
    let actions = p.update(&m, Duration::from_millis(600));
    assert!(matches!(
        actions.as_slice(),
        [Action::Tray(TrayAction::MenuClose { .. })]
    ));
    assert!(!p.menu.surface.mapped);
    p.command(&m, command(), Duration::from_millis(650));
    p.update(&m, Duration::from_millis(800));
    assert_eq!(p.menu.item.as_deref(), Some("tray-0"));
}
#[test]
fn overflow_dimensions_and_battery_strings() {
    assert_eq!(Popups::overflow_size(1), (34., 54.));
    assert_eq!(Popups::overflow_size(6), (159., 54.));
    assert_eq!(Popups::overflow_size(7), (159., 92.));
    assert_eq!(hyprv_ui_bar::format::human_rate(1024.), "1.0kB/s");
    assert_eq!(hyprv_ui_bar::format::duration(3660.), "1小时1分钟");
    assert_eq!(hyprv_ui_bar::format::watts(Some(-4.23)), "-4.23 W");
}
#[test]
fn surfaces_corners_and_ipc_contract() {
    let mut m = baseline();
    assert_eq!(surfaces::bar(&m).size, (3072, 58));
    assert_eq!(surfaces::bar(&m).keyboard, surfaces::Keyboard::None);
    assert_eq!(surfaces::corners(&m).len(), 1);
    for (r, node) in surfaces::corners(&m) {
        assert!(r.input_passthrough);
        assert_eq!(r.size, (0, 0));
        assert_eq!(
            r.namespace,
            format!("hyprv-screen-corner-overlay-{}", m.output.name)
        );
        assert_eq!(
            r.anchors,
            surfaces::Anchors {
                top: true,
                bottom: true,
                left: true,
                right: true
            }
        );
        assert_eq!(r.layer, surfaces::Layer::Overlay);
        assert_eq!(r.keyboard, surfaces::Keyboard::None);
        assert!(r.mapped && r.input_regions.is_empty());
        assert_eq!(node.children.len(), 4);
        for (child, (x, y)) in node.children.iter().zip([
            (0., 0.),
            (m.output.width - 27., 0.),
            (0., m.output.height - 27.),
            (m.output.width - 27., m.output.height - 27.),
        ]) {
            assert_eq!(child.frame, hyprv_ui::geometry::Rect::new(x, y, 27., 27.));
        }
        assert_eq!(r.exclusive_zone, -1);
    }
    let before = m.reveal;
    hyprv_ui_bar::ipc::toggle(&mut m);
    assert_eq!(m.reveal, before);
    let value: serde_json::Value =
        serde_json::from_str(&hyprv_ui_bar::ipc::layout_status(Some(&m))).unwrap();
    assert_eq!(value["expanded"], true);
    assert_eq!(hyprv_ui_bar::ipc::layout_status(None), "null");
}

#[test]
fn independent_popup_outputs_and_resize_follow_their_sources() {
    let m = baseline();
    let mut other = m.clone();
    other.output.name = "HDMI-A-1".into();
    other.output.width = 1280.;
    let mut p = Popups::default();
    p.command(
        &m,
        PopupCommand::Menu {
            item: "tray-0".into(),
            anchor: Rect::new(200., 10., 18., 38.),
            source: Target::Tray("tray-0".into()),
        },
        Duration::ZERO,
    );
    p.command(
        &other,
        PopupCommand::Battery {
            anchor: Rect::new(1000., 10., 70., 38.),
            source: Target::Battery,
        },
        Duration::ZERO,
    );
    let requirements = p.requirements(&other);
    assert_eq!(requirements[0].1.output, "DP-4");
    assert_eq!(requirements[0].1.size.0, 3072);
    assert_eq!(requirements[2].1.output, "HDMI-A-1");
    other.output.width = 1600.;
    p.update(&other, Duration::ZERO);
    assert_eq!(p.requirements(&m)[2].1.size.0, 1600);
}
#[test]
fn calendar_arm_child_release_and_initial_fullscreen() {
    let mut m = baseline();
    m.fullscreen = true;
    m.calendar_open = true;
    let mut c = Controller::new(m);
    assert_eq!(c.model.fullscreen_hide, 1.);
    assert!(c.model.calendar_presented);
    assert!(!c.model.calendar_pointer_armed);
    c.motion.advance(&mut c.model, Duration::from_millis(260));
    assert!(c.model.calendar_pointer_armed);
    c.motion
        .pointer_calendar(false, &c.model, Duration::from_millis(260));
    assert!(c
        .motion
        .advance(&mut c.model, Duration::from_millis(479))
        .is_empty());
    assert!(matches!(
        c.motion
            .advance(&mut c.model, Duration::from_millis(480))
            .as_slice(),
        [Action::Host(HostAction::CloseCalendar { force: false })]
    ));
}

#[test]
fn svg_absolute_viewport_is_fitted_into_requested_icon() {
    use hyprv_ui::{
        render::Renderer,
        scene::{Content, Node},
        skia,
    };
    let picture=hyprv_ui_bar::view::svg_picture("<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200' viewBox='0 0 200 200'><rect x='100' y='100' width='100' height='100' fill='red'/></svg>",16.,16.).unwrap();
    let scene = Scene::new(Node::new(
        Rect::new(0., 0., 16., 16.),
        Content::Picture(picture),
    ));
    let mut surface = Renderer::new().raster(&scene, 16., 16., 1.).unwrap();
    let pixels = surface.peek_pixels().unwrap();
    assert_eq!(pixels.get_color((12, 12)), skia::Color::RED);
    assert_eq!(pixels.get_color((4, 4)), skia::Color::TRANSPARENT);
}
#[test]
fn strip_adapter_leaves_motion_and_center_host_to_shell() {
    let mut m = baseline();
    m.fullscreen_hide = 1.;
    m.calendar_presented = true;
    m.calendar_progress = 1.;
    m.calendar_width = 520.;
    m.calendar_height = 500.;
    let tree = hyprv_ui_bar::view::build_strip(&m, &m.theme);
    assert_eq!(tree.frame.height, 58.);
    assert_eq!(tree.children[0].frame.y, 0.);
    assert_eq!(tree.children[0].children.len(), 4);
}
#[test]
fn unavailable_power_keeps_last_severity_and_disabled_menu_closes() {
    let mut m = baseline();
    m.stats.package_power_w = Some(95.);
    let mut c = Controller::new(m);
    let mut next = c.model.clone();
    next.stats.package_power_w = None;
    c.sync(next, Duration::ZERO);
    let l = layout::compute(&c.model, &c.model.theme);
    let module = l
        .modules
        .iter()
        .find(|v| v.target == Target::Battery)
        .unwrap();
    assert_eq!(module.label, "-- W");
    assert_eq!(
        module.color,
        hyprv_widgets::foundations::Palette::new(false).critical
    );
    let mut m = baseline();
    m.tray.menus = vec![hyprv_svc_media::tray::TrayMenu {
        item_key: "tray-0".into(),
        root: TrayMenuEntry {
            children: common::menu_entries(),
            ..Default::default()
        },
        revision: 1,
    }];
    let mut p = Popups::default();
    p.command(
        &m,
        PopupCommand::Menu {
            item: "tray-0".into(),
            anchor: Rect::new(200., 10., 18., 38.),
            source: Target::Tray("tray-0".into()),
        },
        Duration::ZERO,
    );
    p.update(&m, Duration::ZERO);
    p.update(&m, Duration::from_millis(16));
    p.update(&m, Duration::from_millis(216));
    let panel = p.card_rect(&m, Kind::Menu);
    let (x, y) = (panel.x + 20., panel.y + 9. + 3. * 35. + 2. + 10.);
    p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Press {
            x,
            y,
            button: Button::Left,
        },
        Duration::from_millis(300),
    );
    let result = p.pointer(
        &m,
        Kind::Menu,
        PointerEvent::Release {
            x,
            y,
            button: Button::Left,
        },
        Duration::from_millis(301),
    );
    assert!(result.actions.is_empty());
    assert!(!p.menu.surface.requested);
}

#[test]
fn shared_slot_identity_does_not_sustain_idle_frame_callbacks() {
    use hyprv_ui::scene::{Content, Node};
    use std::rc::Rc;
    let extent = Rect::new(0., 0., 40., 20.);
    let shared = Rc::new(Node::new(extent, Content::Group));
    let root = |tree| Node::new(extent, Content::Shared(tree));
    let mut scene = Scene::new(root(shared.clone()));
    scene.take_damage(extent);
    let id = scene.root().id();
    let revision = scene.revision();
    assert_eq!(
        hyprv_ui_bar::view::reconcile(&mut scene, id, root(shared.clone())),
        0
    );
    assert_eq!(scene.revision(), revision);
    assert!(
        !scene.is_dirty(),
        "unchanged shared slot must let frame callbacks stop"
    );

    let mut changed = Node::new(extent, Content::Group);
    changed.opacity = 0.5;
    assert!(hyprv_ui_bar::view::reconcile(&mut scene, id, root(Rc::new(changed))) > 0);
    assert!(
        scene.is_dirty(),
        "a new shared tree must still invalidate the slot"
    );
}

#[test]
fn corner_overlay_tracks_output_and_legacy_offsets() {
    let mut m = baseline();
    for (width, height, scale) in [(1280., 720., 1.), (1920., 1080., 1.5)] {
        m.output.width = width;
        m.output.height = height;
        m.output.scale = scale;
        for (reveal, hide, y) in [
            (0., 0., 15.),
            (0.5, 0.5, 3.75),
            (0., 1., 0.),
            (1.2, 0., 0.),
            (-0.2, 0., 15.),
        ] {
            m.reveal = reveal;
            m.fullscreen_hide = hide;
            let overlays = surfaces::corners(&m);
            assert_eq!(overlays.len(), 1);
            let (_, node) = &overlays[0];
            assert_eq!(
                node.frame,
                hyprv_ui::geometry::Rect::new(0., 0., width, height)
            );
            assert_eq!(node.children[0].frame.y, y);
            assert_eq!(
                node.children[1].frame,
                hyprv_ui::geometry::Rect::new(width - 27., y, 27., 27.)
            );
            assert_eq!(
                node.children[3].frame,
                hyprv_ui::geometry::Rect::new(width - 27., height - 27., 27., 27.)
            );
        }
    }
}

#[test]
fn control_svg_source_size_tracks_real_output_dpr() {
    use hyprv_ui::scene::{Content,Node};
    fn contains(n:&Node,width:i32)->bool {
        matches!(&n.content,Content::Image(i) if i.width()==width && i.height()==width)
            || n.children.iter().any(|c|contains(c,width))
    }
    for scale in [1.,1.25,1.5,1.75,2.,2.5,3.,4.] {
        let mut m=baseline();m.output.scale=scale;
        let key=if m.theme.dark {"control-panel-dark"}else{"control-panel-light"};
        assert_eq!(hyprv_ui_bar::baked::image(key,16,scale,"source").unwrap().width(),(32.*scale).round() as i32);
        assert!(contains(&hyprv_ui_bar::build(&m,&m.theme),(32.*scale).round() as i32));
    }
}
