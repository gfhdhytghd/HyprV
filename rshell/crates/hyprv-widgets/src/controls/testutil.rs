//! Test helpers: offscreen render, reference loading/cropping, premultiplied diffs.
//! Output PNGs (mine / reference / ×4 diff) go to `$HYPRV_WIDGET_TEST_OUT`
//! (default `$TMPDIR/hyprv-widgets-test`); every diff is appended to `parity.tsv` there.
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, Scene},
    skia as sk,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub struct Img {
    pub w: usize,
    pub h: usize,
    /// Straight-alpha RGBA8.
    pub px: Vec<u8>,
}

fn locate(rel_from_this_dir: &str) -> PathBuf {
    let here = Path::new(file!()).parent().unwrap().join(rel_from_this_dir);
    if here.is_absolute() {
        return here;
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join(&here);
        if cand.exists() {
            return cand;
        }
        if !dir.pop() {
            return here;
        }
    }
}

pub fn widget_ref(name: &str) -> PathBuf {
    locate("qml-ref/reference").join(name)
}

pub fn island_ref(name: &str) -> PathBuf {
    locate("qml-ref")
        .join("../../../../../parity/reference/island")
        .join(name)
}

pub fn out_dir() -> PathBuf {
    let d = std::env::var_os("HYPRV_WIDGET_TEST_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("hyprv-widgets-test"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn to_img(image: &sk::Image) -> Img {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let info = sk::ImageInfo::new(
        (w as i32, h as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    let mut px = vec![0u8; w * h * 4];
    assert!(image.read_pixels(&info, &mut px, w * 4, (0, 0), sk::image::CachingHint::Allow));
    Img { w, h, px }
}

/// Render `node` (positioned in canvas coordinates) on a transparent `w×h` logical canvas.
pub fn render(node: Node, w: f32, h: f32, scale: f64) -> Img {
    let mut root = Node::new(Rect::new(0., 0., w, h), Content::Group);
    root.add(node);
    let scene = Scene::new(root);
    let mut r = Renderer::new();
    let mut surface = r.raster(&scene, w, h, scale).expect("raster");
    to_img(&surface.image_snapshot())
}

pub fn load(path: &Path) -> Img {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let image = sk::Image::from_encoded(sk::Data::new_copy(&data)).expect("decode png");
    to_img(&image)
}

impl Img {
    pub fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Img {
        let mut px = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            let s = (row * self.w + x) * 4;
            px.extend_from_slice(&self.px[s..s + w * 4]);
        }
        Img { w, h, px }
    }
    pub fn save(&self, path: &Path) {
        let info = sk::ImageInfo::new(
            (self.w as i32, self.h as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        let image = sk::images::raster_from_data(&info, sk::Data::new_copy(&self.px), self.w * 4).unwrap();
        let data = image.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
        std::fs::write(path, data.as_bytes()).unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Diff {
    /// Mean absolute premultiplied channel difference (0..255) over the crop.
    pub mean: f64,
    /// Largest premultiplied channel difference.
    pub max: u8,
    /// Fraction of pixels whose largest premultiplied channel difference exceeds 48.
    pub bad: f64,
}

fn premul(p: &[u8]) -> [f64; 4] {
    let a = p[3] as f64 / 255.;
    [p[0] as f64 * a, p[1] as f64 * a, p[2] as f64 * a, p[3] as f64]
}

thread_local! {
    static FAILURES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Panic with every failure recorded by `check` in this test (call at the end of a test).
pub fn assert_all() {
    let f = FAILURES.with(|f| std::mem::take(&mut *f.borrow_mut()));
    assert!(f.is_empty(), "{} parity failure(s):\n{}", f.len(), f.join("\n"));
}

/// Compare and record; failures are collected and raised by [`assert_all`].
pub fn check(name: &str, mine: &Img, reference: &Img, max_mean: f64, max_bad: f64) -> Diff {
    assert_eq!((mine.w, mine.h), (reference.w, reference.h), "{name}: size mismatch");
    let n = mine.w * mine.h;
    let (mut sum, mut max, mut bad) = (0., 0u8, 0usize);
    let mut diff = vec![0u8; n * 4];
    for i in 0..n {
        let a = premul(&mine.px[i * 4..i * 4 + 4]);
        let b = premul(&reference.px[i * 4..i * 4 + 4]);
        let mut m = 0f64;
        for c in 0..4 {
            let d = (a[c] - b[c]).abs();
            sum += d;
            m = m.max(d);
        }
        max = max.max(m.round() as u8);
        if m > 48. {
            bad += 1;
        }
        let v = (m * 4.).min(255.) as u8;
        diff[i * 4..i * 4 + 4].copy_from_slice(&[v, 0, 0, 255]);
    }
    let d = Diff {
        mean: sum / (n * 4) as f64,
        max,
        bad: bad as f64 / n as f64,
    };
    let out = out_dir();
    let stem = name.replace(['/', ' '], "_");
    mine.save(&out.join(format!("{stem}.mine.png")));
    reference.save(&out.join(format!("{stem}.ref.png")));
    Img {
        w: mine.w,
        h: mine.h,
        px: diff,
    }
    .save(&out.join(format!("{stem}.diff.png")));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join("parity.tsv"))
        .unwrap();
    // One write per record: tests run in parallel and append to the same file.
    f.write_all(format!("{name}\t{:.3}\t{}\t{:.4}\n", d.mean, d.max, d.bad).as_bytes())
        .unwrap();
    if !(d.mean <= max_mean && d.bad <= max_bad) {
        let msg = format!(
            "{name}: mean {:.3} (≤{max_mean}) max {} bad {:.4} (≤{max_bad}) — see {}",
            d.mean,
            d.max,
            d.bad,
            out.display()
        );
        FAILURES.with(|f| f.borrow_mut().push(msg));
    }
    d
}

/// Write a rendered PNG for visual inspection (spec-only tests).
pub fn snapshot(name: &str, img: &Img) {
    img.save(&out_dir().join(format!("{name}.png")));
}

/// `rshell/parity/reference/<component>/<name>` (full-shell captures by the parity agent).
pub fn shell_ref(component: &str, name: &str) -> PathBuf {
    locate("qml-ref")
        .join("../../../../../parity/reference")
        .join(component)
        .join(name)
}

impl Img {
    pub fn px_at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    /// Straight-alpha "self over bg" (bg straight RGBA).
    pub fn over(&self, bg: [u8; 4]) -> Img {
        let mut px = Vec::with_capacity(self.px.len());
        let (bga, bgc) = (bg[3] as f64 / 255., [bg[0] as f64, bg[1] as f64, bg[2] as f64]);
        for p in self.px.chunks(4) {
            let a = p[3] as f64 / 255.;
            let out_a = a + bga * (1. - a);
            for c in 0..3 {
                let v = if out_a > 0. {
                    (p[c] as f64 * a + bgc[c] * bga * (1. - a)) / out_a
                } else {
                    0.
                };
                px.push(v.round() as u8);
            }
            px.push((out_a * 255.).round() as u8);
        }
        Img {
            w: self.w,
            h: self.h,
            px,
        }
    }
    /// Median straight colour of a rectangle (background estimate).
    pub fn median(&self, x: usize, y: usize, w: usize, h: usize) -> [u8; 4] {
        let mut ch: [Vec<u8>; 4] = Default::default();
        for yy in y..y + h {
            for xx in x..x + w {
                let p = self.px_at(xx, yy);
                for c in 0..4 {
                    ch[c].push(p[c]);
                }
            }
        }
        let mut out = [0u8; 4];
        for c in 0..4 {
            ch[c].sort_unstable();
            out[c] = ch[c][ch[c].len() / 2];
        }
        out
    }
}

/// Find `patch` in `hay` (device pixels): best top-left within `[x0..=x1]×[y0..=y1]` by
/// premultiplied SSE. Returns (x, y).
pub fn find(hay: &Img, patch: &Img, x0: usize, x1: usize, y0: usize, y1: usize) -> (usize, usize) {
    let pm: Vec<[f64; 4]> = patch.px.chunks(4).map(premul).collect();
    let mut best = (f64::MAX, x0, y0);
    for y in y0..=y1.min(hay.h - patch.h) {
        for x in x0..=x1.min(hay.w - patch.w) {
            let mut sse = 0.;
            'rows: for py in 0..patch.h {
                let row = ((y + py) * hay.w + x) * 4;
                for px in 0..patch.w {
                    let h = premul(&hay.px[row + px * 4..row + px * 4 + 4]);
                    let p = pm[py * patch.w + px];
                    for c in 0..4 {
                        sse += (h[c] - p[c]).powi(2);
                    }
                }
                if sse > best.0 {
                    break 'rows;
                }
            }
            if sse < best.0 {
                best = (sse, x, y);
            }
        }
    }
    (best.1, best.2)
}
