//! Offscreen PNG + parity diff tests for controls against widget-level QML references
//! captured from the ORIGINAL quickshell components (`qml-ref/`, see its README.md).
use super::testutil::{self, Img};
use super::*;
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

const THEMES: [(&str, bool); 2] = [("light", false), ("dark", true)];
const SCALES: [(&str, f64); 2] = [("1", 1.), ("2", 2.)];
/// Time far after every settle in the fixtures (animations finished).
const LATE: Duration = Duration::from_secs(10);

/// Diff `build(tok)` (placed on a `w×h` canvas) against `qml-ref/reference/<fixture>--<theme>@<s>.png`.
fn diff_fixture(fixture: &str, w: f32, h: f32, max_mean: f64, max_bad: f64, build: impl Fn(&Tok) -> Node) {
    for (theme, dark) in THEMES {
        let tok = Tok::new(dark);
        for (tag, scale) in SCALES {
            let reference = testutil::load(&testutil::widget_ref(&format!("{fixture}--{theme}@{tag}.png")));
            let mine = testutil::render(build(&tok), w, h, scale);
            testutil::check(
                &format!("{fixture}--{theme}@{tag}"),
                &mine,
                &reference,
                max_mean,
                max_bad,
            );
        }
    }
}

fn at<C: Control>(c: C, x: f32, y: f32, tok: &Tok) -> Node {
    let mut n = c.build(tok, LATE);
    n.frame.x += x;
    n.frame.y += y;
    n
}

fn hovered() -> Interaction {
    Interaction {
        hovered: true,
        pressed: None,
    }
}
fn pressed() -> Interaction {
    Interaction {
        hovered: true,
        pressed: Some(Button::Left),
    }
}

// ------------------------------------------------------------------ W7 ToggleTile

fn toggle(icon: ToggleIcon, active: bool, state: Interaction, enabled: bool) -> ToggleTile {
    let mut t = ToggleTile::new(icon);
    t.active = active;
    t.state = state;
    t.toggle_enabled = enabled;
    t
}
fn glyph(g: &str) -> ToggleIcon {
    ToggleIcon::Glyph(g.into())
}
fn light_dark_svg() -> ToggleIcon {
    let p = testutil::widget_ref("../../../../../../../quickshell/assets/control-panel/light-dark.svg");
    let bytes = std::fs::read(&p)
        .or_else(|_| std::fs::read(dirs_home().join(".config/HyprV/quickshell/assets/control-panel/light-dark.svg")))
        .expect("light-dark.svg");
    ToggleIcon::svg(&bytes).expect("svg parse")
}
fn dirs_home() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
}

/// Geometry/colour-dominated fixtures.
const T_MEAN: f64 = 3.0;
const T_BAD: f64 = 0.03;
/// Text-dominated fixtures: remaining error is glyph rasterisation (Qt FreeType hinting /
/// distance-field vs Skia), not placement — ink centroids agree within 0.05 px.
const TXT_MEAN: f64 = 4.5;
const TXT_BAD: f64 = 0.045;

#[test]
fn toggle_tiles_match_qml() {
    let cases: Vec<(&str, ToggleTile)> = vec![
        (
            "toggle-dnd-inactive",
            toggle(glyph("\u{f009b}"), false, Interaction::default(), true),
        ),
        (
            "toggle-dnd-active",
            toggle(glyph("\u{f009b}"), true, Interaction::default(), true),
        ),
        ("toggle-dnd-hover", toggle(glyph("\u{f009b}"), false, hovered(), true)),
        ("toggle-active-hover", toggle(glyph("\u{f009b}"), true, hovered(), true)),
        ("toggle-dnd-pressed", toggle(glyph("\u{f009b}"), false, pressed(), true)),
        (
            "toggle-vm-disabled",
            toggle(glyph("\u{f05b3}"), false, Interaction::default(), false),
        ),
        (
            "toggle-svg-active",
            toggle(light_dark_svg(), true, Interaction::default(), true),
        ),
        (
            "toggle-svg-inactive",
            toggle(light_dark_svg(), false, Interaction::default(), true),
        ),
    ];
    for (name, t) in cases {
        diff_fixture(name, 72., 72., T_MEAN, T_BAD, |tok| at(t.clone(), 5., 5., tok));
    }
    let mut labelled = toggle(glyph("\u{f0ec3}"), false, Interaction::default(), true);
    labelled.icon_only = false;
    labelled.label = "Screen\nrecord".into();
    labelled.frame = Rect::new(0., 0., 62., 72.);
    diff_fixture("toggle-labelled", 72., 82., TXT_MEAN, TXT_BAD, |tok| {
        at(labelled.clone(), 5., 5., tok)
    });
    testutil::assert_all();
}

// ------------------------------------------------------------------ W8 SplitTile

#[test]
fn split_tiles_match_qml() {
    let mk = |icon: &str, title: &str, sub: &str, active: bool| {
        let mut s = SplitTile::new(icon, title, sub);
        s.active = active;
        s
    };
    let mut chevron = mk("\u{f0002}", "Proxy", "Rule mode", true);
    chevron.expand_indicator_visible = true;
    chevron.expanded = true;
    let mut pressed_icon = mk("\u{f00af}", "Bluetooth", "Ready", false);
    pressed_icon.left = pressed();
    let cases = vec![
        ("split-wifi-active", mk("\u{f0928}", "WIFI", "HomeNet-5G", true)),
        ("split-bt-off", mk("\u{f00b2}", "Bluetooth", "Turned off", false)),
        ("split-proxy-chevron", chevron),
        (
            "split-long-elide",
            mk("\u{f0928}", "WIFI", "Connected to a very long SSID", false),
        ),
        ("split-icon-pressed", pressed_icon),
    ];
    for (name, s) in cases {
        diff_fixture(name, 144., 72., T_MEAN, T_BAD, |tok| at(s.clone(), 5., 5., tok));
    }
    testutil::assert_all();
}

// ------------------------------------------------------------------ W9 SliderTile

#[test]
fn sliders_match_qml() {
    let s = |icon: &str, v: f32| SliderTile::new(icon, v);
    let mut fan = s("\u{f0210}", 40.);
    fan.enabled = false;
    fan.opacity = 0.48;
    let mut quick = s("\u{f028}", 50.);
    quick.frame = Rect::new(0., 0., 278., 62.);
    quick.icon_left_inset = 24.;
    quick.icon_clickable = true;
    for (name, sl, w) in [
        ("slider-brightness-37", s("\u{f00df}", 37.), 144.),
        ("slider-brightness-0", s("\u{f00df}", 0.), 144.),
        ("slider-brightness-100", s("\u{f00df}", 100.), 144.),
        ("slider-fan-disabled", fan, 144.),
        ("slider-quick-volume", quick, 288.),
    ] {
        diff_fixture(name, w, 72., T_MEAN, T_BAD, |tok| at(sl.clone(), 5., 5., tok));
    }
    testutil::assert_all();
}

#[test]
fn slider_pointer_semantics() {
    let mut s = SliderTile::new("\u{f028}", 10.);
    s.icon_clickable = true;
    let now = Duration::ZERO;
    // Press on the icon → iconClicked, no adjustment.
    let r = s.pointer(
        PointerEvent::Press {
            x: 32.,
            y: 31.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(r.action, Some(SliderAction::IconClicked));
    s.pointer(
        PointerEvent::Release {
            x: 32.,
            y: 31.,
            button: Button::Left,
        },
        now,
    );
    assert!(!s.adjusting);
    // Press at 50 % → preview 50, drag to 75 %, release → finished 75.
    let r = s.pointer(
        PointerEvent::Press {
            x: 67.,
            y: 31.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(r.action, Some(SliderAction::ValueChangeRequested(50.)));
    let r = s.pointer(PointerEvent::Move { x: 100.5, y: 31. }, now);
    assert_eq!(r.action, Some(SliderAction::ValueChangeRequested(75.)));
    let r = s.pointer(
        PointerEvent::Release {
            x: 400.,
            y: 31.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(r.action, Some(SliderAction::AdjustmentFinished(100.)));
}

// ------------------------------------------------------------------ W5 ActionChip / W6 Badge

fn close_chip(tok: &Tok, state: Interaction) -> ActionChip {
    let p = tok.pal.primary_text;
    let mut c = ActionChip::label("Close");
    c.minimum_width = 76.;
    c.corner_radius = 9.;
    c.fill = Some(crate::foundations::with_alpha(p, if tok.dark() { 0.08 } else { 0.12 }));
    c.stroke = Some(tok.pal.card_stroke);
    c.state = state;
    c.at(5., 5., tok)
}

#[test]
fn action_chips_match_qml() {
    diff_fixture("chip-close", 150., 44., T_MEAN, T_BAD, |tok| {
        close_chip(tok, Interaction::default()).build(tok, LATE)
    });
    diff_fixture("chip-close-hover", 150., 44., T_MEAN, T_BAD, |tok| {
        close_chip(tok, hovered()).build(tok, LATE)
    });
    diff_fixture("chip-close-pressed", 150., 44., T_MEAN, T_BAD, |tok| {
        close_chip(tok, pressed()).build(tok, LATE)
    });
    diff_fixture("chip-scan-disabled", 150., 44., T_MEAN, T_BAD, |tok| {
        let mut c = ActionChip::label("Scan");
        c.minimum_width = 88.;
        c.corner_radius = 9.;
        c.disabled = true;
        c.fill = Some(tok.pal.card_fill);
        c.at(5., 5., tok).build(tok, LATE)
    });
    diff_fixture("chip-icon-connect", 150., 44., T_MEAN, T_BAD, |tok| {
        let mut c = ActionChip::icon("\u{f090}");
        c.minimum_width = 50.;
        c.corner_radius = 9.;
        c.fill = Some(tok.pal.card_fill);
        c.foreground = Some(tok.pal.accent);
        c.stroke = Some(crate::foundations::with_alpha(tok.pal.accent, 0.18));
        c.at(5., 5., tok).build(tok, LATE)
    });
    diff_fixture("chip-turnoff-accent", 150., 44., T_MEAN, T_BAD, |tok| {
        let mut c = ActionChip::label("Turn Off");
        c.minimum_width = 96.;
        c.corner_radius = 9.;
        c.fill = Some(tok.pal.card_strong_fill);
        c.foreground = Some(tok.pal.accent);
        c.stroke = Some(crate::foundations::with_alpha(tok.pal.accent, 0.18));
        c.at(5., 5., tok).build(tok, LATE)
    });
    diff_fixture("badge-lock", 32., 30., T_MEAN, T_BAD, |tok| {
        let mut b = Badge::security(true);
        b.corner_radius = 9.;
        at(b, 5., 5., tok)
    });
    testutil::assert_all();
}

// ------------------------------------------------------------------ W1 TextModule

fn ws(label: &str) -> TextModule {
    TextModule {
        label: label.into(),
        module_height: 32.,
        padding_left: 5.,
        padding_right: 5.,
        minimum_width: 32.,
        ..Default::default()
    }
}
fn sized(mut m: TextModule, tok: &Tok) -> TextModule {
    let (w, h) = m.implicit_size(tok);
    m.frame = Rect::new(5., 5., w, h);
    m
}

#[test]
fn text_modules_match_qml() {
    diff_fixture("tm-workspace-active", 110., 48., T_MEAN, T_BAD, |tok| {
        let mut m = ws("1");
        m.highlighted = true;
        m.text_color = Some(tok.pal.muted_workspace_text);
        sized(m, tok).build(tok, LATE)
    });
    diff_fixture("tm-workspace-urgent", 110., 48., T_MEAN, T_BAD, |tok| {
        let mut m = ws("3");
        m.highlighted = true;
        m.highlight_color = Some(crate::foundations::rgb("#a6e3a1"));
        m.highlighted_text_color = Some(crate::foundations::rgb("#11111b"));
        m.text_color = Some(tok.pal.muted_workspace_text);
        sized(m, tok).build(tok, LATE)
    });
    diff_fixture("tm-workspace-hover", 110., 48., T_MEAN, T_BAD, |tok| {
        let mut m = ws("2");
        m.hoverable = true;
        m.interactive = true;
        m.state = hovered();
        m.text_color = Some(tok.pal.muted_workspace_text);
        sized(m, tok).build(tok, LATE)
    });
    let module = |label: &str, px: f32, pl: f32, pr: f32, color: Option<fn(&Tok) -> hyprv_ui::animation::Color>| {
        let label = label.to_owned();
        move |tok: &Tok| {
            let m = TextModule {
                label: label.clone(),
                font_pixel_size: px,
                padding_left: pl,
                padding_right: pr,
                interactive: true,
                text_color: color.map(|f| f(tok)),
                ..Default::default()
            };
            sized(m, tok).build(tok, LATE)
        }
    };
    diff_fixture(
        "tm-launcher",
        110.,
        48.,
        T_MEAN,
        T_BAD,
        module("\u{f303}", 17., 7., 4., Some(|t: &Tok| t.pal.accent)),
    );
    diff_fixture(
        "tm-cpu",
        110.,
        48.,
        T_MEAN,
        T_BAD,
        module("\u{f2db} 7%", 16., 12., 4., None),
    );
    diff_fixture(
        "tm-volume",
        110.,
        48.,
        T_MEAN,
        T_BAD,
        module("\u{f028}", 16., 8., 12., Some(|t: &Tok| t.pal.accent)),
    );
    testutil::assert_all();
}

#[test]
fn text_module_signals() {
    let mut m = TextModule {
        wheel_interactive: true,
        interactive: true,
        ..ws("1")
    };
    m.frame = Rect::new(0., 0., 32., 32.);
    let now = Duration::ZERO;
    m.pointer(
        PointerEvent::Press {
            x: 5.,
            y: 5.,
            button: Button::Right,
        },
        now,
    );
    let r = m.pointer(
        PointerEvent::Release {
            x: 5.,
            y: 5.,
            button: Button::Right,
        },
        now,
    );
    assert_eq!(r.action, Some(TextModuleAction::RightClicked));
    let r = m.pointer(
        PointerEvent::Wheel {
            angle_x: 0.,
            angle_y: -120.,
            pixel_x: 0.,
            pixel_y: 0.,
        },
        now,
    );
    assert_eq!(r.action, Some(TextModuleAction::WheelDown));
    // Release outside cancels the click (QML MouseArea).
    m.pointer(
        PointerEvent::Press {
            x: 5.,
            y: 5.,
            button: Button::Left,
        },
        now,
    );
    let r = m.pointer(
        PointerEvent::Release {
            x: 50.,
            y: 5.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(r.action, None);
}

// ------------------------------------------------------------------ W2 PillButton (Header)

fn header(label: &str, emphasized: bool, tok: &Tok) -> PillButton {
    let mut b = PillButton::header(label, emphasized);
    b.update(tok, Duration::ZERO);
    let (w, h) = b.implicit_size(tok);
    b.frame = Rect::new(5., 5., w, h);
    b
}

#[test]
fn header_buttons_match_qml() {
    diff_fixture("hb-create", 250., 44., T_MEAN, T_BAD, |tok| {
        header("＋ 创建日程", true, tok).build(tok, LATE)
    });
    diff_fixture("hb-range", 250., 44., T_MEAN, T_BAD, |tok| {
        header("2026/10/19 – 25  ·  09:41", false, tok).build(tok, LATE)
    });
    diff_fixture("hb-prev-hover", 250., 44., T_MEAN, T_BAD, |tok| {
        let mut b = header("‹", false, tok);
        b.pointer(PointerEvent::Enter { x: 18., y: 17. }, Duration::ZERO);
        b.build(tok, LATE)
    });
    diff_fixture("hb-next-pressed", 250., 44., T_MEAN, T_BAD, |tok| {
        let mut b = header("›", false, tok);
        b.pointer(PointerEvent::Enter { x: 18., y: 17. }, Duration::ZERO);
        b.pointer(
            PointerEvent::Press {
                x: 18.,
                y: 17.,
                button: Button::Left,
            },
            Duration::from_millis(20),
        );
        b.build(tok, LATE)
    });
    testutil::assert_all();
}

#[test]
fn pill_behaviors_animate_like_qml() {
    let tok = Tok::new(false);
    let mut b = PillButton::island("\u{f023}", 11., 12.);
    b.update(&tok, Duration::ZERO);
    assert!(!b.animating(Duration::ZERO), "Behaviors do not run on creation");
    b.pointer(PointerEvent::Enter { x: 5., y: 5. }, Duration::from_millis(100));
    assert!(b.animating(Duration::from_millis(150)));
    assert!(!b.animating(Duration::from_millis(220)), "hover fade is 120 ms");
    b.pointer(
        PointerEvent::Press {
            x: 5.,
            y: 5.,
            button: Button::Left,
        },
        Duration::from_millis(300),
    );
    let r = b.pointer(
        PointerEvent::Release {
            x: 5.,
            y: 5.,
            button: Button::Left,
        },
        Duration::from_millis(360),
    );
    assert_eq!(r.action, Some(()));
    assert!(b.animating(Duration::from_millis(400)));
}

// ------------------------------------------------------------------ W4 media buttons (crop of the media card)

#[test]
fn media_buttons_match_qml_card_crop() {
    // Card 134×134 at (5,5): controls row width 125 at x = 5 + 4.5, y = 5 + 100, h 34.
    for (fixture, available, playing) in [
        ("media-idle", false, false),
        ("media-playing", true, true),
        ("media-paused", true, false),
    ] {
        for (theme, dark) in THEMES {
            let tok = Tok::new(dark);
            for (tag, scale) in SCALES {
                let reference = testutil::load(&testutil::widget_ref(&format!("{fixture}--{theme}@{tag}.png")));
                let s = scale as usize;
                let crop = |img: &Img| img.crop(9 * s, 105 * s, 126 * s, 34 * s);
                // The card's fill sits under the buttons: reproduce it in the crop.
                let mut group = Node::new(Rect::new(0., 0., 144., 144.), hyprv_ui::scene::Content::Group);
                group.add(kit::qt_rect(
                    Rect::new(5., 5., 134., 134.),
                    19.,
                    tok.pal.card_strong_fill,
                    Some((1., tok.pal.card_stroke)),
                ));
                let mut x = 5. + 4.5;
                let glyphs = [
                    ("\u{f04ae}", false),
                    (if playing { "\u{f03e4}" } else { "\u{f040a}" }, true),
                    ("\u{f04ad}", false),
                ];
                for (g, primary) in glyphs {
                    let mut b = GlyphButton::media(g, primary);
                    b.enabled = available;
                    b.frame.x = x;
                    b.frame.y = 5. + 100.;
                    x += b.frame.width + 8.;
                    group.add(b.build(&tok, LATE));
                }
                let mine = testutil::render(group, 144., 144., scale);
                testutil::check(
                    &format!("{fixture}-buttons--{theme}@{tag}"),
                    &crop(&mine),
                    &crop(&reference),
                    T_MEAN,
                    T_BAD,
                );
            }
        }
    }
    testutil::assert_all();
}

// ------------------------------------------------------------------ W10 Switch (crop of the Bluetooth header)

#[test]
fn switch_matches_qml_bluetooth_header_crop() {
    // Header row at (15,15) h40: title "Bluetooth" 17 px, spacing 12, switch 48×28 vcentred.
    let title_w = kit::measure("Bluetooth", 17., kit::BOLD).width;
    let sx = 15. + title_w + 12.;
    let sy = 15. + kit::vcenter(40., 28.);
    for (fixture, on) in [("bt-header-on", true), ("bt-header-off", false)] {
        for (theme, dark) in THEMES {
            let tok = Tok::new(dark);
            for (tag, scale) in SCALES {
                let reference = testutil::load(&testutil::widget_ref(&format!("{fixture}--{theme}@{tag}.png")));
                let s = scale as f32;
                let (cx, cy) = ((sx - 2.).floor(), (sy - 2.).floor());
                let crop = |img: &Img| {
                    img.crop(
                        (cx * s) as usize,
                        (cy * s) as usize,
                        (52. * s) as usize,
                        (32. * s) as usize,
                    )
                };
                let mut sw = Switch::new(on);
                sw.update(&tok, Duration::ZERO);
                sw.frame.x = sx;
                sw.frame.y = sy;
                let mine = testutil::render(sw.build(&tok, LATE), 394., 250., scale);
                testutil::check(
                    &format!("{fixture}-switch--{theme}@{tag}"),
                    &crop(&mine),
                    &crop(&reference),
                    T_MEAN,
                    T_BAD,
                );
            }
        }
    }
    testutil::assert_all();
}

#[test]
fn switch_knob_animates_160ms_inoutquad() {
    let tok = Tok::new(false);
    let mut sw = Switch::new(false);
    sw.on = true;
    sw.update(&tok, Duration::from_millis(1000));
    assert!(sw.animating(Duration::from_millis(1100)));
    assert!(!sw.animating(Duration::from_millis(1161)));
    // InOutQuad at t=0.5 → 0.5: knob halfway between 3 and 23.
    let mid = sw.build(&tok, Duration::from_millis(1080));
    let knob = &mid.children[1];
    assert!((knob.frame.x - 13.).abs() < 0.01, "{}", knob.frame.x);
}

// ------------------------------------------------------------------ W12 DialogButton

#[test]
fn dialog_buttons_match_qml() {
    let mk = |text: &str, dark: bool, state: Interaction, focus: bool| {
        let mut b = DialogButton::new(text, dark);
        b.frame = Rect::new(5., 5., 88., 32.);
        b.state = state;
        b.visual_focus = focus;
        b
    };
    for (fixture, text, state, focus) in [
        ("db-skip", "本次跳过", Interaction::default(), false),
        ("db-launch-hover", "启动", hovered(), false),
        ("db-launch-pressed", "启动", pressed(), false),
    ] {
        diff_fixture(fixture, 98., 42., TXT_MEAN, TXT_BAD, |tok| {
            mk(text, tok.dark(), state, focus).build(tok, LATE)
        });
    }
    // Focus ring (spec-only: the harness window never becomes active, so QML references
    // cannot show `visualFocus`): 2 px accent ring at −3 px, radius 11.
    let tok = Tok::new(false);
    let ring = mk("本次跳过", false, Interaction::default(), true);
    let img = testutil::render(ring.build(&tok, LATE), 98., 42., 2.);
    let px = |x: usize, y: usize| &img.px[(y * img.w + x) * 4..(y * img.w + x) * 4 + 4];
    assert_eq!(
        px(2 * 3, 2 * 20),
        &[0x1e, 0x66, 0xf5, 255],
        "ring at x = 5 − 3 + 1 (light accent #1e66f5)"
    );
    testutil::snapshot("db-skip-focus--light@2", &img);
    let mut b = DialogButton::new("启动", false);
    b.keyboard_ready = false;
    assert_eq!(
        b.key_press(DialogKey::Return, false),
        None,
        "keys swallowed before ready"
    );
    b.keyboard_ready = true;
    assert_eq!(b.key_press(DialogKey::Space, true), None, "auto-repeat ignored");
    assert_eq!(b.key_press(DialogKey::Enter, false), Some(()));
    testutil::assert_all();
}

// ------------------------------------------------------------------ spec-only snapshots (no QML reference)

#[test]
fn spec_only_snapshots_render() {
    for (theme, dark) in THEMES {
        let tok = Tok::new(dark);
        let mut row = Node::new(Rect::new(0., 0., 300., 260.), hyprv_ui::scene::Content::Group);
        let mut y = 4.;
        let rows = vec![
            MenuRow::new(MenuRowKind::Back, 282.),
            MenuRow::new(
                MenuRowKind::Entry {
                    text: "Show window".into(),
                    enabled: true,
                    indicator: MenuIndicator::None,
                    icon: None,
                    has_children: false,
                },
                282.,
            ),
            MenuRow::new(MenuRowKind::Separator, 282.),
            MenuRow::new(
                MenuRowKind::Entry {
                    text: "Do not disturb".into(),
                    enabled: true,
                    indicator: MenuIndicator::Checkbox(true),
                    icon: None,
                    has_children: true,
                },
                282.,
            ),
            MenuRow::new(
                MenuRowKind::Entry {
                    text: "Disabled entry".into(),
                    enabled: false,
                    indicator: MenuIndicator::Radio(false),
                    icon: None,
                    has_children: false,
                },
                282.,
            ),
        ];
        for (i, mut r) in rows.into_iter().enumerate() {
            r.frame.x = 9.;
            r.frame.y = y;
            if i == 1 {
                r.state = hovered();
            }
            y += r.frame.height + 1.;
            row.add(r.build(&tok, LATE));
        }
        let mut seg = SegmentedRow::mihomo_modes(282.);
        seg.selected = Some("global".into());
        seg.frame.x = 9.;
        seg.frame.y = y + 8.;
        row.add(seg.build(&tok, LATE));
        let mut tray = TrayButton::new("discord");
        tray.frame.x = 9.;
        tray.frame.y = y + 50.;
        row.add(tray.build(&tok, LATE));
        let mut notif = GlyphButton::notification(false, true);
        notif.frame.x = 40.;
        notif.frame.y = y + 50.;
        row.add(notif.build(&tok, LATE));
        let mut more = GlyphButton::tray_overflow();
        more.frame.x = 70.;
        more.frame.y = y + 50.;
        row.add(more.build(&tok, LATE));
        let mut circle = CircleGlyphButton::new("\u{f040a}", true);
        circle.frame.x = 100.;
        circle.frame.y = y + 47.;
        row.add(circle.build(&tok, LATE));
        let img = testutil::render(row, 300., 260., 2.);
        assert!(img.px.chunks(4).any(|p| p[3] > 0));
        testutil::snapshot(&format!("spec-only-controls--{theme}@2"), &img);
    }
}

#[test]
fn tray_button_actions() {
    let mut t = TrayButton::new("nm-applet");
    t.has_menu = true;
    let now = Duration::ZERO;
    t.pointer(
        PointerEvent::Press {
            x: 4.,
            y: 4.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(
        t.pointer(
            PointerEvent::Release {
                x: 4.,
                y: 4.,
                button: Button::Left
            },
            now
        )
        .action,
        Some(TrayButtonAction::Activate)
    );
    t.only_menu = true;
    t.pointer(
        PointerEvent::Press {
            x: 4.,
            y: 4.,
            button: Button::Left,
        },
        now,
    );
    assert_eq!(
        t.pointer(
            PointerEvent::Release {
                x: 4.,
                y: 4.,
                button: Button::Left
            },
            now
        )
        .action,
        Some(TrayButtonAction::OpenMenu)
    );
    let r = t.pointer(
        PointerEvent::Wheel {
            angle_x: 120.,
            angle_y: 0.,
            pixel_x: 0.,
            pixel_y: 0.,
        },
        now,
    );
    assert_eq!(
        r.action,
        Some(TrayButtonAction::Scroll(-1)),
        "horizontal-only wheel sends −1 (QML quirk)"
    );
    assert_eq!(TrayButton::new("").fallback_letter(), "?");
}

#[test]
fn disabled_drag_finishes_once_and_cannot_resume() {
    for cancel in [false, true] {
        let mut s = SliderTile::new("", 30.);
        assert!(matches!(
            s.pointer(
                PointerEvent::Press {
                    x: 40.,
                    y: 30.,
                    button: Button::Left
                },
                Duration::ZERO
            )
            .action,
            Some(SliderAction::ValueChangeRequested(_))
        ));
        s.enabled = false;
        let event = if cancel {
            PointerEvent::Cancel
        } else {
            PointerEvent::Release {
                x: 100.,
                y: 30.,
                button: Button::Left,
            }
        };
        assert!(matches!(
            s.pointer(event, Duration::ZERO).action,
            Some(SliderAction::AdjustmentFinished(_))
        ));
        assert!(!s.adjusting);
        assert_eq!(s.state.pressed, None);
        assert_eq!(s.pointer(event, Duration::ZERO).action, None);
        s.enabled = true;
        assert_eq!(
            s.pointer(PointerEvent::Move { x: 110., y: 30. }, Duration::ZERO).action,
            None
        );
        assert!(!s.adjusting);
    }
}

#[test]
fn switch_initial_binding_snaps_then_real_changes_animate() {
    for dark in [false, true] {
        for on in [false, true] {
            let mut s = Switch::new(on);
            assert!(!s.animating(Duration::ZERO));
            s.update(&Tok::new(dark), Duration::ZERO);
            assert!(!s.animating(Duration::ZERO));
            s.update(&Tok::new(!dark), Duration::from_secs(1));
            assert!(s.animating(Duration::from_secs(1)));
            assert!(!s.animating(Duration::from_secs(2)));
            s.on = !on;
            s.update(&Tok::new(!dark), Duration::from_secs(2));
            assert!(s.animating(Duration::from_secs(2)));
        }
    }
}

#[test]
fn disabled_shared_controls_forget_the_previous_press() {
    let press = PointerEvent::Press {
        x: 12.,
        y: 12.,
        button: Button::Left,
    };
    let release = PointerEvent::Release {
        x: 12.,
        y: 12.,
        button: Button::Left,
    };
    let mut s = Switch::new(false);
    s.pointer(press, Duration::ZERO);
    s.enabled = false;
    s.pointer(release, Duration::ZERO);
    s.enabled = true;
    assert_eq!(s.pointer(release, Duration::ZERO).action, None);
    let mut s = SegmentedRow::mihomo_modes(200.);
    s.pointer(press, Duration::ZERO);
    s.enabled = false;
    s.pointer(PointerEvent::Cancel, Duration::ZERO);
    s.enabled = true;
    assert_eq!(s.pointer(release, Duration::ZERO).action, None);
}

#[test]
fn disabled_binding_cancels_even_if_no_pointer_event_arrives_while_disabled() {
    let mut slider = SliderTile::new("", 50.);
    let now = Duration::ZERO;
    let tok = Tok::new(false);
    slider.pointer(
        PointerEvent::Press {
            x: 40.,
            y: 30.,
            button: Button::Left,
        },
        now,
    );
    slider.enabled = false;
    slider.update(&tok, now);
    assert!(!slider.adjusting);
    assert_eq!(slider.state.pressed, None);
    slider.enabled = true;
    slider.update(&tok, now);
    assert!(matches!(
        slider.take_canceled_adjustment(),
        Some(SliderAction::AdjustmentFinished(_))
    ));
    assert_eq!(slider.take_canceled_adjustment(), None);
    assert_eq!(slider.pointer(PointerEvent::Move { x: 120., y: 30. }, now).action, None);
    let mut switch = Switch::new(false);
    switch.pointer(
        PointerEvent::Press {
            x: 10.,
            y: 10.,
            button: Button::Left,
        },
        now,
    );
    switch.enabled = false;
    switch.update(&tok, now);
    switch.enabled = true;
    switch.update(&tok, now);
    assert_eq!(
        switch
            .pointer(
                PointerEvent::Release {
                    x: 10.,
                    y: 10.,
                    button: Button::Left
                },
                now
            )
            .action,
        None
    );
}

// ------------------------------------------------------------------ WIDGETS-1/2 follow-ups (same patterns elsewhere)

#[test]
fn header_pill_first_theme_snaps_then_theme_changes_animate() {
    for dark in [false, true] {
        let mut b = PillButton::header("‹", false);
        assert!(!b.animating(Duration::ZERO));
        b.pointer(PointerEvent::Enter { x: 5., y: 5. }, Duration::ZERO); // before any theme
        b.update(&Tok::new(dark), Duration::from_millis(500));
        assert!(
            !b.animating(Duration::from_millis(500)),
            "first theme must snap (dark={dark})"
        );
        b.update(&Tok::new(!dark), Duration::from_secs(1));
        assert!(b.animating(Duration::from_secs(1)), "real theme change animates");
    }
}

#[test]
fn text_module_and_split_tile_forget_press_when_disabled() {
    let now = Duration::ZERO;
    let tok = Tok::new(false);
    let press = PointerEvent::Press {
        x: 5.,
        y: 5.,
        button: Button::Left,
    };
    let release = PointerEvent::Release {
        x: 5.,
        y: 5.,
        button: Button::Left,
    };
    let mut m = TextModule {
        interactive: true,
        ..ws("1")
    };
    m.frame = Rect::new(0., 0., 32., 32.);
    m.pointer(press, now);
    m.interactive = false;
    m.update(&tok, now);
    m.pointer(release, now);
    m.interactive = true;
    m.pointer(press, now);
    assert_eq!(
        m.pointer(release, now).action,
        Some(TextModuleAction::LeftClicked),
        "fresh gesture works"
    );
    m.pointer(press, now);
    m.visible = false;
    m.pointer(release, now);
    m.visible = true;
    assert_eq!(m.pointer(release, now).action, None, "no stale click after hide");

    let mut t = SplitTile::new("x", "t", "s");
    t.pointer(press, now);
    t.left_enabled = false;
    t.update(&tok, now);
    t.left_enabled = true;
    assert_eq!(t.pointer(release, now).action, None, "stale left press must not click");
}
