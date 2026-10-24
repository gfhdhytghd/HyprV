//! Offscreen PNG + parity tests for visualisations: widget-level QML references
//! (`controls/qml-ref/reference`) and crops of the island references (`parity/reference/island`).
use super::*;
use crate::controls::kit;
use crate::controls::testutil::{self, Img};
use crate::controls::{Button, CircleGlyphButton, Control, PillButton, PointerEvent, Tok};
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

const THEMES: [(&str, bool); 2] = [("light", false), ("dark", true)];
const SCALES: [(&str, f64); 2] = [("1", 1.), ("2", 2.)];
const LATE: Duration = Duration::from_secs(10);
const G_MEAN: f64 = 3.0;
const G_BAD: f64 = 0.03;
const TXT_MEAN: f64 = 4.5;
const TXT_BAD: f64 = 0.045;

fn placed<C: Control>(c: &C, tok: &Tok, x: f32, y: f32) -> Node {
    let mut n = c.build(tok, LATE);
    n.frame.x += x;
    n.frame.y += y;
    n
}

fn diff_widget(fixture: &str, w: f32, h: f32, max_mean: f64, max_bad: f64, build: impl Fn(&Tok) -> Node) {
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

/// Crop `[x,y,w,h]` (logical) of an island reference vs our rendering of `build` on the
/// 640-wide island canvas (`canvas_h` = the reference height).
fn diff_island(state: &str, canvas_h: f32, crop: [f32; 4], max_mean: f64, max_bad: f64, build: impl Fn(&Tok) -> Node) {
    for (theme, dark) in THEMES {
        let tok = Tok::new(dark);
        for (tag, scale) in SCALES {
            let reference = testutil::load(&testutil::island_ref(&format!("{state}--{theme}@{tag}.png")));
            let mine = testutil::render(build(&tok), 640., canvas_h, scale);
            let s = scale as f32;
            let c = |img: &Img| {
                img.crop(
                    (crop[0] * s) as usize,
                    (crop[1] * s) as usize,
                    (crop[2] * s) as usize,
                    (crop[3] * s) as usize,
                )
            };
            testutil::check(
                &format!(
                    "island:{state}[{}]--{theme}@{tag}",
                    crop.map(|v| v as i32).map(|v| v.to_string()).join(",")
                ),
                &c(&mine),
                &c(&reference),
                max_mean,
                max_bad,
            );
        }
    }
}

/// The island frame interior under widgets: `surfaceColor` (moduleBackground), no border.
fn island_bg(tok: &Tok, r: Rect) -> Node {
    kit::qt_rect(r, 0., tok.pal.module_background, None)
}

// ------------------------------------------------------------------ V3 spectrum

const VALUES: [f32; 16] = [
    12., 38., 74., 92., 66., 41., 58., 80., 95., 61., 33., 47., 70., 52., 28., 15.,
];

#[test]
fn spectrum_matches_qml() {
    let with = |mut s: SpectrumBars, active: bool| {
        s.values = VALUES.to_vec();
        s.active = active;
        s
    };
    diff_widget("spectrum-compact-playing", 32., 32., G_MEAN, G_BAD, |tok| {
        placed(&with(SpectrumBars::island_compact(), true), tok, 5., 5.)
    });
    diff_widget("spectrum-expanded-playing", 80., 66., G_MEAN, G_BAD, |tok| {
        placed(&with(SpectrumBars::island_expanded(), true), tok, 5., 5.)
    });
    diff_widget("spectrum-compact-paused-phase0", 32., 32., G_MEAN, G_BAD, |tok| {
        placed(&with(SpectrumBars::island_compact(), false), tok, 5., 5.)
    });
    testutil::assert_all();
}

#[test]
fn spectrum_tick_contract() {
    let mut s = SpectrumBars::island_compact();
    s.values = VALUES.to_vec();
    s.active = true;
    assert!(!s.wants_tick(), "playing with data: no idle wave timer");
    s.active = false;
    assert!(s.wants_tick());
    s.tick();
    assert!((s.phase - 0.28).abs() < 1e-6);
    s.visible = false;
    assert!(!s.wants_tick(), "never tick while hidden");
}

// ------------------------------------------------------------------ V4 trend chart

fn series(f: impl Fn(f64) -> f64) -> Vec<f64> {
    (0..120).map(|i| (f(i as f64) * 100.).round() / 100.).collect()
}

#[test]
fn trend_charts_match_qml() {
    let cpu = series(|i| 35. + 25. * (i / 7.).sin() + 10. * (i / 2.3).sin());
    let mem = series(|i| 68. + 3. * (i / 15.).sin());
    let net: Vec<f64> = (0..120)
        .map(|i| {
            let i = i as f64;
            let v = 20000. + 180000. * (i / 9.).sin().max(0.).powi(3) + if (i as i64) % 17 == 0 { 50000. } else { 0. };
            (v.max(0.) * 10.).round() / 10.
        })
        .collect();
    let chart = |w: f32, h: f32, samples: &[f64], auto: bool, accent: fn(&Tok) -> hyprv_ui::animation::Color| {
        let samples = samples.to_vec();
        move |tok: &Tok| {
            let mut c = TrendChart::new(w, h);
            c.samples = samples.clone();
            c.auto_scale = auto;
            c.accent = Some(accent(tok));
            placed(&c, tok, 5., 5.)
        }
    };
    diff_widget(
        "chart-cpu-120",
        352.,
        162.,
        G_MEAN,
        G_BAD,
        chart(342., 152., &cpu, false, |t| t.pal.severity(45.)),
    );
    diff_widget(
        "chart-memory-120",
        526.,
        122.,
        G_MEAN,
        G_BAD,
        chart(516., 112., &mem, false, |t| t.pal.severity(70.)),
    );
    diff_widget(
        "chart-network-auto",
        526.,
        122.,
        G_MEAN,
        G_BAD,
        chart(516., 112., &net, true, |t| t.pal.accent),
    );
    diff_widget(
        "chart-one-sample",
        352.,
        162.,
        G_MEAN,
        G_BAD,
        chart(342., 152., &[42.], false, |t| t.pal.severity(45.)),
    );
    diff_widget(
        "chart-empty",
        352.,
        162.,
        G_MEAN,
        G_BAD,
        chart(342., 152., &[], false, |t| t.pal.severity(45.)),
    );
    testutil::assert_all();
}

#[test]
fn trend_chart_autoscale() {
    let mut c = TrendChart::new(100., 50.);
    c.auto_scale = true;
    c.samples = vec![0., 200., 50.];
    assert!((c.resolved_max() - 230.).abs() < 1e-9);
    c.samples.clear();
    assert_eq!(c.resolved_max(), 1.);
}

// ------------------------------------------------------------------ V5 corners

#[test]
fn corners_match_qml() {
    for (name, corner) in [
        ("corner-topLeft", Corner::TopLeft),
        ("corner-topRight", Corner::TopRight),
        ("corner-bottomLeft", Corner::BottomLeft),
        ("corner-bottomRight", Corner::BottomRight),
    ] {
        diff_widget(name, 27., 27., 2.0, 0.02, |tok| {
            InverseCorner::shell(corner).build(tok, LATE)
        });
    }
    testutil::assert_all();
}

// ------------------------------------------------------------------ V6 calendar body

fn calendar_fixture() -> TimelineGrid {
    let mut g = TimelineGrid::new(520., 500.);
    // Buffer starts Mon 2026-10-12 (weekStart Oct 19 − 7); today = Fri Oct 23 = index 11.
    let wd = ["一", "二", "三", "四", "五", "六", "日"];
    let dates = [
        12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 1,
    ];
    g.days = (0..21)
        .map(|i| DayLabel {
            weekday: wd[i % 7].into(),
            date: dates[i],
            today: i == 11,
        })
        .collect();
    let home = Some(crate::foundations::rgb("#e07a5f"));
    let work = Some(crate::foundations::rgb("#4f86c6"));
    g.all_day = vec![
        AllDayInput {
            start_day: 9,
            end_day: 12,
            title: "Trip to Kyoto".into(),
            color: home,
        },
        AllDayInput {
            start_day: 11,
            end_day: 12,
            title: "Release day".into(),
            color: work,
        },
    ];
    let m = |day: f64, h: f64, min: f64| day * 1440. + h * 60. + min;
    g.timed = vec![
        TimedInput {
            start_min: m(11., 9., 0.),
            end_min: m(11., 9., 30.),
            title: "Standup".into(),
            start_label: "09:00".into(),
            color: work,
        },
        TimedInput {
            start_min: m(11., 10., 0.),
            end_min: m(11., 11., 30.),
            title: "Design review".into(),
            start_label: "10:00".into(),
            color: work,
        },
        TimedInput {
            start_min: m(11., 10., 30.),
            end_min: m(11., 11., 0.),
            title: "Dentist".into(),
            start_label: "10:30".into(),
            color: home,
        },
        TimedInput {
            start_min: m(8., 8., 0.),
            end_min: m(8., 8., 20.),
            title: "1:1".into(),
            start_label: "08:00".into(),
            color: work,
        },
    ];
    g.content_x = g.initial_content_x();
    g.content_y = 0.; // the harness never triggers positionNearNow (see ui-panels-calendar.md §2.4)
    g.now = Some((11, 9. * 60. + 41.));
    g
}

#[test]
fn calendar_body_matches_qml() {
    for (theme, dark) in THEMES {
        let tok = Tok::new(dark);
        for (tag, scale) in SCALES {
            let reference = testutil::load(&testutil::widget_ref(&format!("calendar-week--{theme}@{tag}.png")));
            let mine = testutil::render(calendar_fixture().build(&tok, LATE), 520., 500., scale);
            let s = scale as usize;
            let crop = |img: &Img| img.crop(0, 57 * s, 520 * s, 443 * s);
            testutil::check(
                &format!("calendar-body--{theme}@{tag}"),
                &crop(&mine),
                &crop(&reference),
                TXT_MEAN,
                TXT_BAD,
            );
        }
    }
    testutil::assert_all();
}

#[test]
fn calendar_layout_rules() {
    let g = calendar_fixture();
    let (items, lanes) = layout_all_day(&g.all_day);
    assert_eq!(lanes, 2);
    assert_eq!((items[0].start_day, items[0].span, items[0].lane), (9, 3, 0));
    assert_eq!((items[1].start_day, items[1].span, items[1].lane), (11, 1, 1));
    assert_eq!(g.all_day_height(), 60.);
    let timed = layout_timed(&g.timed);
    let review = timed.iter().find(|t| t.input == 1).unwrap();
    let dentist = timed.iter().find(|t| t.input == 2).unwrap();
    assert_eq!(
        (review.lane, dentist.lane, review.lanes),
        (0, 1, 2),
        "overlap → 2 lanes that day"
    );
    let short = timed.iter().find(|t| t.input == 3).unwrap();
    assert_eq!(short.end_min - short.start_min, 20.);
    // 15-minute minimum and midnight split.
    let split = layout_timed(&[TimedInput {
        start_min: 1430.,
        end_min: 1445.,
        title: "x".into(),
        start_label: String::new(),
        color: None,
    }]);
    assert_eq!(split.len(), 2);
    assert_eq!((split[0].day, split[0].end_min - split[0].start_min), (0, 15.));
    assert_eq!(g.content_y_near(9), 7. * 44.);
}

// ------------------------------------------------------------------ island crops: V2 ring, V1 track, W3, W2

#[test]
fn progress_ring_matches_island() {
    // agent-compact-running: island 132 wide at x = 320 − 66 − 2 = 252, ring at +100, +7 (y 10).
    for (state, value) in [("agent-compact-running", 0.58), ("agent-compact-progress-zero", 0.0)] {
        diff_island(state, 58., [352., 17., 24., 24.], G_MEAN, G_BAD, |tok| {
            let mut g = kit::group(Rect::new(0., 0., 640., 58.));
            g.add(island_bg(tok, Rect::new(340., 10., 40., 38.)));
            let mut r = ProgressRing::new(24., value);
            r.frame.x = 352.;
            r.frame.y = 17.;
            g.add(r.build(tok, LATE));
            g
        });
    }
    testutil::assert_all();
}

#[test]
fn progress_track_matches_island() {
    // music-expanded-playing: island 392×176 at (124, 10); content inset 13 → track at
    // (137, 10 + 13 + 72 + 14 = 109), 366×5; position 83 / 225.
    diff_island(
        "music-expanded-playing",
        200.,
        [133., 105., 374., 13.],
        G_MEAN,
        G_BAD,
        |tok| {
            let mut g = kit::group(Rect::new(0., 0., 640., 200.));
            g.add(island_bg(tok, Rect::new(124., 95., 392., 30.)));
            let mut t = ProgressTrack::new(366.);
            t.position_s = 83.;
            t.length_s = 225.;
            t.update(tok, Duration::ZERO);
            t.frame.x = 137.;
            t.frame.y = 109.;
            g.add(t.build(tok, LATE));
            g
        },
    );
    testutil::assert_all();
}

#[test]
fn circle_buttons_match_island() {
    // Controls row: bottom of the content box (186 − 13 = 173), h 44, spacing 18, centred in
    // 366 → x = 137 + 103 = 240: prev 40×40 (y +2), play 44×44, next 40×40.
    diff_island(
        "music-expanded-playing",
        200.,
        [236., 125., 168., 52.],
        TXT_MEAN,
        TXT_BAD,
        |tok| {
            let mut g = kit::group(Rect::new(0., 0., 640., 200.));
            g.add(island_bg(tok, Rect::new(124., 120., 392., 60.)));
            let mut x = 240.;
            for (glyph, primary) in [("\u{f04ae}", false), ("\u{f03e4}", true), ("\u{f04ad}", false)] {
                let mut b = CircleGlyphButton::new(glyph, primary);
                let d = b.frame.width;
                b.frame.x = x;
                b.frame.y = 129. + kit::vcenter(44., d);
                x += d + 18.;
                g.add(b.build(tok, LATE));
            }
            g
        },
    );
    testutil::assert_all();
}

fn idle_island(tok: &Tok, hover_lock: bool, press_at: Option<Duration>) -> Node {
    // Island = idle row (lock | hh:mm | power), 120.16 wide, centred: x = 320 − hc(120.16).
    let lock_w = kit::measure("\u{f023}", 16., kit::BOLD).width + 23.;
    let clock_w = kit::measure("09:41", 16., kit::BOLD).width;
    let power_w = kit::measure("\u{f011}", 16., kit::BOLD).width + 23.;
    let total = lock_w + clock_w + power_w;
    let x0 = 320. - kit::hc(total);
    let mut g = kit::group(Rect::new(0., 0., 640., 58.));
    g.add(kit::qt_rect(
        Rect::new(x0, 10., total, 38.),
        24.,
        tok.pal.module_background,
        None,
    ));
    let mut lock = PillButton::island("\u{f023}", 11., 12.);
    lock.frame = Rect::new(x0, 10., lock_w, 38.);
    if hover_lock {
        lock.pointer(PointerEvent::Enter { x: 16., y: 19. }, Duration::ZERO);
    }
    if let Some(t) = press_at {
        lock.pointer(
            PointerEvent::Press {
                x: 16.,
                y: 19.,
                button: Button::Left,
            },
            t,
        );
    }
    let mut clock = PillButton::island("09:41", 0., 0.);
    clock.interactive = false;
    clock.frame = Rect::new(x0 + lock_w, 10., clock_w, 38.);
    let mut power = PillButton::island("\u{f011}", 10., 13.);
    power.frame = Rect::new(x0 + lock_w + clock_w, 10., power_w, 38.);
    let now = press_at.map(|t| t + Duration::from_millis(60)).unwrap_or(LATE);
    for b in [&lock, &clock, &power] {
        g.add(b.build(tok, if press_at.is_some() { now } else { LATE }));
    }
    g
}

#[test]
fn island_pill_buttons_match_island() {
    diff_island("idle", 58., [256., 8., 128., 42.], TXT_MEAN, TXT_BAD, |tok| {
        idle_island(tok, false, None)
    });
    // Resolves ui-island.md §4.2: with the pointer over the lock button the reference shows NO
    // hover fill — the z-99 side hotspot (hover-only MouseArea) takes the hover, so the button
    // never sees containsMouse. The island composite must therefore not forward hover to
    // lock/power while the hotspots are visible; PillButton itself still renders hover.
    diff_island(
        "idle-hover-lock",
        58.,
        [256., 8., 128., 42.],
        TXT_MEAN,
        TXT_BAD,
        |tok| idle_island(tok, false, None),
    );
    testutil::assert_all();
}

#[test]
fn track_seek_contract() {
    let mut t = ProgressTrack::new(366.);
    t.length_s = 200.;
    t.update(&Tok::new(false), Duration::ZERO);
    // Press 6 px left of the track (inside the +8 margin) → clamps to 0.
    t.pointer(
        PointerEvent::Press {
            x: -6.,
            y: 2.,
            button: Button::Left,
        },
        Duration::ZERO,
    );
    assert!(t.seeking);
    assert_eq!(t.seek_preview_s, 0.);
    t.pointer(PointerEvent::Move { x: 183., y: 2. }, Duration::from_millis(10));
    assert!((t.displayed_position() - 100.).abs() < 1e-9);
    let r = t.pointer(
        PointerEvent::Release {
            x: 183.,
            y: 2.,
            button: Button::Left,
        },
        Duration::from_millis(20),
    );
    assert_eq!(r.action, Some(TrackAction::SeekRequested(100.)));
    assert_eq!(t.preview_reset_at, Some(Duration::from_millis(720)));
    assert!(!t.expire(Duration::from_millis(700)));
    assert!(t.expire(Duration::from_millis(720)));
    assert!(!t.seeking);
}

#[test]
fn track_seek_cancels_when_length_drops_to_zero() {
    let tok = Tok::new(false);
    let mut t = ProgressTrack::new(366.);
    t.length_s = 200.;
    t.update(&tok, Duration::ZERO);
    t.pointer(
        PointerEvent::Press {
            x: 100.,
            y: 2.,
            button: Button::Left,
        },
        Duration::ZERO,
    );
    assert!(t.seeking);
    t.length_s = 0.;
    t.update(&tok, Duration::from_millis(50));
    assert_eq!(t.state.pressed, None);
    assert_eq!(
        t.preview_reset_at,
        Some(Duration::from_millis(750)),
        "QML onCanceled restarts seekPreviewReset"
    );
    let r = t.pointer(
        PointerEvent::Release {
            x: 100.,
            y: 2.,
            button: Button::Left,
        },
        Duration::from_millis(60),
    );
    assert_eq!(r.action, None, "no seek after cancellation");
    assert!(t.expire(Duration::from_millis(750)));
    assert!(!t.seeking);
}
