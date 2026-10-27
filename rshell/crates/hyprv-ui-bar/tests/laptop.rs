//! Laptop profile bar behaviour (docs/spec/laptop-bar.md). Desktop stays the default.
mod common;
use common::baseline;
use hyprv_ui_bar::{
    input::{Input, PopupCommand},
    ipc::{self, ToggleGate},
    layout,
    model::*,
    motion::Motion,
    surfaces, tooltip,
};
use hyprv_widgets::controls::{Button, PointerEvent};
use std::time::Duration;

fn laptop() -> Model {
    let mut m = baseline();
    m.profile = Profile::Laptop;
    m.output.width = 1440.;
    m.output.height = 900.;
    m.expanded = false;
    m.stats.package_power_w = None;
    m
}
fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

#[test]
fn desktop_defaults_are_unchanged() {
    let mut m = baseline();
    assert_eq!(m.profile, Profile::Desktop);
    assert!(m.bar_toggle_is_noop() && m.battery_popup_never_opens());
    ipc::toggle(&mut m);
    assert!(!m.expanded, "desktop toggleBar is a no-op");
    let mut motion = Motion::new(&m);
    motion.advance(&mut m, ms(10));
    assert_eq!(m.reveal, 1.);
    assert!(surfaces::corner_input(&m).is_empty());
    assert!(!m.compact_strip_visible());
    let l = layout::compute(&m, &m.theme);
    assert!(l.target(&Target::Viewflow).is_some());
    assert!(l.target(&Target::CpuPower).is_none());
}

#[test]
fn laptop_starts_compact_and_animates_reveal_with_overshoot() {
    let mut m = laptop();
    let mut motion = Motion::new(&m);
    motion.advance(&mut m, ms(0));
    assert_eq!(m.reveal, 0., "QUIRK_LAPTOP_BAR_STARTS_COMPACT: initial value without animation");
    assert_eq!(surfaces::bar(&m).exclusive_zone, 25);
    assert_eq!(surfaces::bar(&m).layer, surfaces::Layer::Overlay);
    assert_eq!(m.top_corner_offset(), 15.);
    ipc::toggle(&mut m);
    assert!(m.expanded);
    motion.sync(&m, ms(1000));
    let mut peak = 0f64;
    for t in (1000..=1700).step_by(16) {
        motion.advance(&mut m, ms(t));
        peak = peak.max(m.reveal);
    }
    motion.advance(&mut m, ms(1700));
    assert!(peak > 1., "bezier [0.1,0.9,0.1,1.05] overshoots");
    assert!((m.reveal - 1.).abs() < 1e-9);
    assert!(!motion.animating());
    assert_eq!(surfaces::bar(&m).exclusive_zone, 58);
    assert_eq!(surfaces::bar(&m).layer, surfaces::Layer::Top);
    assert!(surfaces::corner_input(&m).is_empty());
}

#[test]
fn toggle_gate_is_global_500ms() {
    let mut gate = ToggleGate::default();
    assert!(gate.allow(10_000));
    assert!(!gate.allow(10_300), "a second toggle within 500 ms is dropped (any output)");
    assert!(gate.allow(10_600));
    assert!(!gate.allow(10_000), "clock going backwards is inside the window");
}

#[test]
fn compact_strip_mask_and_clicks() {
    let mut m = laptop();
    m.reveal = 0.;
    let mask = surfaces::corner_input(&m);
    assert_eq!(mask.len(), 1);
    assert_eq!((mask[0].width, mask[0].height), (1440., 15.));
    let strip = surfaces::compact_strip(&m);
    assert_eq!(strip.frame.y, 0.);
    let find = |t: Target| strip.items.iter().find(|(x, _)| *x == t).map(|(_, r)| *r).unwrap();
    let mut input = Input::default();
    let mut click = |m: &mut Model, r: hyprv_ui::geometry::Rect| {
        let (x, y) = (r.x + r.width / 2., r.y + 7.);
        input.strip_pointer(m, PointerEvent::Press { x, y, button: Button::Left });
        input.strip_pointer(m, PointerEvent::Release { x, y, button: Button::Left })
    };
    let r = click(&mut m, find(Target::StripClock));
    assert!(matches!(r.actions[..], [Action::Host(HostAction::ToggleBar), Action::Host(HostAction::ToggleCalendar)]));
    // QUIRK_STRIP_BATTERY_CLICK_UNGUARDED: opens the battery popup even without a battery.
    m.power.battery_present = false;
    let r = click(&mut m, find(Target::Battery));
    assert!(matches!(r.popup, Some(PopupCommand::Battery { .. })));
    let r = click(&mut m, find(Target::Workspace(2)));
    assert!(matches!(r.actions[..], [Action::Desktop(DesktopAction::WorkspaceActivate(2))]));
    // Full mode: strip hidden and click-through.
    m.reveal = 1.;
    assert!(surfaces::corner_input(&m).is_empty());
    let r = click(&mut m, find(Target::StripClock));
    assert!(r.actions.is_empty());
}

#[test]
fn laptop_right_pill_and_battery_modes() {
    let mut m = laptop();
    m.reveal = 1.;
    let l = layout::compute(&m, &m.theme);
    assert!(l.target(&Target::Viewflow).is_none(), "QUIRK_LAPTOP_NO_VIEWFLOW_BUTTON");
    assert!(l.target(&Target::CpuPower).is_none(), "hidden without RAPL");
    assert!(l.target(&Target::Temperature).is_none());
    m.stats.package_power_w = Some(64.);
    let l = layout::compute(&m, &m.theme);
    let module = l.modules.iter().find(|n| n.target == Target::CpuPower).unwrap();
    assert_eq!(module.label, "64.0 W");
    assert!(!module.interactive);
    // UPower "idle" (Unknown + OnBattery) is unplugged: level glyph, can be critical.
    m.power.battery_present = true;
    m.power.battery_percent = Some(20.);
    m.power.battery_mode = BatteryMode::Idle;
    assert!(m.battery_on_battery());
    assert_eq!(hyprv_ui_bar::format::power_label(&m), format!("{} 20%", hyprv_ui_bar::format::battery_glyph(20.)));
    m.power.battery_mode = BatteryMode::Full;
    assert!(!m.battery_on_battery());
}

#[test]
fn cpu_tooltip_after_350ms_hover() {
    let mut m = laptop();
    m.reveal = 1.;
    m.stats.package_power_w = Some(12.3);
    m.stats.temperature_c = Some(82.);
    let r = layout::compute(&m, &m.theme).target(&Target::CpuPower).unwrap();
    let mut input = Input::default();
    m.now = ms(100);
    let res = input.pointer(&mut m, PointerEvent::Move { x: r.x + 5., y: r.y + 5. });
    assert_eq!(m.hovered, Some(Target::CpuPower));
    assert_eq!(res.cursor, Some(hyprv_widgets::controls::Cursor::Default));
    assert_eq!(input.next_deadline(), Some(ms(450)));
    assert!(!input.tooltip(&mut m, ms(449)));
    assert!(tooltip::rect(&m).is_none());
    assert!(input.tooltip(&mut m, ms(450)));
    let rect = tooltip::rect(&m).unwrap();
    assert_eq!(rect.y, r.y + r.height + 9.);
    input.pointer(&mut m, PointerEvent::Leave);
    assert!(tooltip::rect(&m).is_none());
}

#[test]
fn laptop_network_segment_yields_to_a_widened_island() {
    let mut m = laptop();
    m.expanded = true;
    let narrow = layout::compute(&m, &m.theme);
    assert!(narrow.target(&Target::Network).is_some());
    let stats_end = |l: &layout::Layout| l.target(&Target::Memory).map(|r| r.x + r.width).unwrap();
    m.island.width = 1440. - 2. * (stats_end(&narrow) - 20.);
    let wide = layout::compute(&m, &m.theme);
    assert!(wide.target(&Target::Network).is_none(), "network hides when the island would cover it");
    assert!(wide.target(&Target::Memory).is_some() && wide.target(&Target::Cpu).is_some());
    assert_eq!(wide.title.width, 0., "window title pill hides with the network stat");
    let mut d = baseline();
    d.island.width = m.island.width;
    assert!(layout::compute(&d, &d.theme).target(&Target::Network).is_some(), "desktop unchanged");
}
