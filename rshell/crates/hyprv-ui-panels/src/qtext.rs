//! Qt Quick text, reproduced exactly (docs/spec/text-rendering.md).
//!
//! Layout: HarfBuzz glyph ids (via skia textlayout), Qt itemisation (primary family,
//! then HarmonyOS Sans SC), FreeType design advances truncated to 1/64 px, line ascent
//! = max(primary, used fallbacks), implicit size per QQuickText, ElideRight per
//! QTextEngine::elidedText, absolute letterSpacing after every glyph, word wrap.
//!
//! Raster: NativeRendering = unhinted FreeType gray AA at device scale with Qt's
//! quarter-pixel horizontal subpixel buckets and vertex snapping (bit-exact against
//! Qt 6.12 in rshell/experiments/text); QtRendering = Qt's distance field (base 54 px,
//! radius 5 px, smoothstep thresholds), ≈1 % ink error.
//!
//! The engine (hyprv-ui) is integrating this pipeline; until then text nodes are
//! `Content::Picture`s that draw A8 coverage images at device-pixel-aligned positions.
//! A text node therefore needs its absolute logical origin and the output scale.
use freetype as ft;
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Node},
    skia::{
        self as sk,
        textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle as SkTextStyle, TypefaceFontProvider},
    },
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// `font.kerning` defaults to true: Qt keeps HarfBuzz's GPOS kerning on top of its own
/// (design) advances. Matters for proportional fonts (Inter, HarmonyOS Sans).
pub const QUIRK_KERNING: bool = true;
pub const BASE_FONT: &str = "JetBrainsMono Nerd Font";
pub const CJK_FALLBACK: &str = "HarmonyOS Sans SC";
/// QGuiApplication default font (fontconfig `sans-serif` → Inter on this machine).
pub const DEFAULT_FONT: &str = "sans-serif";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Render {
    /// `Text.NativeRendering`
    Native,
    /// Qt default `Text.QtRendering` (distance field)
    Qt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: &'static str,
    /// QML `font.pixelSize` (truncated to int like Qt).
    pub px: f32,
    pub weight: i32,
    pub letter_spacing: f32,
    pub render: Render,
}

impl Font {
    pub fn native(px: f32, weight: i32) -> Self {
        Self { family: BASE_FONT, px, weight, letter_spacing: 0., render: Render::Native }
    }
    pub fn qt(px: f32, weight: i32) -> Self {
        Self { family: BASE_FONT, px, weight, letter_spacing: 0., render: Render::Qt }
    }
    pub fn family(mut self, f: &'static str) -> Self {
        self.family = f;
        self
    }
    pub fn spacing(mut self, s: f32) -> Self {
        self.letter_spacing = s;
        self
    }
    fn px_int(&self) -> f64 {
        self.px.trunc() as f64
    }
    fn key(&self) -> String {
        format!("{}|{}|{}|{}|{:?}", self.family, self.px.trunc(), self.weight, self.letter_spacing, self.render)
    }
}

pub const NORMAL: i32 = 400;
pub const MEDIUM: i32 = 500;
pub const DEMIBOLD: i32 = 600;
pub const BOLD: i32 = 700;

/// A Text item: QML `text`, font, `width` (when set explicitly), elide/wrap/align.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub text: String,
    pub font: Font,
    pub color: Color,
    /// Explicit item width (enables elide, wrap and alignment).
    pub width: Option<f32>,
    pub elide: bool,
    pub wrap: bool,
    pub max_lines: Option<usize>,
    pub align: Align,
    /// `lineHeightMode: Text.FixedHeight` value.
    pub fixed_line_height: Option<f32>,
    /// `echoMode: TextInput.Password` (mask U+25CF).
    pub password: bool,
}

impl TextBox {
    pub fn new(text: impl Into<String>, font: Font, color: Color) -> Self {
        Self {
            text: text.into(),
            font,
            color,
            width: None,
            elide: false,
            wrap: false,
            max_lines: None,
            align: Align::Left,
            fixed_line_height: None,
            password: false,
        }
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn elide(mut self) -> Self {
        self.elide = true;
        self
    }
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
    pub fn lines(mut self, n: usize) -> Self {
        self.max_lines = Some(n);
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn line_height(mut self, h: f32) -> Self {
        self.fixed_line_height = Some(h);
        self
    }
    fn shown_text(&self) -> String {
        if self.password {
            "\u{25CF}".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }
    fn key(&self) -> String {
        format!(
            "{}\u{1}{}\u{1}{:?}\u{1}{}{}{:?}{:?}{:?}{}",
            // masked text only: a password never reaches the layout/glyph caches
            self.shown_text(),
            self.font.key(),
            self.width.map(|w| w.to_bits()),
            self.elide,
            self.wrap,
            self.max_lines,
            self.align,
            self.fixed_line_height.map(|h| h.to_bits()),
            self.password
        )
    }
}

// ------------------------------------------------------------------ faces
impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Face({})", self.family_name)
    }
}

pub(crate) struct Face {
    typeface: sk::Typeface,
    ft: ft::Face<&'static [u8]>,
    upem: f64,
    ascender: f64,
    descender: f64,
    family_name: String,
    embolden_weight: bool,
}

type SdfCache = HashMap<(String, u16), Option<Rc<Sdf>>>;

thread_local! {
    static FT: ft::Library = ft::Library::init().expect("FreeType");
    static FACES: RefCell<HashMap<(String, i32), Rc<Face>>> = RefCell::new(HashMap::new());
    static LAYOUTS: RefCell<HashMap<String, Rc<Layout>>> = RefCell::new(HashMap::new());
    static IMAGES: RefCell<HashMap<String, Option<Rc<Coverage>>>> = RefCell::new(HashMap::new());
    static SDFS: RefCell<SdfCache> = RefCell::new(HashMap::new());
}

fn face(family: &str, weight: i32) -> Rc<Face> {
    FACES.with(|faces| {
        if let Some(f) = faces.borrow().get(&(family.to_string(), weight)) {
            return f.clone();
        }
        // hyprv-ui's single fontconfig match + process-lifetime file mapping (PANELS-MEM-1):
        // the FreeType face and the shaping typeface borrow the engine's bytes instead of a
        // separate Skia mapping plus a heap copy of every font file.
        let mapped = hyprv_ui::text::mapped_font(family, weight)
            .or_else(|_| hyprv_ui::text::mapped_font(DEFAULT_FONT, weight))
            .expect("no usable font");
        let typeface = mapped.typeface.clone();
        let ft = FT.with(|lib| lib.new_memory_face2(mapped.bytes, mapped.index)).expect("FreeType face");
        let raw = ft.raw();
        let upem = raw.units_per_EM as f64;
        let ascender = raw.ascender as f64;
        let descender = -(raw.descender as f64);
        // QFontEngineFT::init fake bold: weight ≥ Bold, face lacks the bold style flag, not
        // fixed-width, and calculateActualWeight() < 700. Qt gets no named-instance index here
        // so it reads the OS/2 usWeightClass of the default instance (text-rendering.md §3.5).
        let bold_flag = raw.style_flags & ft::ffi::FT_STYLE_FLAG_BOLD as ft::ffi::FT_Long != 0;
        let fixed = raw.face_flags & ft::ffi::FT_FACE_FLAG_FIXED_WIDTH as ft::ffi::FT_Long != 0;
        let os2 = unsafe { ft::ffi::FT_Get_Sfnt_Table(raw as *const _ as *mut _, ft::ffi::ft_sfnt_os2) as *const ft::ffi::TT_OS2 };
        let actual = if os2.is_null() { 700 } else { unsafe { (*os2).usWeightClass } };
        let embolden_weight = !bold_flag && !fixed && actual < 700;
        let f = Rc::new(Face { family_name: typeface.family_name(), typeface, ft, upem, ascender, descender, embolden_weight });
        faces.borrow_mut().insert((family.to_string(), weight), f.clone());
        f
    })
}

fn trunc64(v: f64) -> f64 {
    (v * 64.0 + 1e-9).floor() / 64.0
}

impl Face {
    fn ascent(&self, px: f64) -> f64 {
        trunc64(self.ascender * px / self.upem)
    }
    fn descent(&self, px: f64) -> f64 {
        trunc64(self.descender * px / self.upem)
    }
    fn set_size(&self, px: f64) {
        let _ = self.ft.set_char_size(0, (px * 64.0) as isize, 0, 0);
        let mut m = ft::Matrix { xx: 0x10000, xy: 0, yx: 0, yy: 0x10000 };
        let mut d = ft::Vector { x: 0, y: 0 };
        self.ft.set_transform(&mut m, &mut d);
    }
    /// Design advance (linearHoriAdvance 16.16 >> 10, in 26.6) and the hinted metrics Qt
    /// uses for the right bearing of the last glyph.
    fn glyph_metrics(&self, glyph: u16, px: f64) -> (f64, f64) {
        self.set_size(px);
        if self.ft.load_glyph(glyph as u32, ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP).is_err() {
            return (0., 0.);
        }
        let slot = self.ft.glyph();
        let adv = (slot.linear_hori_advance() >> 10) as f64 / 64.0;
        let m = slot.metrics();
        let ink_right = (m.horiBearingX + m.width) as f64 / 64.0;
        // rightBearing = round(advance) − ceil(ink right); overflow = max(0, −rightBearing)
        let overflow = (ink_right.ceil() - adv.round()).max(0.);
        (adv, if m.width > 0 { overflow } else { 0. })
    }
}

// ------------------------------------------------------------------ layout

#[derive(Clone, Debug)]
struct Glyph {
    face: u8,
    id: u16,
    x: f64,
    cluster: usize, // char index in the line's text
}

#[derive(Clone, Debug)]
pub struct Line {
    glyphs: Vec<Glyph>,
    /// The line's text (wrapped or elided segment, trailing spaces included).
    pub text: String,
    /// Natural width (advances + last-glyph overflow, no trailing whitespace).
    pub width: f64,
    pub x: f64,
    pub y: f64,
}

impl Line {
    /// Glyph x positions (debugging and parity tools).
    pub fn glyph_positions(&self) -> Vec<f64> {
        self.glyphs.iter().map(|g| g.x).collect()
    }
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub ascent: f64,
    pub descent: f64,
    pub line_height: f64,
    /// QQuickText implicitWidth (unwrapped natural width incl. last-glyph overflow).
    pub implicit_width: f64,
    pub implicit_height: f64,
    /// Sum of advances of the unwrapped text (no overflow).
    pub advance: f64,
    faces: [Rc<Face>; 2],
    px: f64,
    embolden: bool,
}

/// One unwrapped run: glyphs with Qt advances.
struct Shaped {
    glyphs: Vec<Glyph>,
    advances: Vec<f64>,
    overflow_last: f64,
    used_fallback: bool,
}

fn shape(text: &str, font: &Font, faces: &[Rc<Face>; 2]) -> Shaped {
    let px = font.px_int();
    if text.is_empty() {
        return Shaped { glyphs: vec![], advances: vec![], overflow_last: 0., used_fallback: false };
    }
    let mut provider = TypefaceFontProvider::new();
    provider.register_typeface(faces[0].typeface.clone(), Some("qt-primary"));
    provider.register_typeface(faces[1].typeface.clone(), Some("qt-fallback"));
    let mut fc = FontCollection::new();
    fc.set_asset_font_manager(Some(provider.into()));
    fc.disable_font_fallback();
    let mut ts = SkTextStyle::new();
    ts.set_font_families(&["qt-primary", "qt-fallback"]);
    ts.set_font_size(px as f32);
    ts.set_font_style(sk::FontStyle::new(sk::font_style::Weight::from(font.weight), sk::font_style::Width::NORMAL, sk::font_style::Slant::Upright));
    let mut ps = ParagraphStyle::new();
    ps.set_text_style(&ts);
    let mut b = ParagraphBuilder::new(&ps, fc);
    b.push_style(&ts);
    b.add_text(text);
    let mut p = b.build();
    p.layout(1e9);
    // byte offset → char index
    let mut byte_to_char = vec![0usize; text.len() + 1];
    for (ci, (bi, _)) in text.char_indices().enumerate() {
        byte_to_char[bi] = ci;
    }
    byte_to_char[text.len()] = text.chars().count();
    let primary = faces[0].typeface.family_name();
    let mut glyphs = Vec::new();
    // GPOS kerning per glyph (HarfBuzz via skia): shaped advance − plain glyph advance.
    let mut kerns = Vec::new();
    p.visit(|_, info| {
        if let Some(info) = info {
            let face = if info.font().typeface().family_name() == primary { 0 } else { 1 };
            let starts = info.utf8_starts();
            let pos = info.positions();
            let mut widths = vec![0f32; info.glyphs().len()];
            info.font().get_widths(info.glyphs(), &mut widths);
            for (i, g) in info.glyphs().iter().enumerate() {
                let byte = starts.get(i).copied().unwrap_or(0) as usize;
                glyphs.push(Glyph { face, id: *g, x: 0., cluster: byte_to_char[byte.min(text.len())] });
                let kern = match pos.get(i + 1) {
                    Some(next) if QUIRK_KERNING => (next.x - pos[i].x - widths[i]) as f64,
                    _ => 0.,
                };
                // Qt's hb_font scale is 26.6: adjustments are whole 1/64 px.
                kerns.push(if kern.abs() < 1e-3 { 0. } else { (kern * 64.).round() / 64. });
            }
        }
    });
    let mut advances = Vec::with_capacity(glyphs.len());
    let mut pen = 0.;
    let mut overflow_last = 0.;
    let mut used_fallback = false;
    for (i, g) in glyphs.iter_mut().enumerate() {
        let (adv, of) = faces[g.face as usize].glyph_metrics(g.id, px);
        g.x = pen;
        let a = adv + kerns[i] + font.letter_spacing as f64;
        pen += a;
        advances.push(a);
        overflow_last = of;
        used_fallback |= g.face == 1;
    }
    Shaped { glyphs, advances, overflow_last, used_fallback }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}
fn no_break_before(c: char) -> bool {
    "，。、；：？！）」』】》〉,.;:?!)]}…·".contains(c)
}
fn no_break_after(c: char) -> bool {
    "（「『【《〈([{".contains(c)
}

/// Break opportunities (char index where a new line may start), UAX #14-lite: after
/// spaces, and between CJK ideographs (respecting basic kinsoku punctuation).
fn break_points(chars: &[char]) -> Vec<bool> {
    let mut ok = vec![false; chars.len() + 1];
    for i in 1..chars.len() {
        let (a, b) = (chars[i - 1], chars[i]);
        if (a == ' ' && b != ' ')
            || ((is_cjk(a) || is_cjk(b)) && a != ' ' && b != ' ' && !no_break_before(b) && !no_break_after(a))
        {
            ok[i] = true;
        }
    }
    ok
}

fn faces_for(font: &Font) -> [Rc<Face>; 2] {
    [face(font.family, font.weight), face(CJK_FALLBACK, font.weight)]
}

/// Width of a char range of a shaped run (advances; with the last glyph's overflow when
/// `with_overflow`).
fn range_width(sh: &Shaped, from: usize, to: usize) -> f64 {
    sh.glyphs.iter().zip(&sh.advances).filter(|(g, _)| g.cluster >= from && g.cluster < to).map(|(_, a)| a).sum()
}

pub fn layout(tb: &TextBox) -> Rc<Layout> {
    let key = tb.key();
    if let Some(l) = LAYOUTS.with(|c| c.borrow().get(&key).cloned()) {
        return l;
    }
    let l = Rc::new(layout_uncached(tb));
    LAYOUTS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(key, l.clone());
    });
    l
}

fn line_from(text: &str, font: &Font, faces: &[Rc<Face>; 2], used: &mut bool) -> (Vec<Glyph>, f64) {
    let trimmed = text.trim_end_matches(' ');
    let sh = shape(trimmed, font, faces);
    *used |= sh.used_fallback;
    let adv: f64 = sh.advances.iter().sum();
    (sh.glyphs, adv + sh.overflow_last)
}

/// QTextEngine::alignLine in 26.6 fixed point: `(line.width − textAdvance) / 2` with
/// QFixed integer division (truncation); textAdvance excludes the last glyph's overflow.
fn aligned_x(align: Align, box_w: f64, natural: f64, glyphs: &[Glyph], overflow_free: Option<f64>) -> f64 {
    let adv = overflow_free.unwrap_or(natural);
    let _ = glyphs;
    let w64 = (box_w * 64.).round() as i64;
    let a64 = (adv * 64.).round() as i64;
    match align {
        Align::Left => 0.,
        Align::Center => ((w64 - a64) / 2) as f64 / 64.,
        Align::Right => (w64 - a64) as f64 / 64.,
    }
}

fn layout_uncached(tb: &TextBox) -> Layout {
    let font = &tb.font;
    let px = font.px_int();
    let faces = faces_for(font);
    let text = tb.shown_text();
    let full = shape(&text, font, &faces);
    let advance: f64 = full.advances.iter().sum();
    let natural = advance + full.overflow_last;
    let mut used_fallback = full.used_fallback;
    let chars: Vec<char> = text.chars().collect();
    let mut lines: Vec<(String, Vec<Glyph>, f64)> = Vec::new();

    let width = tb.width.map(|w| w as f64);
    if let Some(w) = width.filter(|w| tb.wrap && natural > *w + 1e-9) {
        let bp = break_points(&chars);
        let mut start = 0usize;
        while start < chars.len() {
            // greedy: furthest break point whose line (without trailing spaces) fits
            let mut best = None;
            let mut end = start + 1;
            while end <= chars.len() {
                if end == chars.len() || bp[end] {
                    let seg: String = chars[start..end].iter().collect();
                    let seg_trim = seg.trim_end_matches(' ');
                    let cnt = seg_trim.chars().count();
                    let wseg = range_width(&full, start, start + cnt);
                    if wseg <= w + 1e-9 || best.is_none() {
                        best = Some(end);
                        if wseg > w + 1e-9 {
                            break; // a single overlong word
                        }
                    } else {
                        break;
                    }
                }
                end += 1;
            }
            let end = best.unwrap_or(chars.len());
            let seg: String = chars[start..end].iter().collect();
            let (g, wl) = line_from(&seg, font, &faces, &mut used_fallback);
            lines.push((seg, g, wl));
            start = end;
        }
        if let Some(maxl) = tb.max_lines {
            if lines.len() > maxl {
                lines.truncate(maxl);
                if tb.elide {
                    // multi-line elide: the last visible line takes the rest of the text
                    let consumed: usize = lines[..maxl - 1].iter().map(|l| l.0.chars().count()).sum();
                    let rest: String = chars[consumed..].iter().collect();
                    let el = elided(&rest, font, &faces, w);
                    let (g, wl) = line_from(&el, font, &faces, &mut used_fallback);
                    lines[maxl - 1] = (el, g, wl);
                }
            }
        }
    } else if let Some(w) = width.filter(|w| tb.elide && natural > *w + 1e-9 && chars.len() > 1) {
        let el = elided(&text, font, &faces, w);
        let (g, wl) = line_from(&el, font, &faces, &mut used_fallback);
        lines.push((el, g, wl));
    } else {
        lines.push((text.clone(), full.glyphs.clone(), natural));
    }

    let mut ascent = faces[0].ascent(px);
    let mut descent = faces[0].descent(px);
    if used_fallback {
        ascent = ascent.max(faces[1].ascent(px));
        descent = descent.max(faces[1].descent(px));
    }
    let font_height = (ascent + descent).ceil();
    let line_height = tb.fixed_line_height.map(|h| h as f64).unwrap_or(font_height);
    let box_w = width.unwrap_or(natural);
    let out: Vec<Line> = lines
        .into_iter()
        .enumerate()
        .map(|(i, (seg, glyphs, w))| {
            let x = if width.is_some() {
                let adv = shape(seg.trim_end_matches(' '), font, &faces).advances.iter().sum::<f64>();
                aligned_x(tb.align, box_w, w, &glyphs, Some(adv))
            } else {
                match tb.align {
                    Align::Left => 0.,
                    Align::Center => (box_w - w) / 2.,
                    Align::Right => box_w - w,
                }
            };
            Line { glyphs, width: w, x, y: i as f64 * line_height, text: seg }
        })
        .collect();
    let n = out.len().max(1) as f64;
    Layout {
        implicit_width: natural,
        implicit_height: n * line_height,
        lines: out,
        ascent,
        descent,
        line_height,
        advance,
        faces,
        px,
        embolden: font.weight >= 700 && px < 64.,
    }
}

/// QTextEngine::elidedText(ElideRight): ellipsis from the primary font; keep graphemes
/// while the running width < width − ellipsis; the grapheme reaching the limit is dropped.
fn elided(text: &str, font: &Font, faces: &[Rc<Face>; 2], width: f64) -> String {
    let px = font.px_int();
    let ell_glyph = faces[0].typeface.unichar_to_glyph('\u{2026}' as i32);
    let (ell, ell_text) = if ell_glyph != 0 {
        (faces[0].glyph_metrics(ell_glyph, px).0 + font.letter_spacing as f64, "\u{2026}".to_string())
    } else {
        let dot = faces[0].typeface.unichar_to_glyph('.' as i32);
        ((faces[0].glyph_metrics(dot, px).0 + font.letter_spacing as f64) * 3., "...".to_string())
    };
    let available = width - ell;
    if available < 0. {
        return String::new();
    }
    let sh = shape(text, font, faces);
    let chars: Vec<char> = text.chars().collect();
    let (mut cur, mut next) = (0.0, 0usize);
    let mut pos;
    loop {
        pos = next;
        next += 1;
        cur += range_width(&sh, pos, next);
        if !(next < chars.len() && cur < available) {
            break;
        }
    }
    chars[..pos].iter().collect::<String>() + &ell_text
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// implicitWidth
    pub width: f32,
    /// implicitHeight
    pub height: f32,
    pub advance: f32,
}

/// QQuickText implicit size of a single-line label.
pub fn measure(text: &str, font: &Font) -> Metrics {
    let l = layout(&TextBox::new(text, font.clone(), Color::default()));
    Metrics { width: l.implicit_width as f32, height: l.implicit_height as f32, advance: l.advance as f32 }
}

/// Height of a wrapped text at a given width (`contentHeight`).
pub fn wrapped_height(tb: &TextBox) -> f32 {
    layout(tb).implicit_height as f32
}

// ------------------------------------------------------------------ raster

struct Coverage {
    image: sk::Image,
    /// Device-pixel offset of the image relative to the snapped item origin.
    left: i64,
    top: i64,
}

struct Plane {
    w: usize,
    h: usize,
    x0: i64,
    y0: i64,
    a: Vec<f32>,
}
impl Plane {
    fn over(&mut self, x: i64, y: i64, c: f32) {
        let (xi, yi) = (x - self.x0, y - self.y0);
        if xi < 0 || yi < 0 || xi as usize >= self.w || yi as usize >= self.h || c <= 0. {
            return;
        }
        let i = yi as usize * self.w + xi as usize;
        self.a[i] = c + self.a[i] * (1. - c);
    }
    fn image(&self) -> Option<sk::Image> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let bytes: Vec<u8> = self.a.iter().map(|v| (v.clamp(0., 1.) * 255.).round() as u8).collect();
        let info = sk::ImageInfo::new((self.w as i32, self.h as i32), sk::ColorType::Alpha8, sk::AlphaType::Premul, None);
        sk::images::raster_from_data(&info, sk::Data::new_copy(&bytes), self.w)
    }
}

struct GlyphBitmap {
    left: i64,
    top: i64,
    w: usize,
    h: usize,
    pitch: usize,
    buf: Vec<u8>,
}

/// The user's fontconfig (hintstyle=hintslight, autohint=true) applies when Qt is not
/// DPR-scaling (scale-1 outputs): glyphs are light-autohinted, advances stay design
/// metrics (HintLight ⇒ shouldUseDesignMetrics).
pub const QUIRK_FONTCONFIG_HINTSLIGHT_AT_DPR1: bool = true;

/// QFontEngineFT::loadGlyph for a scaled glyph cache: HintNone, matrix = dpr, delta =
/// subpixel bucket, NO_HINTING|NO_BITMAP, optional FT_GlyphSlot_Embolden, NORMAL render.
fn native_glyph(face: &Face, glyph: u16, px: f64, dpr: f64, sub: f64, embolden: bool) -> Option<GlyphBitmap> {
    let _ = face.ft.set_char_size(0, (px * 64.0) as isize, 0, 0);
    let k = (dpr * 65536.0).round() as ft::ffi::FT_Fixed;
    let mut m = ft::Matrix { xx: k, xy: 0, yx: 0, yy: k };
    let mut d = ft::Vector { x: (sub * 64.0) as ft::ffi::FT_Pos, y: 0 };
    face.ft.set_transform(&mut m, &mut d);
    // qfontconfigdatabase.cpp defaultHintStyleFromMatch: HintNone while DPR scaling is
    // active, otherwise fontconfig's hintstyle (hintslight → HintLight) with its autohint.
    let flags = if dpr == 1.0 && QUIRK_FONTCONFIG_HINTSLIGHT_AT_DPR1 {
        ft::face::LoadFlag::TARGET_LIGHT | ft::face::LoadFlag::NO_BITMAP | ft::face::LoadFlag::FORCE_AUTOHINT
    } else {
        ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP | ft::face::LoadFlag::FORCE_AUTOHINT
    };
    face.ft.load_glyph(glyph as u32, flags).ok()?;
    let slot = face.ft.glyph();
    if embolden && face.embolden_weight {
        unsafe { ft::ffi::FT_GlyphSlot_Embolden(slot.raw() as *const _ as *mut _) };
    }
    slot.render_glyph(if dpr == 1.0 && QUIRK_FONTCONFIG_HINTSLIGHT_AT_DPR1 { ft::RenderMode::Light } else { ft::RenderMode::Normal }).ok()?;
    let bm = slot.bitmap();
    let r = GlyphBitmap {
        left: slot.bitmap_left() as i64,
        top: slot.bitmap_top() as i64,
        w: bm.width() as usize,
        h: bm.rows() as usize,
        pitch: bm.pitch().unsigned_abs() as usize,
        buf: bm.buffer().to_vec(),
    };
    let mut m = ft::Matrix { xx: 0x10000, xy: 0, yx: 0, yy: 0x10000 };
    let mut d = ft::Vector { x: 0, y: 0 };
    face.ft.set_transform(&mut m, &mut d);
    Some(r)
}

/// Placement of one native glyph (QSGTextMaskMaterial::populate) in item-local device
/// pixels: (integer glyph origin x, subpixel bucket, baseline row).
fn native_place(local_x: f64, local_baseline: f64, dpr: f64) -> (i64, f64, i64) {
    let fx = (local_x * 64.0).round() / 64.0;
    let dx = fx * dpr;
    let mut sub = 0.0;
    if dx != 0.0 {
        let v = (dx - dx.floor()) + 1.0 / 64.0;
        sub = (v * 4.0).floor() / 4.0;
    }
    let fy = (local_baseline * 64.0).round() / 64.0;
    (dx.floor() as i64, sub, (fy * dpr).round() as i64)
}

fn render_native(l: &Layout, dpr: f64) -> Option<Coverage> {
    let mut bitmaps = Vec::new();
    for line in &l.lines {
        for g in &line.glyphs {
            let (ix, sub, by) = native_place(line.x + g.x, line.y + l.ascent, dpr);
            if let Some(b) = native_glyph(&l.faces[g.face as usize], g.id, l.px, dpr, sub, l.embolden) {
                bitmaps.push((ix + b.left, by - b.top, b));
            }
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for (x, y, b) in &bitmaps {
        if b.w == 0 || b.h == 0 {
            continue;
        }
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(x + b.w as i64);
        y1 = y1.max(y + b.h as i64);
    }
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let mut p = Plane { w: (x1 - x0) as usize, h: (y1 - y0) as usize, x0, y0, a: vec![0.; ((x1 - x0) * (y1 - y0)) as usize] };
    for (x, y, b) in &bitmaps {
        for yy in 0..b.h {
            for xx in 0..b.w {
                let c = b.buf[yy * b.pitch + xx] as f32 / 255.;
                p.over(x + xx as i64, y + yy as i64, c);
            }
        }
    }
    Some(Coverage { image: p.image()?, left: x0, top: y0 })
}

/// Distance field of one glyph as QSGDistanceFieldGlyphCache builds it ([`crate::qdf`]):
/// 8-bit, 255 inside, 0 outside, texel 0 = bbox top-left − 5 base px.
struct Sdf {
    w: usize,
    h: usize,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
    data: Vec<u8>,
}

fn sdf(face: &Face, glyph: u16, embolden: bool) -> Option<Rc<Sdf>> {
    // Faces live for the whole process in FACES, so the address is a stable identity.
    let embolden = embolden && face.embolden_weight;
    let key = (format!("{:p}{}", face as *const Face, embolden), glyph);
    if let Some(s) = SDFS.with(|c| c.borrow().get(&key).cloned()) {
        return s;
    }
    let s = crate::qdf::field(&face.ft, glyph as u32, embolden).map(|f| Rc::new(Sdf { w: f.w, h: f.h, bx: f.bx, by: f.by, bw: f.bw, bh: f.bh, data: f.data }));
    SDFS.with(|c| c.borrow_mut().insert(key, s.clone()));
    s
}

/// Field bytes per glyph of a layout (parity tools): (glyph id, width, height, bytes).
pub fn df_fields(tb: &TextBox) -> Vec<(u16, usize, usize, Vec<u8>)> {
    let l = layout(tb);
    let mut v = Vec::new();
    for line in &l.lines {
        for g in &line.glyphs {
            if let Some(f) = sdf(&l.faces[g.face as usize], g.id, l.embolden) {
                v.push((g.id, f.w, f.h, f.data.clone()));
            }
        }
    }
    v
}

/// qsgdistancefieldglyphnode_p.cpp thresholdFunc / spreadFunc.
pub fn df_alpha_range(px: f64, dpr: f64) -> (f64, f64) {
    let s = px / 54.0 * dpr;
    let base = 0.5 - ((s.clamp(0.15, 0.3) - 0.15) / (0.3 - 0.15) * -0.065 + 0.065);
    let range = 0.06 / s;
    ((base - range).max(0.0), (base + range).min(1.0))
}

fn sample(df: &Sdf, u: f64, v: f64) -> f64 {
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

/// Distance-field text: no snapping; `phase` = fractional device offset of the item origin.
fn render_df(l: &Layout, dpr: f64, phase: (f64, f64)) -> Option<Coverage> {
    let fs = l.px / 54.0;
    let (amin, amax) = df_alpha_range(l.px, dpr);
    let mut quads = Vec::new();
    let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for line in &l.lines {
        for g in &line.glyphs {
            let face = &l.faces[g.face as usize];
            let Some(df) = sdf(face, g.id, l.embolden) else { continue };
            // device coords relative to the floor of the item origin
            let ox = phase.0 + (line.x + g.x) * dpr;
            let oy = phase.1 + (line.y + l.ascent) * dpr;
            // QSGDistanceFieldGlyphNode::updateGeometry: quad = bbox ± margin (2 item px,
            // capped at the 5 px field radius); texel 0 stays at bbox − 5.
            let tm = (2.0 / fs).min(5.0);
            let fx0 = ox + (df.bx - 5.0) * fs * dpr;
            let fy0 = oy + (df.by - 5.0) * fs * dpr;
            let qx0 = ox + (df.bx - tm) * fs * dpr;
            let qy0 = oy + (df.by - tm) * fs * dpr;
            let qx1 = ox + (df.bx + df.bw + tm) * fs * dpr;
            let qy1 = oy + (df.by + df.bh + tm) * fs * dpr;
            x0 = x0.min(qx0.floor() as i64);
            y0 = y0.min(qy0.floor() as i64);
            x1 = x1.max(qx1.ceil() as i64);
            y1 = y1.max(qy1.ceil() as i64);
            quads.push((df, fx0, fy0, qx0, qy0, qx1, qy1));
        }
    }
    if quads.is_empty() || x0 >= x1 || y0 >= y1 {
        return None;
    }
    let mut p = Plane { w: (x1 - x0) as usize, h: (y1 - y0) as usize, x0, y0, a: vec![0.; ((x1 - x0) * (y1 - y0)) as usize] };
    for (df, fx0, fy0, qx0, qy0, qx1, qy1) in quads {
        for py in qy0.floor() as i64..qy1.ceil() as i64 {
            for px in qx0.floor() as i64..qx1.ceil() as i64 {
                let cx = px as f64 + 0.5;
                let cy = py as f64 + 0.5;
                if cx < qx0 || cx >= qx1 || cy < qy0 || cy >= qy1 {
                    continue;
                }
                let u = (cx - fx0) / (fs * dpr);
                let v = (cy - fy0) / (fs * dpr);
                let d = sample(&df, u, v);
                let t = ((d - amin) / (amax - amin)).clamp(0.0, 1.0);
                p.over(px, py, (t * t * (3.0 - 2.0 * t)) as f32);
            }
        }
    }
    Some(Coverage { image: p.image()?, left: x0, top: y0 })
}

fn coverage(tb: &TextBox, dpr: f64, phase: (f64, f64)) -> Option<Rc<Coverage>> {
    let key = match tb.font.render {
        Render::Native => format!("{}\u{2}{}", tb.key(), dpr),
        Render::Qt => format!("{}\u{2}{}\u{2}{}\u{2}{}", tb.key(), dpr, (phase.0 * 64.).round(), (phase.1 * 64.).round()),
    };
    if let Some(c) = IMAGES.with(|c| c.borrow().get(&key).cloned()) {
        return c;
    }
    let l = layout(tb);
    let c = match tb.font.render {
        Render::Native => render_native(&l, dpr),
        Render::Qt => render_df(&l, dpr, phase),
    }
    .map(Rc::new);
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 2048 {
            cache.clear();
        }
        cache.insert(key, c.clone());
    });
    c
}

/// QQuickText hands its colour to the glyph node as a QRgb: 8 bits per channel, unpremultiplied
/// (the material premultiplies and applies inherited opacity in float).
pub const QUIRK_TEXT_COLOR_QRGB8: bool = true;
fn qrgb8(c: Color) -> sk::Color4f {
    let q = |v: f32| if QUIRK_TEXT_COLOR_QRGB8 { (v.clamp(0., 1.) * 255.).round() / 255. } else { v };
    sk::Color4f::new(q(c.r), q(c.g), q(c.b), q(c.a))
}

/// Single-line NativeRendering labels become engine `Content::Text` nodes in Qt GL mode:
/// the engine rasterises the same Qt glyph masks and, on the GL path, draws them with
/// QQuickText's glyph material (QRgb8 colour, premultiplied, then inherited opacity) as
/// the bar does. Wrapped, fixed-height and password texts keep the coverage picture.
pub const ENGINE_NATIVE_TEXT: bool = true;

/// Build a text node. `frame` is relative to the parent; `abs` is the frame's absolute
/// logical origin on the surface (needed for Qt's device-pixel snapping); `dpr` = output
/// scale. The node's size is the item size (width = explicit width or implicitWidth).
pub fn node(tb: &TextBox, frame: Rect, abs: (f32, f32), dpr: f32) -> Node {
    let mut n = Node::new(frame, Content::Group);
    if tb.text.is_empty() || tb.color.a <= 0. {
        return n;
    }
    if ENGINE_NATIVE_TEXT && tb.font.render == Render::Native && tb.wrap && tb.fixed_line_height.is_none() && !tb.password {
        // Qt's line breaking (layout), each line an engine single-line text item
        let l = layout(tb);
        let w = tb.width.unwrap_or(l.implicit_width as f32);
        for line in &l.lines {
            let mut one = tb.clone();
            one.wrap = false;
            one.elide = false;
            one.max_lines = None;
            one.text = line.text.trim_end_matches(' ').to_string();
            // the engine centres by natural width; this width reproduces Qt's 26.6
            // overflow-free offset (line.x) at the same snapped item origin
            let (lw, align) = if line.x > 0. { ((2. * line.x + line.width) as f32, Align::Center) } else { (w, Align::Left) };
            one.width = Some(lw);
            one.align = align;
            n.children.push(node(&one, Rect::new(0., line.y as f32, lw, l.line_height as f32), (abs.0, abs.1 + line.y as f32), dpr));
        }
        return n;
    }
    if ENGINE_NATIVE_TEXT && tb.font.render == Render::Native && !tb.wrap && tb.fixed_line_height.is_none() && !tb.password {
        n.content = Content::Text(hyprv_ui::scene::Text {
            render_type: hyprv_ui::scene::RenderType::NativeRendering,
            text: tb.text.clone(),
            families: vec![tb.font.family.into(), CJK_FALLBACK.into()],
            size: tb.font.px,
            weight: tb.font.weight,
            color: tb.color,
            letter_spacing: tb.font.letter_spacing,
            max_lines: Some(1),
            elide: tb.elide,
            align: match tb.align { Align::Left => sk::textlayout::TextAlign::Left, Align::Center => sk::textlayout::TextAlign::Center, Align::Right => sk::textlayout::TextAlign::Right },
            ..Default::default()
        });
        n.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
        return n;
    }
    node_picture(tb, frame, abs, dpr)
}
fn node_picture(tb: &TextBox, frame: Rect, abs: (f32, f32), dpr: f32) -> Node {
    let dpr64 = dpr as f64;
    let mut n = Node::new(frame, Content::Group);
    if tb.text.is_empty() || tb.color.a <= 0. {
        return n;
    }
    let ax = abs.0 as f64 * dpr64;
    let ay = abs.1 as f64 * dpr64;
    let (cov, ox, oy) = match tb.font.render {
        // Native: textmask.vert snaps the item origin: floor(x·dpr + 0.5)
        Render::Native => (coverage(tb, dpr64, (0., 0.)), (ax + 0.5).floor(), (ay + 0.5).floor()),
        Render::Qt => (coverage(tb, dpr64, (ax - ax.floor(), ay - ay.floor())), ax.floor(), ay.floor()),
    };
    let Some(cov) = cov else { return n };
    let left = (ox + cov.left as f64 - ax) / dpr64;
    let top = (oy + cov.top as f64 - ay) / dpr64;
    let w = cov.image.width() as f64 / dpr64;
    let h = cov.image.height() as f64 / dpr64;
    let bounds = sk::Rect::from_xywh(left as f32, top as f32, w as f32, h as f32);
    let mut rec = sk::PictureRecorder::new();
    let canvas = rec.begin_recording(bounds, false);
    let mut paint = sk::Paint::default();
    paint.set_color4f(qrgb8(tb.color), None);
    canvas.draw_image_rect(&cov.image, None, bounds, &paint);
    if let Some(pic) = rec.finish_recording_as_picture(Some(&bounds)) {
        if tb.font.render == Render::Native {
            NATIVE_PICS.with(|m| {
                let mut m = m.borrow_mut();
                if m.len() > 2048 {
                    m.clear();
                }
                m.insert(pic.unique_id(), NativePic { image: cov.image.clone(), left: cov.left, top: cov.top, color: tb.color });
            });
        }
        n.content = Content::Picture(pic);
    }
    n
}

/// Keys of the long-lived text caches (layouts, glyph coverage), for leak tests: a
/// password must never appear here.
#[doc(hidden)]
pub fn debug_cache_keys() -> Vec<String> {
    let mut v: Vec<String> = LAYOUTS.with(|c| c.borrow().keys().cloned().collect());
    v.extend(IMAGES.with(|c| c.borrow().keys().cloned().collect::<Vec<_>>()));
    v
}

struct NativePic {
    image: sk::Image,
    left: i64,
    top: i64,
    color: Color,
}

thread_local! {
    static NATIVE_PICS: RefCell<HashMap<u32, NativePic>> = RefCell::new(HashMap::new());
}

/// A native text picture re-placed under a scaled item transform (textmask.vert): the
/// glyph quad's corners go through the item matrix, then `floor(x·dpr + 0.5)`; the glyph
/// texture is sampled with Nearest filtering. `origin` = the text item's window
/// position, `s` = accumulated uniform scale. Returns a picture in item-local units.
pub fn transformed_picture(pic: &sk::Picture, origin: (f32, f32), s: f32, dpr: f32) -> Option<sk::Picture> {
    NATIVE_PICS.with(|m| {
        let m = m.borrow();
        let np = m.get(&pic.unique_id())?;
        let (w, h) = (np.image.width() as f32, np.image.height() as f32);
        let snap = |o: f32, local_px: f32| ((o + s * local_px / dpr) * dpr + 0.5).floor();
        let (x0, x1) = (snap(origin.0, np.left as f32), snap(origin.0, np.left as f32 + w));
        let (y0, y1) = (snap(origin.1, np.top as f32), snap(origin.1, np.top as f32 + h));
        let (qw, qh) = ((x1 - x0).max(0.) as usize, (y1 - y0).max(0.) as usize);
        if qw == 0 || qh == 0 {
            return None;
        }
        let info = sk::ImageInfo::new((np.image.width(), np.image.height()), sk::ColorType::Alpha8, sk::AlphaType::Premul, None);
        let mut src = vec![0u8; np.image.width() as usize * np.image.height() as usize];
        np.image.read_pixels(&info, &mut src, np.image.width() as usize, (0, 0), sk::image::CachingHint::Allow).then_some(())?;
        let mut dst = vec![0u8; qw * qh];
        for y in 0..qh {
            let ty = (((y as f32 + 0.5) / qh as f32 * h) as usize).min(h as usize - 1);
            for x in 0..qw {
                let tx = (((x as f32 + 0.5) / qw as f32 * w) as usize).min(w as usize - 1);
                dst[y * qw + x] = src[ty * w as usize + tx];
            }
        }
        let dinfo = sk::ImageInfo::new((qw as i32, qh as i32), sk::ColorType::Alpha8, sk::AlphaType::Premul, None);
        let img = sk::images::raster_from_data(&dinfo, sk::Data::new_copy(&dst), qw)?;
        let bounds = sk::Rect::from_xywh((x0 / dpr - origin.0) / s, (y0 / dpr - origin.1) / s, qw as f32 / dpr / s, qh as f32 / dpr / s);
        let mut rec = sk::PictureRecorder::new();
        let canvas = rec.begin_recording(bounds, false);
        let mut paint = sk::Paint::default();
        paint.set_color4f(qrgb8(np.color), None);
        canvas.draw_image_rect(&img, None, bounds, &paint);
        rec.finish_recording_as_picture(Some(&bounds))
    })
}

/// Convenience: the item size Qt reports (explicit width or implicitWidth, implicitHeight).
pub fn item_size(tb: &TextBox) -> (f32, f32) {
    let l = layout(tb);
    (tb.width.unwrap_or(l.implicit_width as f32), l.implicit_height as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn w(text: &str, px: f32) -> f32 {
        measure(text, &Font::native(px, BOLD)).width
    }
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.006, "{a} vs {b}");
    }
    /// components.md §0.2 measured table (Qt 6.12, this machine).
    #[test]
    fn qt_measured_widths() {
        close(w("\u{F028}", 16.), 18.59);
        close(w("\u{F303}", 17.), 17.19);
        close(w("\u{F027}", 16.), 12.59);
        close(w("\u{F05B3}", 17.), 13.19);
        close(w("\u{F466}", 16.), 16.59);
        close(w("\u{F0A2}", 14.), 12.39);
        close(w("\u{F009B}", 14.), 13.39);
        close(w("\u{F04B}", 16.), 11.59);
        close(w("\u{F023}", 16.), 12.59);
        close(w("\u{F023}", 11.), 8.59);
        close(w("\u{F0928}", 20.), 19.98);
        close(w("\u{F00AF}", 20.), 11.98);
        close(w("\u{F040A}", 31.), 18.59);
        close(w("\u{F00DF}", 21.08), 19.59);
        close(w("\u{F0EC3}", 15.), 13.00);
        close(w("12:34", 16.), 47.97);
        close(w("WIFI", 11.), 26.38);
        close(w("Close", 13.), 38.98);
        close(w("Turn Off", 13.), 62.38);
        close(w("C1 12%", 13.), 46.78);
        close(w("Bluetooth", 17.), 91.69);
        close(w("Network", 17.), 71.31);
        close(w("系统资源", 15.), 60.);
        close(w("＋ 创建日程", 16.), 89.59);
    }
    #[test]
    fn heights() {
        for (px, h) in [(8., 11.), (9., 12.), (10., 14.), (11., 15.), (12., 16.), (13., 18.), (14., 19.), (15., 20.), (16., 22.), (17., 23.), (20., 27.), (21.08, 28.), (24., 32.), (26., 35.), (31., 41.)] {
            assert_eq!(measure("x", &Font::native(px, BOLD)).height, h, "px {px}");
        }
    }
    #[test]
    fn elide_and_wrap() {
        let tb = TextBox::new("Firefox — a very long window title that must elide", Font::native(16., BOLD), Color::default()).width(150.5).elide();
        assert_eq!(layout(&tb).lines[0].glyphs.len(), 15);
        let tb = TextBox::new("已接通电源，当前未充电", Font::native(12., DEMIBOLD), Color::default()).width(100.).elide();
        assert_eq!(layout(&tb).lines[0].glyphs.len(), 8);
        let tb = TextBox::new("Ethernet is active. No visible Wi-Fi networks right now.", Font::native(13., NORMAL), Color::default()).width(336.).wrap();
        let l = layout(&tb);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.implicit_height, 36.);
    }
    #[test]
    fn spacing() {
        let m = measure("12:34", &Font::native(16., BOLD).spacing(3.));
        close(m.width, 62.969);
    }
}
