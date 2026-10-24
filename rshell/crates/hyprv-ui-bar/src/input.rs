use crate::{
    layout::{self, Layout},
    model::*,
};
use hyprv_ui::geometry::Rect;
use hyprv_widgets::controls::{Button, Cursor, PointerEvent};
use std::time::Duration;
#[derive(Clone, Debug)]
pub enum PopupCommand {
    Menu {
        item: String,
        anchor: Rect,
        source: Target,
    },
    Overflow {
        anchor: Rect,
        source: Target,
    },
    Battery {
        anchor: Rect,
        source: Target,
    },
}
#[derive(Default)]
pub struct InputResult {
    pub actions: Vec<Action>,
    pub popup: Option<PopupCommand>,
    pub redraw: bool,
    pub cursor: Option<Cursor>,
    pub consumed: bool,
}
#[derive(Default)]
pub struct Input {
    pressed: Option<(Target, Button)>,
    hovered: Option<Target>,
    delayed: Vec<(Duration, Action)>,
    /// Laptop CPU-power tooltip: hover start, restarted on every re-enter (§4).
    tooltip_since: Option<Duration>,
    /// Laptop compact strip press (corner-window MouseAreas, left button only).
    strip_pressed: Option<Target>,
}
impl Input {
    pub fn pointer(&mut self, m: &mut Model, event: PointerEvent) -> InputResult {
        let l = layout::compute(m, &m.theme);
        let mut result = InputResult::default();
        match event {
            PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => {
                let target = if m.calendar_open || inside_center(&l, x, y) {
                    None
                } else {
                    l.hit(x, y)
                };
                if self.hovered != target {
                    result.redraw = true;
                    self.hovered = target.clone();
                    m.hovered = target.clone();
                    self.tooltip_since = (target == Some(Target::CpuPower)).then_some(m.now);
                    m.tooltip_visible = false;
                }
                result.cursor = Some(if target.as_ref().is_some_and(|t| *t != Target::CpuPower) {
                    Cursor::Pointer
                } else {
                    Cursor::Default
                });
                result.consumed = l.input_regions.iter().any(|r| layout::contains(*r, x, y));
            }
            PointerEvent::Leave => {
                result.redraw = self.hovered.take().is_some();
                m.hovered = None;
                self.tooltip_since = None;
                m.tooltip_visible = false;
            }
            PointerEvent::Cancel => {
                self.pressed = None;
            }
            PointerEvent::Press { x, y, button } => {
                self.pressed = if m.calendar_open || inside_center(&l, x, y) {
                    None
                } else {
                    l.hit(x, y).map(|t| (t, button))
                };
                result.consumed = l.input_regions.iter().any(|r| layout::contains(*r, x, y));
            }
            PointerEvent::Release { x, y, button } => {
                let target = if m.calendar_open || inside_center(&l, x, y) {
                    None
                } else {
                    l.hit(x, y)
                };
                if let Some((pressed, original)) = self.pressed.take() {
                    if original == button && target.as_ref() == Some(&pressed) {
                        self.click(m, &l, pressed, button, &mut result);
                    }
                } else if m.calendar_open
                    && !layout::contains(
                        Rect::new(
                            l.center.x,
                            l.center.y + l.content_y,
                            l.center.width,
                            l.center.height,
                        ),
                        x,
                        y,
                    )
                    && button == Button::Left
                {
                    result
                        .actions
                        .push(Action::Host(HostAction::CloseCalendar { force: false }));
                }
                result.consumed = l.input_regions.iter().any(|r| layout::contains(*r, x, y));
            }
            PointerEvent::Wheel {
                angle_y,
                pixel_y,
                angle_x,
                pixel_x,
            } => {
                let y = if angle_y != 0. { angle_y } else { pixel_y };
                match self.hovered.as_ref() {
                    Some(Target::Volume) if y != 0. => {
                        let up = y > 0.;
                        result
                            .actions
                            .push(Action::Audio(if up == QUIRK_INVERT_VOLUME_WHEEL {
                                AudioAction::Decrease
                            } else {
                                AudioAction::Increase
                            }));
                        self.show_volume_later(m.now);
                        result.consumed = true;
                    }
                    Some(Target::Tray(item)) if y != 0. || angle_x != 0. || pixel_x != 0. => {
                        let delta = if y > 0. {
                            1
                        } else if y < 0. || QUIRK_HORIZONTAL_TRAY_WHEEL_SENDS_MINUS_ONE {
                            -1
                        } else {
                            0
                        };
                        if delta != 0 {
                            result.actions.push(Action::Tray(TrayAction::Scroll {
                                item: item.clone(),
                                delta,
                                horizontal: false,
                            }));
                        }
                        result.consumed = true;
                    }
                    _ => (),
                }
            }
        }
        result
    }
    fn show_volume_later(&mut self, now: Duration) {
        self.delayed
            .retain(|(_, a)| !matches!(a, Action::Host(HostAction::ShowVolume)));
        self.delayed.push((
            now + Duration::from_millis(80),
            Action::Host(HostAction::ShowVolume),
        ));
    }
    fn click(
        &mut self,
        m: &mut Model,
        l: &Layout,
        target: Target,
        button: Button,
        r: &mut InputResult,
    ) {
        let anchor = l.target(&target).unwrap_or_default();
        let host = |a| Action::Host(a);
        match (target.clone(), button) {
            (Target::Launcher, Button::Left) => r.actions.push(host(HostAction::Launcher)),
            (Target::Viewflow, Button::Left) => r.actions.push(host(HostAction::ViewflowStart {
                global_x: (m.output.global_x + anchor.x).round() as i32,
                global_y: (m.output.global_y + anchor.y + anchor.height + 8.).round() as i32,
            })),
            (Target::Viewflow, Button::Right) => r.actions.push(host(HostAction::ViewflowMenu)),
            (Target::Workspace(id), Button::Left)
                if m.desktop.workspaces.iter().any(|w| w.id == id) =>
            {
                r.actions
                    .push(Action::Desktop(DesktopAction::WorkspaceActivate(id)));
            }
            (Target::Cpu | Target::Memory | Target::Network, Button::Left) => {
                r.actions.push(host(HostAction::ToggleStats { anchor }))
            }
            (Target::Cpu | Target::Memory, Button::Right) | (Target::Temperature, Button::Left) => {
                r.actions.push(host(HostAction::OpenSystemMonitor))
            }
            (Target::Battery, Button::Left) if m.battery_available() => {
                r.popup = Some(PopupCommand::Battery {
                    anchor,
                    source: target,
                })
            }
            (Target::Previous, Button::Left) => {
                r.actions.push(Action::Media(MprisAction::Previous))
            }
            (Target::Next, Button::Left) => r.actions.push(Action::Media(MprisAction::Next)),
            (Target::PlayPause, Button::Left) => {
                m.media.playing = !m.media.playing;
                r.actions.push(Action::Media(MprisAction::PlayPause));
                r.redraw = true;
            }
            (Target::Volume, Button::Left) => {
                r.actions.push(Action::Audio(AudioAction::ToggleMute));
                self.show_volume_later(m.now);
            }
            (Target::Volume, Button::Right) => r.actions.push(host(HostAction::OpenAudioMixer)),
            (Target::Wifi, Button::Left) => r.actions.push(host(HostAction::ToggleWifi { anchor })),
            (Target::ControlPanel, Button::Left) => r
                .actions
                .push(host(HostAction::ToggleControlPanel { anchor })),
            (Target::Notifications, Button::Left) => r
                .actions
                .push(Action::Notification(NotificationAction::ToggleVisibility)),
            (Target::Notifications, Button::Right) => r
                .actions
                .push(Action::Notification(NotificationAction::ToggleDnd)),
            (Target::Overflow, Button::Left) => {
                r.popup = Some(PopupCommand::Overflow {
                    anchor,
                    source: target,
                })
            }
            (Target::Tray(key), Button::Left | Button::Right) => {
                if let Some(item) = m.tray.items.iter().find(|i| i.key == key) {
                    if (button == Button::Right || item.only_menu) && item.has_menu {
                        r.popup = Some(PopupCommand::Menu {
                            item: key,
                            anchor,
                            source: target,
                        });
                    } else if button == Button::Left && !item.only_menu {
                        r.actions.push(Action::Tray(TrayAction::Activate {
                            item: key,
                            x: (m.output.global_x + anchor.x).round() as i32,
                            y: (m.output.global_y + anchor.y + anchor.height).round() as i32,
                        }));
                    } else if button == Button::Right {
                        r.actions.push(Action::Tray(TrayAction::SecondaryActivate {
                            item: key,
                            x: (m.output.global_x + anchor.x).round() as i32,
                            y: (m.output.global_y + anchor.y + anchor.height).round() as i32,
                        }));
                    }
                }
            }
            _ => (),
        }
    }
    pub fn advance(&mut self, now: Duration) -> Vec<Action> {
        let mut due = Vec::new();
        let mut later = Vec::new();
        for (deadline, action) in self.delayed.drain(..) {
            if now >= deadline {
                due.push(action);
            } else {
                later.push((deadline, action));
            }
        }
        self.delayed = later;
        due
    }
    /// Laptop compact strip in the corner window (laptop-bar.md §2). Coordinates are
    /// corner-window (output) logical pixels. Clicks are left-button MouseArea clicks.
    pub fn strip_pointer(&mut self, m: &mut Model, event: PointerEvent) -> InputResult {
        let mut r = InputResult::default();
        if !m.compact_strip_visible() {
            self.strip_pressed = None;
            return r;
        }
        let strip = crate::surfaces::compact_strip(m);
        let hit = |x: f32, y: f32| {
            strip
                .items
                .iter()
                .rev()
                .find(|(_, rect)| layout::contains(*rect, x, y))
                .map(|(t, rect)| (t.clone(), *rect))
        };
        let inside = |x: f32, y: f32| layout::contains(crate::surfaces::corner_input(m).first().copied().unwrap_or_default(), x, y);
        match event {
            PointerEvent::Enter { x, y } | PointerEvent::Move { x, y } => {
                r.consumed = inside(x, y);
                r.cursor = Some(Cursor::Default);
            }
            PointerEvent::Press { x, y, button } => {
                r.consumed = inside(x, y);
                self.strip_pressed = (button == Button::Left).then(|| hit(x, y).map(|h| h.0)).flatten();
            }
            PointerEvent::Release { x, y, button } => {
                r.consumed = inside(x, y);
                let pressed = self.strip_pressed.take();
                if let (Some(pressed), Some((target, anchor)), Button::Left) = (pressed, hit(x, y), button) {
                    if pressed == target {
                        self.strip_click(m, target, anchor, &mut r);
                    }
                }
            }
            PointerEvent::Leave | PointerEvent::Cancel => self.strip_pressed = None,
            PointerEvent::Wheel { .. } => {}
        }
        r
    }
    fn strip_click(&mut self, m: &mut Model, target: Target, anchor: Rect, r: &mut InputResult) {
        let host = |a| Action::Host(a);
        match target {
            // The fallback {id:1} entry has no activate(): the QML click throws, a no-op.
            Target::Workspace(id) if m.desktop.workspaces.iter().any(|w| w.id == id) => r
                .actions
                .push(Action::Desktop(DesktopAction::WorkspaceActivate(id))),
            Target::Cpu => r.actions.push(host(HostAction::ToggleStats { anchor })),
            Target::StripClock => {
                // QUIRK_STRIP_CALENDAR_DEBOUNCE_GAP: the calendar toggles even when the
                // host's debounce drops this ToggleBar.
                if !m.expanded {
                    r.actions.push(host(HostAction::ToggleBar));
                }
                r.actions.push(host(HostAction::ToggleCalendar));
            }
            Target::Battery if QUIRK_STRIP_BATTERY_CLICK_UNGUARDED || m.battery_available() => {
                r.popup = Some(PopupCommand::Battery { anchor, source: Target::Battery })
            }
            Target::Wifi | Target::ControlPanel => {
                r.actions.push(host(HostAction::ToggleControlPanel { anchor }))
            }
            Target::Notifications => r
                .actions
                .push(Action::Notification(NotificationAction::ToggleVisibility)),
            _ => {}
        }
    }
    /// Show the hover tooltip once `tooltip::DELAY` has elapsed.
    pub fn tooltip(&mut self, m: &mut Model, now: Duration) -> bool {
        let due = self
            .tooltip_since
            .is_some_and(|since| now >= since + crate::tooltip::DELAY);
        if due {
            self.tooltip_since = None;
            m.tooltip_visible = true;
        }
        due
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.delayed
            .iter()
            .map(|(t, _)| *t)
            .chain(self.tooltip_since.map(|t| t + crate::tooltip::DELAY))
            .min()
    }
}

fn inside_center(l: &Layout, x: f32, y: f32) -> bool {
    layout::contains(
        Rect::new(
            l.center.x,
            l.center.y + l.content_y,
            l.center.width,
            l.center.height,
        ),
        x,
        y,
    )
}
