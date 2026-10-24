//! Diff tests against the parity agent's full-shell captures (`rshell/parity/reference/{bar,panels}`).
//! Those images are whole composites, so each widget is *located* (template search in a small
//! window around its spec position, background sampled from the reference) and then diffed;
//! placement itself is the composite's responsibility. The found position is recorded in
//! the test name so a drift is visible in `parity.tsv`.
use super::testutil::{self, Img};
use super::*;
use hyprv_ui::{geometry::Rect, scene::Node};
use std::time::Duration;

const THEMES: [(&str, bool); 2] = [("light", false), ("dark", true)];
const SCALES: [(&str, f64); 2] = [("1", 1.), ("2", 2.)];
const LATE: Duration = Duration::from_secs(10);
const TXT_MEAN: f64 = 4.5;
const TXT_BAD: f64 = 0.045;

struct Probe<'a> {
    component: &'a str,
    state: &'a str,
    /// Logical search window for the patch's top-left: (x0, x1, y0, y1).
    search: (f32, f32, f32, f32),
    /// Logical rectangle sampled (median) as the background under the widget.
    bg: (f32, f32, f32, f32),
    /// Horizontal inset (logical) excluded from search/compare — for widgets whose edges
    /// overlap composite-specific background (e.g. the bar pill's rounded end).
    inset_x: f32,
    /// Vertical inset (logical), same purpose.
    inset_y: f32,
}

fn run(p: Probe, size: (f32, f32), build: impl Fn(&Tok) -> Node) {
    run_scaled(p, size, |tok, _| build(tok));
}
fn run_scaled(p: Probe, size: (f32, f32), build: impl Fn(&Tok, f64) -> Node) {
    for (theme, dark) in THEMES {
        let tok = Tok::new(dark);
        for (tag, scale) in SCALES {
            let path = testutil::shell_ref(p.component, &format!("{}--{theme}@{tag}.png", p.state));
            let reference = testutil::load(&path);
            let s = scale as f32;
            let d = |v: f32| (v * s).round() as usize;
            let bg = reference.median(d(p.bg.0), d(p.bg.1), d(p.bg.2), d(p.bg.3));
            let full = testutil::render(build(&tok, scale), size.0, size.1, scale).over(bg);
            let (ix, iy) = (d(p.inset_x), d(p.inset_y));
            let patch = full.crop(ix, iy, full.w - 2 * ix, full.h - 2 * iy);
            let (x, y) = testutil::find(
                &reference,
                &patch,
                d(p.search.0) + ix,
                d(p.search.1) + ix,
                d(p.search.2) + iy,
                d(p.search.3) + iy,
            );
            let crop: Img = reference.crop(x, y, patch.w, patch.h);
            let (lx, ly) = ((x - ix) as f32 / s, (y - iy) as f32 / s);
            let name = format!("shell:{}/{}@({lx:.1},{ly:.1})--{theme}@{tag}", p.component, p.state);
            testutil::check(&name, &patch, &crop, TXT_MEAN, TXT_BAD);
        }
    }
}

#[test]
fn segmented_row_matches_mihomo_page() {
    // Card at crop x 88 (3072 − 384 − 10 − 2590); content x +10; mode row = 4th row:
    // y = top + 10 + 40 + 10 + 54 + 10 (≈ 132). The 10-px gap above it is panel background.
    run(
        Probe {
            component: "panels",
            state: "cp-mihomo-online",
            search: (90., 106., 120., 145.),
            bg: (110., 124., 300., 5.),
            inset_x: 0.,
            inset_y: 0.,
        },
        (364., 34.),
        |tok| {
            let mut s = SegmentedRow::mihomo_modes(364.);
            s.selected = Some("rule".into());
            s.build(tok, LATE)
        },
    );
    testutil::assert_all();
}

#[test]
fn quick_adjust_slider_matches_shell() {
    // popupBounds 278×62 right-aligned to the primary bar's tray pill, y = bar zone (58).
    run(
        Probe {
            component: "panels",
            state: "qa-volume-30",
            search: (390., 420., 0., 16.),
            bg: (0., 150., 300., 20.),
            inset_x: 0.,
            inset_y: 0.,
        },
        (278., 62.),
        |tok| {
            let mut s = SliderTile::new("\u{f027}", 30.);
            s.frame = Rect::new(0., 0., 278., 62.);
            s.icon_left_inset = 24.;
            s.icon_clickable = true;
            s.build(tok, LATE)
        },
    );
    testutil::assert_all();
}

#[test]
fn tray_missing_icon_placeholder_matches_bar() {
    // The historical fixture name is retained, but Quickshell resolves the missing
    // theme icon to its checker pixmap. A letter applies only when no image exists.
    // The background is sampled from the tray pill's glyph-free top rows.
    run_scaled(
        Probe {
            component: "bar",
            state: "tray-fallback-letter",
            search: (2960., 3010., 9., 11.),
            bg: (2885., 12., 30., 2.),
            inset_x: 0.,
            inset_y: 0.,
        },
        (18., 38.),
        |tok, scale| {
            let (side, pixels) = crate::icon_theme::missing_icon((18. * scale) as u32);
            let info = hyprv_ui::skia::ImageInfo::new(
                (side as i32, side as i32), hyprv_ui::skia::ColorType::RGBA8888,
                hyprv_ui::skia::AlphaType::Premul, None);
            let mut button = TrayButton::new("zzz-unknown");
            button.icon = hyprv_ui::skia::images::raster_from_data(
                &info, hyprv_ui::skia::Data::new_copy(&pixels), side as usize * 4);
            button.build(tok, LATE)
        },
    );
    testutil::assert_all();
}

#[test]
fn notification_glyphs_match_bar() {
    for (state, dnd, dot) in [
        ("notifications-bell", false, false),
        ("notifications-dnd", true, false),
        ("notifications-dot", false, true),
    ] {
        let b = GlyphButton::notification(dnd, dot);
        let size = (b.frame.width, b.frame.height);
        run(
            Probe {
                component: "bar",
                state,
                search: (3020., 3050., 9., 11.),
                bg: (2885., 12., 30., 2.),
                inset_x: 4.,
                inset_y: 2.,
            },
            size,
            |tok| b.build(tok, LATE),
        );
    }
    testutil::assert_all();
}

#[test]
fn dialog_buttons_match_startup_confirm() {
    // Window 360×156 centred on 3072×1728 → crop (66, 106); buttons right-aligned in the
    // 312-wide content at y 100: skip at x 152, launch at x 248 (relative to the card).
    for (state, hover, press) in [
        ("startup-confirm", false, false),
        ("startup-confirm-hover", true, false),
        ("startup-confirm-press", true, true),
    ] {
        for (label, x) in [("本次跳过", 218.), ("启动", 314.)] {
            let hot = label == "启动";
            run(
                Probe {
                    component: "panels",
                    state,
                    // Patch = button + 3 px ring margin.
                    search: (x - 11., x + 5., 195., 211.),
                    bg: (100., 120., 150., 3.),
                    inset_x: 0.,
                    inset_y: 0.,
                },
                (94., 38.),
                |tok| {
                    // StartupConfirm's `dark` comes from SystemPalette (StartupConfirm.qml:14), not the
                    // shell theme; the harness palette is light, so both captures show the light dialog.
                    let _ = tok;
                    let mut b = DialogButton::new(label, false);
                    if hot && hover {
                        b.state.hovered = true;
                    }
                    if hot && press {
                        b.state.pressed = Some(Button::Left);
                    }
                    // The skip button takes keyboard focus 150 ms after start (Tab reason) →
                    // its 2 px accent focus ring is visible in the live shell. Pressing "启动" with
                    // the mouse moves focus there with MouseFocusReason → no ring anywhere.
                    b.visual_focus = !hot && !press;
                    b.frame = Rect::new(3., 3., 88., 32.);
                    b.build(tok, LATE)
                },
            );
        }
    }
    testutil::assert_all();
}
