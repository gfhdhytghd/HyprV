//! Pixel parity against the ORIGINAL QML components rendered by Qt (qml-ref/render.sh).
//! Skips when the references have not been rendered.
mod common;
use hyprv_ui::{geometry::Rect, scene::{Content, Node, Scene}};
use hyprv_ui_panels::{
    calendar::CalendarPanel,
    control_panel::{ControlPanel, Page},
    fixtures,
    quick_adjust::{Mode, QuickAdjust},
    startup::StartupConfirm,
    stats::StatsPopup,
    wifi::WifiPanel,
};
use std::time::Duration;
use std::path::Path;

pub struct Diff {
    pub max: u8,
    pub over8: usize,
    pub over24: usize,
    pub mean: f64,
    pub n: usize,
}

/// Compare premultiplied RGBA (the straight-alpha PNGs are premultiplied here, so
/// low-alpha colour noise does not count).
pub fn diff(a: &[u8], b: &[u8]) -> Diff {
    let mut d = Diff { max: 0, over8: 0, over24: 0, mean: 0., n: a.len() / 4 };
    let mut sum = 0f64;
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let pm = |p: &[u8]| [p[0] as f32 * p[3] as f32 / 255., p[1] as f32 * p[3] as f32 / 255., p[2] as f32 * p[3] as f32 / 255., p[3] as f32];
        let (x, y) = (pm(pa), pm(pb));
        let m = (0..4).map(|i| (x[i] - y[i]).abs()).fold(0f32, f32::max).round() as u8;
        d.max = d.max.max(m);
        if m > 8 {
            d.over8 += 1;
        }
        if m > 24 {
            d.over24 += 1;
        }
        sum += m as f64;
    }
    d.mean = sum / d.n as f64;
    d
}

fn write_diff(a: &[u8], b: &[u8], w: usize, h: usize, path: &Path) {
    use hyprv_ui::skia as sk;
    let mut px = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let m = (0..4).map(|c| (a[i * 4 + c] as i32 - b[i * 4 + c] as i32).unsigned_abs()).max().unwrap().min(255) as u8;
        px[i * 4] = 255;
        px[i * 4 + 1] = 255 - m;
        px[i * 4 + 2] = 255 - m;
        px[i * 4 + 3] = 255;
    }
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let img = sk::images::raster_from_data(&info, sk::Data::new_copy(&px), w * 4).unwrap();
    let png = img.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
    std::fs::write(path, png.as_bytes()).unwrap();
}

#[test]
fn control_panel_pages_match_qt() {
    let refdir = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml-ref/out");
    let out = common::out_dir();
    let mut report = String::new();
    let mut worst = 0usize;
    for (name, f) in fixtures::all() {
        for dark in [false, true] {
            let theme = if dark { "dark" } else { "light" };
            // StartupConfirm follows the (always light) system palette in both shell themes
            let ref_theme = if name == "startup" && hyprv_ui_panels::startup::QUIRK_STARTUP_CONFIRM_ALWAYS_LIGHT { "light" } else { theme };
            let refpng = refdir.join(format!("{name}--{ref_theme}@2.png"));
            let (qw, qh, qt) = common::read_png(&refpng).unwrap_or_else(|| panic!("missing Qt reference {} (run qml-ref/render.sh)", refpng.display()));
            let model = f(dark);
            let (card, w, h) = match fixtures::page_of(name) {
                "main" => ControlPanel::new().page_preview(&model, Page::Main),
                "bluetooth" => ControlPanel::new().page_preview(&model, Page::Bluetooth),
                "wifi" => WifiPanel::new().preview(&model),
                "stats" => StatsPopup::preview(&model),
                "calendar" => (CalendarPanel::new(520., 500.).build(&model, (0., 0.)), 520., 500.),
                "quick-brightness" => QuickAdjust::new().preview(&model, Mode::Brightness),
                "quick-volume" => QuickAdjust::new().preview(&model, Mode::Volume),
                "startup" => {
                    let mut s = StartupConfirm::new(dark, Duration::ZERO);
                    s.set_focus_ring(false);
                    let h = s.height();
                    (s.build(2.), 360., h)
                }
                p => panic!("unknown page {p}"),
            };
            let mut root = Node::new(Rect::new(0., 0., w, h), Content::Group);
            root.children.push(card);
            let (rw, rh, ours) = common::render(&Scene::new(root), w, h, 2.);
            assert_eq!((rw, rh), (qw, qh), "{name}--{theme}: size");
            common::write_rgba(&out.join(format!("ours-{name}--{theme}@2.png")), rw, rh, &ours);
            let d = diff(&qt, &ours);
            write_diff(&qt, &ours, qw, qh, &out.join(format!("diff-{name}--{theme}@2.png")));
            report += &format!("{name}--{theme}: max {} >8 {} >24 {} mean {:.3}\n", d.max, d.over8, d.over24, d.mean);
            worst = worst.max(d.max as usize);
        }
    }
    eprintln!("{report}");
    std::fs::write(out.join("qml-parity.txt"), &report).unwrap();
    assert!(worst <= PARITY_MAX_DIFF, "parity regression:\n{report}");
}

/// docs/ARCHITECTURE.md gate: per-pixel max channel diff ≤ 8/255.
const PARITY_MAX_DIFF: usize = 8;
