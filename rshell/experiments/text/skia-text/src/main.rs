//! textcmp: reproduce Qt Quick text (NativeRendering and QtRendering/distance field)
//! with skia-safe / FreeType and diff against Qt reference renders made by ../qt-ref.
//!
//! usage: textcmp <scene.json> <qt-dump.txt> <qt.png> <out-dir> [scale]
//!
//! Every method produces an alpha (coverage) plane in device pixels. Qt's text is a
//! single opaque colour, so comparing the alpha channel isolates rasterisation and
//! placement from colour handling (verified separately: light vs dark Qt alpha planes).

use freetype as ft;
use serde_json::Value;
use skia_safe as sk;
use skia_safe::textlayout::{
    FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle, TypefaceFontProvider,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const JB: &str = "JetBrainsMono Nerd Font";
const HM: &str = "HarmonyOS Sans SC";

#[derive(Clone, Debug)]
struct Sample {
    name: String,
    text: String,
    px: f32,
    weight: i32,
    x: f64,
    y: f64,
    native: bool,
    letter_spacing: f64,
    width: Option<f64>, // set → elide: Text.ElideRight
}

#[derive(Clone, Debug, Default)]
struct QtRun {
    family: String,
    positions: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
struct QtText {
    implicit_w: f64,
    implicit_h: f64,
    ascent: f64,
    runs: Vec<QtRun>,
}

/// Font face resolved exactly like fontconfig resolves it for Qt.
#[derive(Clone)]
struct Face {
    file: PathBuf,
    index: isize, // FreeType face index incl. named-instance bits
    typeface: sk::Typeface,
    upem: f64,
    ascender: f64,  // hhea/typo (identical for both fonts used here), font units
    descender: f64, // positive, font units
}

fn fc_weight(qt: i32) -> i32 {
    match qt {
        ..=350 => 50,
        351..=450 => 80,
        451..=550 => 100,
        551..=650 => 180,
        651..=750 => 200,
        _ => 210,
    }
}

fn resolve(family: &str, weight: i32, ftlib: &ft::Library) -> Face {
    let out = Command::new("fc-match")
        .args(["-f", "%{file}|%{index}", &format!("{family}:weight={}", fc_weight(weight))])
        .output()
        .expect("fc-match");
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let (file, index) = s.split_once('|').unwrap();
    let index: isize = index.trim().parse().unwrap();
    let data = std::fs::read(file).unwrap();
    // Skia's FreeType backend does not understand named-instance bits in the
    // index, so open face 0 and pin the variation axes to the named instance.
    let base = sk::FontMgr::new()
        .new_from_data(&data, (index & 0xffff) as usize)
        .expect("typeface");
    let typeface = if index >> 16 != 0 {
        let face = ftlib.new_face(file, index).unwrap();
        let coords = named_instance_coords(&face);
        let params = base.variation_design_parameters().unwrap_or_default();
        let coordinates: Vec<sk::font_arguments::variation_position::Coordinate> = params
            .iter()
            .zip(coords.iter())
            .map(|(p, v)| sk::font_arguments::variation_position::Coordinate { axis: p.tag, value: *v as f32 })
            .collect();
        let pos = sk::font_arguments::VariationPosition { coordinates: &coordinates };
        let args = sk::FontArguments::new().set_variation_design_position(pos);
        base.clone_with_arguments(&args).expect("clone_with_arguments")
    } else {
        base
    };
    let face = ftlib.new_face(file, index).unwrap();
    let upem = face.em_size() as f64;
    let ascender = face.ascender() as f64;
    let descender = -(face.descender() as f64);
    Face { file: PathBuf::from(file), index, typeface, upem, ascender, descender }
}

/// Current design coordinates of an FT face opened on a named instance.
fn named_instance_coords(face: &ft::Face) -> Vec<f64> {
    unsafe {
        let raw = face.raw() as *const ft::ffi::FT_FaceRec as *mut ft::ffi::FT_FaceRec;
        let mut mm: *mut ft::ffi::FT_MM_Var = std::ptr::null_mut();
        if ft::ffi::FT_Get_MM_Var(raw, &mut mm) != 0 {
            return vec![];
        }
        let n = (*mm).num_axis as usize;
        let mut c = vec![0 as ft::ffi::FT_Fixed; n];
        ft::ffi::FT_Get_Var_Design_Coordinates(raw, n as u32, c.as_mut_ptr());
        ft::ffi::FT_Done_MM_Var((*raw).glyph.as_ref().unwrap().library, mm);
        c.iter().map(|v| *v as f64 / 65536.0).collect()
    }
}

// ---------------------------------------------------------------- layout

#[derive(Clone, Debug)]
struct Glyph {
    face: usize, // 0 = JB, 1 = HM
    id: u16,
    x: f64, // logical, relative to item origin
}

#[derive(Clone, Debug)]
struct Line {
    glyphs: Vec<Glyph>,
    ascent: f64,
    width: f64,
}

/// Qt advance: FreeType linearHoriAdvance (16.16) >> 10 → 26.6, i.e. truncated to 1/64 px.

fn trunc64(v: f64) -> f64 {
    (v * 64.0 + 1e-9).floor() / 64.0
}

thread_local! {
    static FT: ft::Library = ft::Library::init().unwrap();
    static FACES: std::cell::RefCell<HashMap<(PathBuf, isize), ft::Face>> = std::cell::RefCell::new(HashMap::new());
}

/// Qt's design advance: FT linearHoriAdvance (16.16) >> 10, i.e. 26.6 truncated.
fn ft_linear_advance(f: &Face, glyph: u16, px: f64) -> f64 {
    FT.with(|lib| {
        FACES.with(|faces| {
            let mut faces = faces.borrow_mut();
            let face = faces.entry((f.file.clone(), f.index)).or_insert_with(|| lib.new_face(&f.file, f.index).unwrap());
            face.set_char_size(0, (px * 64.0) as isize, 0, 0).unwrap();
            face.load_glyph(glyph as u32, ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP).unwrap();
            let lin = face.glyph().linear_hori_advance() as i64;
            (lin >> 10) as f64 / 64.0
        })
    })
}

/// QQuickText single-line ElideRight → QTextEngine::elidedText: the ellipsis comes from
/// the primary font only; keep whole graphemes while the running width < width − ellipsis
/// (the grapheme that reaches the limit is dropped); the result is laid out afresh.
fn layout_elided(s: &Sample, faces: &[Face; 2]) -> Line {
    let full = layout(s, faces);
    let Some(width) = s.width else { return full };
    if full.width <= width || s.text.chars().count() <= 1 {
        return full;
    }
    let ell_glyph = faces[0].typeface.unichar_to_glyph('\u{2026}' as i32);
    let ell_w = ft_linear_advance(&faces[0], ell_glyph, s.px as f64);
    let available = width - ell_w;
    let chars: Vec<char> = s.text.chars().collect();
    assert_eq!(chars.len(), full.glyphs.len(), "experiment assumes 1 glyph per char");
    let adv = |i: usize| -> f64 {
        let end = if i + 1 < full.glyphs.len() { full.glyphs[i + 1].x } else { full.width };
        end - full.glyphs[i].x
    };
    let (mut cur, mut pos, mut next) = (0.0, 0usize, 0usize);
    loop {
        pos = next;
        next += 1;
        cur += adv(pos);
        if !(next < chars.len() && cur < available) {
            break;
        }
    }
    let mut e = s.clone();
    e.text = chars[..pos].iter().collect::<String>() + "\u{2026}";
    e.width = None;
    layout(&e, faces)
}

/// Shape with skia's paragraph (HarfBuzz, same features as Qt: calt/liga on), itemising
/// JB → HM exactly like QFontEngineMulti, then replace the advances with Qt's.
fn layout(s: &Sample, faces: &[Face; 2]) -> Line {
    let mut provider = TypefaceFontProvider::new();
    provider.register_typeface(faces[0].typeface.clone(), Some(JB));
    provider.register_typeface(faces[1].typeface.clone(), Some(HM));
    let mut fc = FontCollection::new();
    fc.set_asset_font_manager(Some(provider.into()));
    fc.disable_font_fallback();
    let mut ts = TextStyle::new();
    ts.set_font_families(&[JB, HM]);
    ts.set_font_size(s.px);
    ts.set_font_style(sk::FontStyle::new(
        sk::font_style::Weight::from(s.weight),
        sk::font_style::Width::NORMAL,
        sk::font_style::Slant::Upright,
    ));
    let mut ps = ParagraphStyle::new();
    ps.set_text_style(&ts);
    let mut b = ParagraphBuilder::new(&ps, fc);
    b.push_style(&ts);
    b.add_text(&s.text);
    let mut p = b.build();
    p.layout(10000.0);
    let mut glyphs = Vec::new();
    p.visit(|_, info| {
        if let Some(info) = info {
            let tf = info.font().typeface();
            let fam = tf.family_name();
            let face = if fam == faces[0].typeface.family_name() { 0 } else { 1 };
            for g in info.glyphs() {
                glyphs.push(Glyph { face, id: *g, x: 0.0 });
            }
        }
    });
    let mut pen = 0.0;
    let mut used = [true, false]; // QFontEngineMulti: the primary engine always contributes
    for g in glyphs.iter_mut() {
        let f = &faces[g.face];
        g.x = pen;
        pen += ft_linear_advance(f, g.id, s.px as f64) + s.letter_spacing;
        used[g.face] = true;
    }
    let ascent = (0..2)
        .filter(|i| used[*i])
        .map(|i| trunc64(faces[i].ascender * s.px as f64 / faces[i].upem))
        .fold(0.0, f64::max);
    Line { glyphs, ascent, width: pen }
}

// ---------------------------------------------------------------- planes

struct Plane {
    w: usize,
    h: usize,
    a: Vec<f32>, // coverage 0..1
}

impl Plane {
    fn new(w: usize, h: usize) -> Self {
        Plane { w, h, a: vec![0.0; w * h] }
    }
    /// Source-over of a coverage value (premultiplied blending with an opaque colour).
    fn over(&mut self, x: i64, y: i64, c: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || c <= 0.0 {
            return;
        }
        let i = y as usize * self.w + x as usize;
        self.a[i] = c + self.a[i] * (1.0 - c);
    }
    fn to_u8(&self) -> Vec<u8> {
        self.a.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
    }
}

fn surface_alpha(surface: &mut sk::Surface) -> Vec<u8> {
    let info = sk::ImageInfo::new(
        (surface.width(), surface.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut px = vec![0u8; (surface.width() * surface.height() * 4) as usize];
    surface.read_pixels(&info, &mut px, surface.width() as usize * 4, (0, 0));
    px.chunks(4).map(|c| c[3]).collect()
}

// Method "para": what crates/hyprv-ui does today (Paragraph on a scaled canvas).
fn render_para(samples: &[Sample], faces_for: &dyn Fn(i32) -> [Face; 2], w: i32, h: i32, scale: f32) -> Vec<u8> {
    let mut surface = sk::surfaces::raster_n32_premul((w, h)).unwrap();
    let canvas = surface.canvas();
    canvas.clear(sk::Color::TRANSPARENT);
    canvas.scale((scale, scale));
    for s in samples {
        let faces = faces_for(s.weight);
        let mut provider = TypefaceFontProvider::new();
        provider.register_typeface(faces[0].typeface.clone(), Some(JB));
        provider.register_typeface(faces[1].typeface.clone(), Some(HM));
        let mut fc = FontCollection::new();
        fc.set_asset_font_manager(Some(provider.into()));
        let mut ts = TextStyle::new();
        ts.set_font_families(&[JB, HM]);
        ts.set_font_size(s.px);
        ts.set_color(sk::Color::BLACK);
        ts.set_font_style(sk::FontStyle::new(
            sk::font_style::Weight::from(s.weight),
            sk::font_style::Width::NORMAL,
            sk::font_style::Slant::Upright,
        ));
        let mut ps = ParagraphStyle::new();
        ps.set_text_style(&ts);
        let mut b = ParagraphBuilder::new(&ps, fc);
        b.push_style(&ts);
        b.add_text(&s.text);
        let mut p = b.build();
        p.layout(10000.0);
        p.paint(canvas, (s.x as f32, s.y as f32));
    }
    surface_alpha(&mut surface)
}

/// Device placement of a native glyph exactly as QSGTextMaskMaterial::populate does it.
/// Returns (integer device x of the glyph origin, subpixel offset in 1/4 px, device baseline row).
fn qt_native_place(item_x: f64, item_y: f64, local_x: f64, local_baseline: f64, dpr: f64) -> (i64, f64, i64) {
    // QSGTextMaskMaterial::populate works in the text node's local coordinates …
    let fx = (local_x * 64.0).round() / 64.0; // QFixedPoint::fromPointF
    let dx = fx * dpr;
    let mut sub = 0.0;
    if dx != 0.0 {
        let v = (dx - dx.floor()) + 1.0 / 64.0;
        sub = (v * 4.0).floor() / 4.0;
    }
    let fy = (local_baseline * 64.0).round() / 64.0;
    // … and textmask.vert snaps the final (item-translated) vertex: floor(x*dpr + 0.5).
    let ox = (item_x * dpr + 0.5).floor() as i64;
    let oy = (item_y * dpr + 0.5).floor() as i64;
    (ox + dx.floor() as i64, sub, oy + (fy * dpr).round() as i64)
}

// Method "skia": skia glyph rasteriser, Qt placement and Qt advances.
fn render_skia(items: &[(Sample, Line, [Face; 2])], w: i32, h: i32, dpr: f64) -> Vec<u8> {
    let mut surface = sk::surfaces::raster_n32_premul((w, h)).unwrap();
    let canvas = surface.canvas();
    canvas.clear(sk::Color::TRANSPARENT);
    let mut paint = sk::Paint::default();
    paint.set_color(sk::Color::BLACK);
    paint.set_anti_alias(true);
    for (s, line, faces) in items {
        for g in &line.glyphs {
            let f = &faces[g.face];
            let mut font = sk::Font::from_typeface(&f.typeface, (s.px as f64 * dpr) as f32);
            font.set_edging(sk::font::Edging::AntiAlias);
            font.set_hinting(sk::FontHinting::None);
            font.set_subpixel(true);
            font.set_linear_metrics(true);
            font.set_baseline_snap(true);
            let (ix, sub, by) = qt_native_place(s.x, s.y, g.x, line.ascent, dpr);
            canvas.draw_glyphs_at(&[g.id], &[sk::Point::new((ix as f64 + sub) as f32, by as f32)][..], (0, 0), &font, &paint);
        }
    }
    surface_alpha(&mut surface)
}

// Method "ft": FreeType exactly as QFontEngineFT::loadGlyph for a scaled glyph cache:
// FT_Set_Char_Size(px), FT_Set_Transform(scale dpr, delta = subpixel), NO_HINTING|NO_BITMAP,
// FT_RENDER_MODE_NORMAL, bitmap placed at (floor(dx)+left, round(dy)-top).
fn render_ft(items: &[(Sample, Line, [Face; 2])], lib: &ft::Library, w: usize, h: usize, dpr: f64) -> Vec<u8> {
    let mut plane = Plane::new(w, h);
    let mut faces: HashMap<(PathBuf, isize), ft::Face> = HashMap::new();
    for (s, line, fs) in items {
        for g in &line.glyphs {
            let f = &fs[g.face];
            let face = faces
                .entry((f.file.clone(), f.index))
                .or_insert_with(|| lib.new_face(&f.file, f.index).unwrap());
            face.set_char_size(0, (s.px as f64 * 64.0) as isize, 0, 0).unwrap();
            let (ix, sub, by) = qt_native_place(s.x, s.y, g.x, line.ascent, dpr);
            let k = (dpr * 65536.0).round() as ft::ffi::FT_Fixed;
            let mut m = ft::Matrix { xx: k, xy: 0, yx: 0, yy: k };
            let mut d = ft::Vector { x: (sub * 64.0) as ft::ffi::FT_Pos, y: 0 };
            face.set_transform(&mut m, &mut d);
            let flags = ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP | ft::face::LoadFlag::FORCE_AUTOHINT;
            if face.load_glyph(g.id as u32, flags).is_err() {
                continue;
            }
            let slot = face.glyph();
            if qt_embolden(face, s.weight, s.px as f64) {
                unsafe { ft::ffi::FT_GlyphSlot_Embolden(slot.raw() as *const _ as *mut _) };
            }
            slot.render_glyph(ft::RenderMode::Normal).unwrap();
            let bm = slot.bitmap();
            let (left, top) = (slot.bitmap_left() as i64, slot.bitmap_top() as i64);
            let pitch = bm.pitch() as i64;
            let buf = bm.buffer();
            for yy in 0..bm.rows() as i64 {
                for xx in 0..bm.width() as i64 {
                    let c = buf[(yy * pitch + xx) as usize] as f32 / 255.0;
                    plane.over(ix + left + xx, by - top + yy, c);
                }
            }
        }
    }
    plane.to_u8()
}

/// QFontEngineFT::init fake-bold rule. For variable fonts opened through fontconfig Qt
/// does not get a named-instance index here, so calculateActualWeight falls back to the
/// OS/2 usWeightClass of the default instance (HarmonyOS Sans SC: 400) → synthetic bold.
fn qt_embolden(face: &ft::Face, weight: i32, px: f64) -> bool {
    let raw = face.raw();
    let bold_flag = raw.style_flags & ft::ffi::FT_STYLE_FLAG_BOLD as ft::ffi::FT_Long != 0;
    let fixed = raw.face_flags & ft::ffi::FT_FACE_FLAG_FIXED_WIDTH as ft::ffi::FT_Long != 0;
    if weight < 700 || bold_flag || fixed || px >= 64.0 {
        return false;
    }
    let os2 = unsafe { ft::ffi::FT_Get_Sfnt_Table(raw as *const _ as *mut _, ft::ffi::ft_sfnt_os2) as *const ft::ffi::TT_OS2 };
    let actual = if os2.is_null() { 700 } else { unsafe { (*os2).usWeightClass } };
    actual < 700
}

// ---------------------------------------------------------------- distance field

/// Distance field of one glyph in Qt's layout: base size 54 px (fonts here have no
/// narrow outlines), texel 0 = path bbox top-left − 5 px, 8-bit, 255 inside, 0 outside,
/// linear ramp over ±5 px (QT_DISTANCEFIELD_RADIUS / SCALE).
struct Df {
    w: usize,
    h: usize,
    bx: f64,
    by: f64,
    data: Vec<u8>,
}

fn make_df(face: &Face, glyph: u16) -> Option<Df> {
    const BASE: f64 = 54.0;
    const SCALE: f64 = 16.0;
    const R: f64 = 5.0;
    let font = sk::Font::from_typeface(&face.typeface, (BASE * SCALE) as f32);
    let path = font.get_path(glyph)?;
    let mut path = path.with_transform(&sk::Matrix::scale((1.0 / SCALE as f32, 1.0 / SCALE as f32)));
    path.set_fill_type(sk::PathFillType::Winding);
    // Qt runs qSimplifyPath (union) first: overlapping CJK strokes have no inner edges.
    if let Some(simple) = path.simplify() {
        path = simple;
    }
    let b = path.bounds();
    if b.is_empty() {
        return None;
    }
    let (bx, by) = (b.left as f64, b.top as f64);
    let w = (b.width() as f64).ceil() as usize + 10;
    let h = (b.height() as f64).ceil() as usize + 10;
    // Densely sample every contour; distance to the polyline approximates the
    // outline distance to well below one 8-bit step at this resolution.
    let mut segs: Vec<(sk::Point, sk::Point)> = Vec::new();
    let mut it = sk::ContourMeasureIter::new(&path, true, 1.0);
    while let Some(c) = it.next() {
        let len = c.length();
        let n = (len / 0.05).ceil().max(4.0) as usize;
        let mut prev = c.pos_tan(0.0).unwrap().0;
        for i in 1..=n {
            let p = c.pos_tan(len * i as f32 / n as f32).unwrap().0;
            segs.push((prev, p));
            prev = p;
        }
    }
    let mut data = vec![0u8; w * h];
    for ty in 0..h {
        for tx in 0..w {
            // texel centre in path coordinates
            let px = bx - R + tx as f64 + 0.5;
            let py = by - R + ty as f64 + 0.5;
            let mut best = f64::MAX;
            for (a, bb) in &segs {
                let (ax, ay, bxx, byy) = (a.x as f64, a.y as f64, bb.x as f64, bb.y as f64);
                let (vx, vy) = (bxx - ax, byy - ay);
                let l2 = vx * vx + vy * vy;
                let t = if l2 > 0.0 { (((px - ax) * vx + (py - ay) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let (qx, qy) = (ax + t * vx - px, ay + t * vy - py);
                best = best.min(qx * qx + qy * qy);
            }
            let mut d = best.sqrt();
            if path.contains(sk::Point::new(px as f32, py as f32)) {
                d = -d;
            }
            // Qt: 8.8 value v ∈ [−127.5, 127.5] (inside negative), byte = (0x7f80 − v) >> 8
            let v = ((d / R).clamp(-1.0, 1.0) * 32640.0).round() as i64;
            data[ty * w + tx] = ((0x7f80 - v) >> 8).clamp(0, 255) as u8;
        }
    }
    Some(Df { w, h, bx, by, data })
}

fn df_threshold(scale: f64) -> (f64, f64) {
    // qsgdistancefieldglyphnode_p.cpp thresholdFunc / spreadFunc
    let base = 0.5 - ((scale.clamp(0.15, 0.3) - 0.15) / (0.3 - 0.15) * -0.065 + 0.065);
    let range = 0.06 / scale;
    ((base - range).max(0.0), (base + range).min(1.0))
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bilinear sample (GL_LINEAR, clamp) of the 8-bit field at texel coordinate (u, v).
fn sample(df: &Df, u: f64, v: f64) -> f64 {
    let x = u - 0.5;
    let y = v - 0.5;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let at = |xi: i64, yi: i64| -> f64 {
        let xi = xi.clamp(0, df.w as i64 - 1) as usize;
        let yi = yi.clamp(0, df.h as i64 - 1) as usize;
        df.data[yi * df.w + xi] as f64 / 255.0
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let a = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
    let b = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
    a * (1.0 - fy) + b * fy
}

fn render_df(items: &[(Sample, Line, [Face; 2])], w: usize, h: usize, dpr: f64) -> Vec<u8> {
    let mut plane = Plane::new(w, h);
    let mut cache: HashMap<(PathBuf, isize, u16), Option<Df>> = HashMap::new();
    for (s, line, fs) in items {
        let font_scale = s.px as f64 / 54.0;
        let (amin, amax) = df_threshold(font_scale * dpr);
        for g in &line.glyphs {
            let f = &fs[g.face];
            let df = cache.entry((f.file.clone(), f.index, g.id)).or_insert_with(|| make_df(f, g.id));
            let Some(df) = df else { continue };
            // No snapping: the quad maps base-size coordinates linearly to logical space.
            let ox = s.x + g.x;
            let oy = s.y + line.ascent;
            // quad = texture extent: [bbox − 5 base px] → logical
            let qx0 = ox + (df.bx - 5.0) * font_scale;
            let qy0 = oy + (df.by - 5.0) * font_scale;
            let qx1 = qx0 + df.w as f64 * font_scale;
            let qy1 = qy0 + df.h as f64 * font_scale;
            let (px0, py0) = ((qx0 * dpr).floor() as i64, (qy0 * dpr).floor() as i64);
            let (px1, py1) = ((qx1 * dpr).ceil() as i64, (qy1 * dpr).ceil() as i64);
            for py in py0..py1 {
                for px in px0..px1 {
                    let lx = (px as f64 + 0.5) / dpr;
                    let ly = (py as f64 + 0.5) / dpr;
                    // rasterisation rule: pixel centre inside the quad
                    if lx < qx0 || lx >= qx1 || ly < qy0 || ly >= qy1 {
                        continue;
                    }
                    let u = (lx - qx0) / font_scale;
                    let v = (ly - qy0) / font_scale;
                    let a = smoothstep(amin, amax, sample(df, u, v));
                    plane.over(px, py, a as f32);
                }
            }
        }
    }
    plane.to_u8()
}

// ---------------------------------------------------------------- io / stats

fn read_png_alpha(path: &Path) -> (usize, usize, Vec<u8>) {
    let data = sk::Data::new_copy(&std::fs::read(path).unwrap());
    let img = sk::Image::from_encoded(data).unwrap();
    let info = sk::ImageInfo::new((img.width(), img.height()), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let mut px = vec![0u8; (img.width() * img.height() * 4) as usize];
    img.read_pixels(&info, &mut px, img.width() as usize * 4, (0, 0), sk::image::CachingHint::Allow);
    (img.width() as usize, img.height() as usize, px.chunks(4).map(|c| c[3]).collect())
}

fn write_gray(path: &Path, w: usize, h: usize, a: &[u8]) {
    let rgba: Vec<u8> = a.iter().flat_map(|v| [0, 0, 0, *v]).collect();
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let img = sk::images::raster_from_data(&info, sk::Data::new_copy(&rgba), w * 4).unwrap();
    let png = img.encode(None, sk::EncodedImageFormat::PNG, None).unwrap();
    std::fs::write(path, png.as_bytes()).unwrap();
}

#[derive(Default, Clone, Copy)]
struct Stats {
    mean: f64,   // mean |Δ| over the region, in 8-bit levels
    max: u8,     // max |Δ|
    over8: f64,  // % of region pixels with |Δ| > 8
    ink_err: f64, // Σ|Δ| / Σ qt alpha, %
}

fn stats(qt: &[u8], ours: &[u8], w: usize, rect: (usize, usize, usize, usize)) -> Stats {
    let (x0, y0, x1, y1) = rect;
    let (mut sum, mut max, mut over, mut ink, mut n) = (0f64, 0u8, 0usize, 0f64, 0usize);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = y * w + x;
            let d = (qt[i] as i32 - ours[i] as i32).unsigned_abs() as u8;
            sum += d as f64;
            max = max.max(d);
            if d > 8 {
                over += 1;
            }
            ink += qt[i] as f64;
            n += 1;
        }
    }
    Stats { mean: sum / n as f64, max, over8: 100.0 * over as f64 / n as f64, ink_err: 100.0 * sum / ink.max(1.0) }
}

fn parse_qt_dump(path: &Path) -> HashMap<String, QtText> {
    let mut map = HashMap::new();
    let mut cur: Option<String> = None;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        if let Some(rest) = line.strip_prefix("TEXT ") {
            let name = rest.split(" |").next().unwrap().trim().to_string();
            let imp = rest.split("implicit=").nth(1).unwrap().split_whitespace().next().unwrap();
            let (w, h) = imp.split_once('x').unwrap();
            let asc = rest.split("fm.asc=").nth(1).unwrap().split_whitespace().next().unwrap();
            map.insert(
                name.clone(),
                QtText { implicit_w: w.parse().unwrap(), implicit_h: h.parse().unwrap(), ascent: asc.parse().unwrap(), runs: vec![] },
            );
            cur = Some(name);
        } else if let Some(rest) = line.trim().strip_prefix("run font=") {
            let family = rest.split(" style=").next().unwrap().to_string();
            let pos = rest.split("pos=[").nth(1).unwrap().split(']').next().unwrap();
            let positions = pos.split_whitespace().map(|v| v.parse().unwrap()).collect();
            map.get_mut(cur.as_ref().unwrap()).unwrap().runs.push(QtRun { family, positions });
        }
    }
    map
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args[1] == "--dfcheck" {
        // textcmp --dfcheck <qt dump json> <family> <weight>
        let lib = ft::Library::init().unwrap();
        let face = resolve(&args[3], args[4].parse().unwrap(), &lib);
        let dump: Value = serde_json::from_str(&std::fs::read_to_string(&args[2]).unwrap()).unwrap();
        for o in dump.as_array().unwrap() {
            let g = o["glyph"].as_u64().unwrap() as u16;
            let df = make_df(&face, g).unwrap();
            let (qw, qh) = (o["w"].as_u64().unwrap() as usize, o["h"].as_u64().unwrap() as usize);
            let data: Vec<u8> = o["data"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
            let (mut sum, mut max, mut n, mut hist) = (0f64, 0i32, 0usize, [0usize; 4]);
            for y in 0..qh.min(df.h) {
                for x in 0..qw.min(df.w) {
                    let d = (data[y * qw + x] as i32 - df.data[y * df.w + x] as i32).abs();
                    sum += d as f64; max = max.max(d); n += 1;
                    hist[match d { 0 => 0, 1..=2 => 1, 3..=8 => 2, _ => 3 }] += 1;
                }
            }
            println!("glyph {g}: qt {qw}x{qh} ours {}x{} bbox qt ({:.3},{:.3}) ours ({:.3},{:.3}) mean|d| {:.2} max {max} hist0/1-2/3-8/>8 {:?}",
                df.w, df.h, o["bx"].as_f64().unwrap(), o["by"].as_f64().unwrap(), df.bx, df.by, sum / n as f64, hist);
            if std::env::var("DFDUMP").is_ok() {
                let row = qh / 2;
                println!("  qt  row{row}: {:?}", &data[row * qw..row * qw + qw]);
                println!("  our row{row}: {:?}", &df.data[row * df.w..row * df.w + df.w.min(qw)]);
            }
        }
        return;
    }
    let scene: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let qt_dump = parse_qt_dump(Path::new(&args[2]));
    let (qw, qh, qt_alpha) = read_png_alpha(Path::new(&args[3]));
    let out = PathBuf::from(&args[4]);
    let dpr: f64 = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(2.0);
    std::fs::create_dir_all(&out).unwrap();

    let samples: Vec<Sample> = scene["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Sample {
            name: s["name"].as_str().unwrap().into(),
            text: s["text"].as_str().unwrap().into(),
            px: s["px"].as_f64().unwrap() as f32,
            weight: s["weight"].as_i64().unwrap() as i32,
            x: s["x"].as_f64().unwrap(),
            y: s["y"].as_f64().unwrap(),
            native: s["mode"].as_str().unwrap() == "native",
            letter_spacing: s["letterSpacing"].as_f64().unwrap_or(0.0),
            width: s["width"].as_f64(),
        })
        .collect();

    let lib = ft::Library::init().unwrap();
    let mut face_cache: HashMap<i32, [Face; 2]> = HashMap::new();
    for s in &samples {
        face_cache.entry(s.weight).or_insert_with(|| [resolve(JB, s.weight, &lib), resolve(HM, s.weight, &lib)]);
    }
    let faces_for = |w: i32| face_cache[&w].clone();

    // ---- layout check against Qt's own glyph positions
    println!("# layout (logical px) vs Qt");
    let mut items = Vec::new();
    for s in &samples {
        let faces = faces_for(s.weight);
        let line = layout_elided(s, &faces);
        let q = &qt_dump[&s.name];
        let qpos: Vec<f64> = q.runs.iter().flat_map(|r| r.positions.clone()).collect();
        let qfam: Vec<String> = q.runs.iter().flat_map(|r| std::iter::repeat(r.family.clone()).take(r.positions.len())).collect();
        let max_dx = line.glyphs.iter().zip(qpos.iter()).map(|(g, q)| (g.x - q).abs()).fold(0.0, f64::max);
        let fam_ok = line.glyphs.len() == qpos.len()
            && line.glyphs.iter().zip(qfam.iter()).all(|(g, f)| (g.face == 0) == f.starts_with("JetBrains"));
        println!(
            "{:9} glyphs {:2}/{:2} fonts {} max|dx| {:.4}  ascent {:.4}/{:.4}  width {:.3}/{:.3}",
            s.name,
            line.glyphs.len(),
            qpos.len(),
            if fam_ok { "ok " } else { "BAD" },
            max_dx,
            line.ascent,
            q.ascent,
            line.width,
            q.implicit_w
        );
        items.push((s.clone(), line, faces));
    }
    for (s, _, f) in items.iter().take(0) {
        let _ = (s, f);
    }
    println!("# faces");
    for (w, f) in &face_cache {
        for face in f {
            println!(
                "weight {w}: {} index {} ({:?} {:?})",
                face.file.display(),
                face.index,
                face.typeface.family_name(),
                face.typeface.variation_design_position().map(|p| p.iter().map(|c| c.value).collect::<Vec<_>>())
            );
        }
    }

    let (w, h) = (qw, qh);
    let para = render_para(&samples, &faces_for, w as i32, h as i32, dpr as f32);
    let skia = render_skia(&items, w as i32, h as i32, dpr);
    let ftr = render_ft(&items, &lib, w, h, dpr);
    let df = render_df(&items, w, h, dpr);
    write_gray(&out.join("qt.png"), w, h, &qt_alpha);
    write_gray(&out.join("para.png"), w, h, &para);
    write_gray(&out.join("skia.png"), w, h, &skia);
    write_gray(&out.join("ft.png"), w, h, &ftr);
    write_gray(&out.join("df.png"), w, h, &df);

    println!("# diff vs Qt alpha (region = item rect + 3 px); mean/max in 8-bit levels, >8 = % pixels, ink = Σ|Δ|/Σqt");
    println!("{:9} {:6} | {:>22} | {:>22} | {:>22} | {:>22}", "sample", "mode", "para", "skia", "ft", "df");
    let mut totals: HashMap<&str, (f64, f64, usize)> = HashMap::new();
    for (s, _, _) in &items {
        let q = &qt_dump[&s.name];
        let x0 = (((s.x - 3.0) * dpr).floor().max(0.0)) as usize;
        let y0 = (((s.y - 3.0) * dpr).floor().max(0.0)) as usize;
        let x1 = (((s.x + q.implicit_w + 3.0) * dpr).ceil() as usize).min(w);
        let y1 = (((s.y + q.implicit_h + 3.0) * dpr).ceil() as usize).min(h);
        let rect = (x0, y0, x1, y1);
        let mut cells = Vec::new();
        for (name, plane) in [("para", &para), ("skia", &skia), ("ft", &ftr), ("df", &df)] {
            let st = stats(&qt_alpha, plane, w, rect);
            let key = if s.native { "native" } else { "qt" };
            let t = totals.entry(if name == "para" { if s.native { "para/native" } else { "para/qt" } } else { name }).or_default();
            let _ = key;
            t.0 += st.ink_err;
            t.1 = t.1.max(st.max as f64);
            t.2 += 1;
            cells.push(format!("{:5.2} {:3} {:5.1}% {:5.1}%", st.mean, st.max, st.over8, st.ink_err));
        }
        println!("{:9} {:6} | {} | {} | {} | {}", s.name, if s.native { "native" } else { "qt" }, cells[0], cells[1], cells[2], cells[3]);
    }
}
