//! Qt text shaping, metrics, FreeType atlas, and distance-field emulation.
//! Adapted from experiments/text/skia-text; see docs/spec/text-rendering.md.
use crate::{
    render::RenderError,
    scene::{RenderType, Text},
};
use freetype as ft;
use sk::textlayout::{
    FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle, TypefaceFontProvider,
};
use skia_safe as sk;
use std::{
    collections::HashMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::PathBuf,
};
const JB: &str = "JetBrainsMono Nerd Font";
const HM: &str = "HarmonyOS Sans SC";
// Skia's FontMgr::new/default creates a system manager and enumerates fonts.
// Qt has one application font database; reuse the startup manager for resolved
// typefaces and wrapped paragraphs instead of doing that work on first input.
thread_local! { static FONT_MANAGER: sk::FontMgr = sk::FontMgr::default(); }
/// Shared UI-thread system font database; clones avoid re-enumerating fonts.
pub fn font_manager() -> sk::FontMgr { FONT_MANAGER.with(Clone::clone) }

#[derive(Clone, Debug)]
struct Sample {
    text: String,
    px: f32,
    weight: i32,
    letter_spacing: f64,
}
/// Font face resolved exactly like fontconfig resolves it for Qt.
#[derive(Clone)]
struct Face {
    /// Unique per resolved face; atlas/field keys use it instead of the file path.
    id: u32,
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

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcInit() -> c_int;
    fn FcPatternCreate() -> *mut c_void;
    fn FcPatternDestroy(p: *mut c_void);
    fn FcPatternAddString(p: *mut c_void, name: *const c_char, s: *const u8) -> c_int;
    fn FcPatternAddInteger(p: *mut c_void, name: *const c_char, n: c_int) -> c_int;
    fn FcConfigSubstitute(config: *mut c_void, p: *mut c_void, kind: c_int) -> c_int;
    fn FcDefaultSubstitute(p: *mut c_void);
    fn FcFontMatch(config: *mut c_void, p: *mut c_void, result: *mut c_int) -> *mut c_void;
    fn FcPatternGetString(
        p: *mut c_void,
        name: *const c_char,
        n: c_int,
        out: *mut *mut u8,
    ) -> c_int;
    fn FcPatternGetInteger(p: *mut c_void, name: *const c_char, n: c_int, out: *mut c_int)
        -> c_int;
}
fn font_match(family: &str, weight: i32) -> Result<(PathBuf, isize), RenderError> {
    let family = CString::new(family).map_err(|e| RenderError::Font(e.to_string()))?;
    // SAFETY: Fontconfig patterns are owned locally and destroyed after copying all results.
    unsafe {
        FcInit();
        let p = FcPatternCreate();
        if p.is_null() {
            return Err(RenderError::Skia);
        }
        FcPatternAddString(p, c"family".as_ptr(), family.as_ptr() as *const u8);
        FcPatternAddInteger(p, c"weight".as_ptr(), fc_weight(weight));
        FcConfigSubstitute(std::ptr::null_mut(), p, 0);
        FcDefaultSubstitute(p);
        let mut result = 0;
        let matched = FcFontMatch(std::ptr::null_mut(), p, &mut result);
        FcPatternDestroy(p);
        if matched.is_null() {
            return Err(RenderError::Font("no font match".into()));
        }
        let mut file = std::ptr::null_mut();
        let mut index = 0;
        let found = FcPatternGetString(matched, c"file".as_ptr(), 0, &mut file) == 0;
        FcPatternGetInteger(matched, c"index".as_ptr(), 0, &mut index);
        let path = if found {
            Some(PathBuf::from(
                CStr::from_ptr(file as *const c_char)
                    .to_string_lossy()
                    .into_owned(),
            ))
        } else {
            None
        };
        FcPatternDestroy(matched);
        path.map(|p| (p, index as isize))
            .ok_or_else(|| RenderError::Font("font match has no file".into()))
    }
}
/// A fontconfig match backed by the engine's shared, immutable file mapping.
/// `index` includes FreeType's named-instance bits; do not discard its high bits
/// when creating a FreeType face. Request this on the UI thread, like other text
/// APIs. The bytes remain valid for the process lifetime and need no heap copy.
#[derive(Clone, Debug)]
pub struct FontMapping {
    pub file: PathBuf,
    pub index: isize,
    pub bytes: &'static [u8],
}
/// Resolve Qt-style family/weight selection and borrow the same mapped bytes
/// used by engine Skia and FreeType faces, including all variable-font weights.
pub fn font_mapping(family: &str, weight: i32) -> Result<FontMapping, RenderError> {
    let (file, index) = font_match(family, weight)?;
    let bytes = mapped_font_bytes(&file)
        .ok_or_else(|| RenderError::Font(format!("cannot map {}", file.display())))?;
    Ok(FontMapping { file, index, bytes })
}
/// One process-lifetime mapping per resolved font file. Both Skia's direct
/// streams and FreeType named instances borrow these same bytes: opening the
/// file independently for each weight counts its resident pages repeatedly in RSS.
fn mapped_font_bytes(file: &std::path::Path) -> Option<&'static [u8]> {
    thread_local! {static MAPPED: std::cell::RefCell<HashMap<PathBuf, (*const u8, usize)>> = Default::default();}
    let (ptr, len) = MAPPED.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(v) = m.get(file) { return Some(*v); }
        let data = sk::Data::from_filename(file)?;
        let v = (data.as_bytes().as_ptr(), data.len());
        std::mem::forget(data); // direct Skia streams can outlive the thread-local cache
        m.insert(file.to_path_buf(), v);
        Some(v)
    })?;
    // SAFETY: the immutable SkData owns this mapping and is deliberately never
    // dropped, including on UI-thread shutdown. No borrowed file handle remains.
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}
/// A font resolved like Qt resolves it (fontconfig), backed by the engine's single
/// process-lifetime mapping of its file: other text stacks (panels' qtext) build their
/// FreeType faces and shapers over these bytes instead of mapping or copying the file again.
#[derive(Clone)]
pub struct MappedFont {
    pub file: PathBuf,
    /// FreeType face index including named-instance bits (`index >> 16`).
    pub index: isize,
    pub bytes: &'static [u8],
    /// Skia typeface over `bytes`, with the named instance's variation applied.
    pub typeface: sk::Typeface,
}
/// [`MappedFont`] for `family` at Qt `weight` (cached per family/weight).
pub fn mapped_font(family: &str, weight: i32) -> Result<MappedFont, RenderError> {
    thread_local! {
        static LIB: ft::Library = ft::Library::init().expect("FreeType");
        static CACHE: std::cell::RefCell<HashMap<(String, i32), MappedFont>> = Default::default();
    }
    let key = (family.to_string(), weight);
    if let Some(f) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(f);
    }
    let face = LIB.with(|lib| resolve(family, weight, lib))?;
    let bytes = mapped_font_bytes(&face.file).ok_or(RenderError::Skia)?;
    let f = MappedFont { file: face.file, index: face.index, bytes, typeface: face.typeface };
    CACHE.with(|c| c.borrow_mut().insert(key, f.clone()));
    Ok(f)
}
/// A Skia typeface over the shared font-file mapping (no copy).
fn mapped_typeface(file: &std::path::Path, ttc: usize) -> Option<(sk::Typeface, usize)> {
    extern "C" {
        // skia-bindings (linked through skia-safe): SkMemoryStream::MakeDirect and
        // SkFontMgr::makeFromStream (takes ownership of the stream).
        fn C_SkMemoryStream_MakeDirect(data: *const std::ffi::c_void, length: usize) -> *mut std::ffi::c_void;
        fn C_SkFontMgr_makeFromStream(mgr: *const std::ffi::c_void, stream: *mut std::ffi::c_void, ttc: i32) -> *mut std::ffi::c_void;
    }
    let data = mapped_font_bytes(file)?;
    let (ptr, len) = (data.as_ptr(), data.len());
    let mgr = font_manager();
    const _: () = assert!(std::mem::size_of::<sk::FontMgr>() == std::mem::size_of::<*const std::ffi::c_void>());
    const _: () = assert!(std::mem::size_of::<sk::Typeface>() == std::mem::size_of::<*const std::ffi::c_void>());
    // SAFETY: FontMgr/Typeface are `#[repr(transparent)]` RCHandles (asserted pointer-sized);
    // the stream refers to the leaked mapping above and is owned by makeFromStream, which
    // returns a new reference (or null).
    unsafe {
        let mgr_ptr: *const std::ffi::c_void = std::mem::transmute_copy(&mgr);
        let stream = C_SkMemoryStream_MakeDirect(ptr.cast(), len);
        let tf = C_SkFontMgr_makeFromStream(mgr_ptr, stream, ttc as i32);
        if tf.is_null() {
            return None;
        }
        Some((std::mem::transmute::<*mut std::ffi::c_void, sk::Typeface>(tf), len))
    }
}
fn resolve(family: &str, weight: i32, lib: &ft::Library) -> Result<Face, RenderError> {
    let (file, index) = font_match(family, weight)?;
    // Named instances of a variable font must share the same backing stream;
    // otherwise preparing three weights copies the entire20MiB CJK font three times.
    type TypefaceCache = HashMap<(PathBuf, usize), (sk::Typeface, usize)>;
    thread_local! {static BASES: std::cell::RefCell<TypefaceCache> = Default::default();}
    let base = BASES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (file.clone(), (index & 0xffff) as usize);
        if let Some((face, _)) = cache.get(&key) { return Ok(face.clone()); }
        // Memory-mapped font file (svc-media, perf-shell-memory.md): only the pages Skia
        // touches become resident, clean and reclaimable, instead of a heap copy per file.
        let (base, len) = match mapped_typeface(&file, key.1) {
            Some(t) => t,
            None => {
                let data = std::fs::read(&file)?;
                (font_manager().new_from_data(&data, key.1).ok_or(RenderError::Skia)?, data.len())
            }
        };
        const LIMIT: usize = 64 * 1024 * 1024;
        if len <= LIMIT {
            if cache.len() >= 16 || cache.values().map(|(_, bytes)| bytes).sum::<usize>() + len > LIMIT { cache.clear(); }
            cache.insert(key, (base.clone(), len));
        }
        Ok::<_, RenderError>(base)
    })?;
    let face = lib
        .new_face(&file, index)
        .map_err(|e| RenderError::Font(e.to_string()))?;
    let typeface = if index >> 16 != 0 {
        let coords = named_instance_coords(&face);
        let params = base.variation_design_parameters().unwrap_or_default();
        let coordinates: Vec<_> = params
            .iter()
            .zip(coords.iter())
            .map(
                |(p, v)| sk::font_arguments::variation_position::Coordinate {
                    axis: p.tag,
                    value: *v as f32,
                },
            )
            .collect();
        let result = base
            .clone_with_arguments(&sk::FontArguments::new().set_variation_design_position(
                sk::font_arguments::VariationPosition {
                    coordinates: &coordinates,
                },
            ))
            .ok_or(RenderError::Skia)?;
        result
    } else {
        base
    };
    static NEXT_FACE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    Ok(Face {
        id: NEXT_FACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        file,
        index,
        typeface,
        upem: face.em_size() as f64,
        ascender: face.ascender() as f64,
        descender: -(face.descender() as f64),
    })
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

type CachedFtFace = ft::Face<&'static [u8]>;
thread_local! {
    static FT: ft::Library = ft::Library::init().unwrap();
    static FACES: std::cell::RefCell<HashMap<(PathBuf, isize), CachedFtFace>> = std::cell::RefCell::new(HashMap::new());
}

// Reuse opened FreeType faces. Every operation sets its size/load flags; reset
// the raster transform on exit so a fractional mask cannot affect later metrics.
fn with_face<T>(f: &Face, operation: impl FnOnce(&CachedFtFace) -> Result<T, RenderError>) -> Result<T, RenderError> {
    FT.with(|lib| FACES.with(|faces| {
        let mut faces = faces.borrow_mut();
        let key = (f.file.clone(), f.index);
        if !faces.contains_key(&key) {
            let bytes = mapped_font_bytes(&f.file).ok_or(RenderError::Skia)?;
            let face = lib.new_memory_face2(bytes, f.index).map_err(|e| RenderError::Font(e.to_string()))?;
            faces.insert(key.clone(), face);
        }
        let face = &faces[&key];
        let result = operation(face);
        face.set_transform(&mut ft::Matrix { xx: 65536, xy: 0, yx: 0, yy: 65536 }, &mut ft::Vector {x: 0, y: 0});
        result
    }))
}
// Immutable design metrics, shared between measurement and rendering.
thread_local! {
    static ADVANCES:std::cell::RefCell<HashMap<(u32,u16,u64),f64>>=Default::default();
    static RIGHT_EDGES:std::cell::RefCell<HashMap<(u32,u16,u64),f64>>=Default::default();
}
fn ft_linear_advance(f:&Face,glyph:u16,px:f64)->f64{
    let key=(f.id,glyph,px.to_bits());
    if let Some(advance)=ADVANCES.with(|cache|cache.borrow().get(&key).copied()){return advance;}
    let value=ft_linear_advance_uncached(f,glyph,px);
    ADVANCES.with(|cache|{let mut cache=cache.borrow_mut();if cache.len()>=8192{cache.clear();}cache.insert(key,value);});
    value
}
#[cfg(test)]
thread_local! {static ADVANCE_LOADS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
fn ft_linear_advance_uncached(f: &Face, glyph: u16, px: f64) -> f64 {
    #[cfg(test)] ADVANCE_LOADS.with(|count|count.set(count.get()+1));
    FT.with(|lib| {
        FACES.with(|faces| {
            let mut faces = faces.borrow_mut();
            let face = faces
                .entry((f.file.clone(), f.index))
                .or_insert_with(|| lib.new_memory_face2(mapped_font_bytes(&f.file).unwrap(), f.index).unwrap());
            face.set_char_size(0, (px * 64.0) as isize, 0, 0).unwrap();
            face.load_glyph(
                glyph as u32,
                ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP,
            )
            .unwrap();
            let lin = face.glyph().linear_hori_advance();
            (lin >> 10) as f64 / 64.0
        })
    })
}

/// Shape with skia's paragraph (HarfBuzz, same features as Qt: calt/liga on), itemising
/// JB → HM exactly like QFontEngineMulti, then replace the advances with Qt's.
fn layout(s: &Sample, faces: &[Face; 2]) -> Line {
    thread_local! {static COLLECTIONS:std::cell::RefCell<HashMap<(u32,u32),FontCollection>>=Default::default();}
    let fc=COLLECTIONS.with(|cache|{
        let mut cache=cache.borrow_mut();let key=(faces[0].id,faces[1].id);
        if let Some(fc)=cache.get(&key){return fc.clone();}
        let mut provider=TypefaceFontProvider::new();
        provider.register_typeface(faces[0].typeface.clone(),Some(JB));
        provider.register_typeface(faces[1].typeface.clone(),Some(HM));
        let mut fc=FontCollection::new();fc.set_asset_font_manager(Some(provider.into()));fc.disable_font_fallback();
        if cache.len()>=16{cache.clear();}cache.insert(key,fc.clone());fc
    });
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
            let face = if fam == faces[0].typeface.family_name() {
                0
            } else {
                1
            };
            for g in info.glyphs() {
                glyphs.push(Glyph {
                    face,
                    id: *g,
                    x: 0.0,
                });
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
    Line {
        glyphs,
        ascent,
        width: pen,
    }
}

fn qt_native_place(
    item_x: f64,
    item_y: f64,
    local_x: f64,
    local_baseline: f64,
    dpr: f64,
    exact: bool,
) -> (i64, f64, i64) {
    // QSGTextMaskMaterial::populate works in the text node's local coordinates …
    let fixed=|v:f64| if exact {v.trunc()}else{v.round()};
    let fx = fixed(local_x * 64.0) / 64.0; // QFixedPoint::fromPointF in native mode
    let dx = fx * dpr;
    let mut sub = 0.0;
    if dx != 0.0 {
        let v = (dx - dx.floor()) + 1.0 / 64.0;
        sub = (v * 4.0).floor() / 4.0;
    }
    let fy = fixed(local_baseline * 64.0) / 64.0;
    // … and textmask.vert snaps the final (item-translated) vertex: floor(x*dpr + 0.5).
    let ox = (item_x * dpr + 0.5).floor() as i64;
    let oy = (item_y * dpr + 0.5).floor() as i64;
    (ox + dx.floor() as i64, sub, oy + (fy * dpr).round() as i64)
}

fn qt_embolden<T>(face: &ft::Face<T>, weight: i32, px: f64) -> bool {
    let raw = face.raw();
    let bold_flag = raw.style_flags & ft::ffi::FT_STYLE_FLAG_BOLD as ft::ffi::FT_Long != 0;
    let fixed = raw.face_flags & ft::ffi::FT_FACE_FLAG_FIXED_WIDTH as ft::ffi::FT_Long != 0;
    if weight < 700 || bold_flag || fixed || px >= 64.0 {
        return false;
    }
    let os2 = unsafe {
        ft::ffi::FT_Get_Sfnt_Table(raw as *const _ as *mut _, ft::ffi::ft_sfnt_os2)
            as *const ft::ffi::TT_OS2
    };
    let actual = if os2.is_null() {
        700
    } else {
        unsafe { (*os2).usWeightClass }
    };
    actual < 700
}

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
    let lib = ft::Library::init().ok()?;
    let ftface = lib.new_face(&face.file, face.index).ok()?;
    ftface
        .set_char_size(0, (BASE * SCALE * 64.) as isize, 0, 0)
        .ok()?;
    ftface
        .load_glyph(
            glyph as u32,
            ft::face::LoadFlag::NO_HINTING | ft::face::LoadFlag::NO_BITMAP,
        )
        .ok()?;
    let (mut path, coords, kinds) = crate::distance_field::outline(&ftface)?;
    path.set_fill_type(sk::PathFillType::Winding);
    // Qt runs qSimplifyPath (union) first: overlapping CJK strokes have no inner edges.
    let b = path.compute_tight_bounds();
    if b.is_empty() {
        return None;
    }
    let (bx, by) = (b.left as f64, b.top as f64);
    let w = (b.width() as f64).ceil() as usize + 10;
    let h = (b.height() as f64).ceil() as usize + 10;
    let data =
        crate::distance_field::raster_elements(&coords, &kinds, w, h, (bx as f32, by as f32));
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
fn sample_df(df: &Df, u: f64, v: f64) -> f64 {
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub implicit_width: f32,
    pub implicit_height: f32,
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
}
#[derive(Hash, Eq, PartialEq, Clone)]
struct LayoutKey {
    value: String,
    family: String,
    fallback: String,
    size: u32,
    weight: i32,
    spacing: u32,
    width: u32,
    elide: bool,
}
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct AtlasKey {
    face: u32,
    glyph: u16,
    px: u32,
    dpr: u64,
    sub: u8,
    weight: i32,
}
struct Mask {
    image: sk::Image,
    left: i32,
    top: i32,
}
struct Shaped {
    sample: Sample,
    line: Line,
    faces: [Face; 2],
    metrics: TextMetrics,
}
/// UI-thread caches; native glyphs and distance fields are independent of color/opacity.
#[derive(Default)]
struct ShapeCache {
    entries:HashMap<u64,Vec<(LayoutKey,std::rc::Rc<Shaped>)>>,
    bytes:usize,
}
thread_local! {static SHAPED:std::cell::RefCell<ShapeCache>=std::cell::RefCell::new(ShapeCache::default());}

pub struct TextEngine {
    lib: ft::Library,
    faces: HashMap<(String, i32), Face>,
    atlas: HashMap<AtlasKey, Option<Mask>>,
    fields: HashMap<(u32, u16), Option<Df>>,
    /// Rasterised distance-field glyph quads by exact device placement (static text).
    df_masks: HashMap<DfMaskKey, Option<(sk::Image, i32, i32)>>,
}
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct DfMaskKey {
    face: u32,
    glyph: u16,
    qx0: u64,
    qy0: u64,
    scale: u64,
    amin: u64,
    amax: u64,
}
impl TextEngine {
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            lib: ft::Library::init().map_err(|e| RenderError::Font(e.to_string()))?,
            faces: HashMap::new(),
            atlas: HashMap::new(),
            fields: HashMap::new(),
            df_masks: HashMap::new(),
        })
    }
    fn face(&mut self, family: &str, weight: i32) -> Result<Face, RenderError> {
        let key = (family.to_string(), weight);
        if !self.faces.contains_key(&key) {
            thread_local! {static RESOLVED: std::cell::RefCell<HashMap<(String,i32),Face>> = Default::default();}
            let face = RESOLVED.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(face) = cache.get(&key) { return Ok(face.clone()); }
                let face = resolve(family, weight, &self.lib)?;
                if cache.len() >= 16 { cache.clear(); }
                cache.insert(key.clone(), face.clone());
                Ok::<_, RenderError>(face)
            })?;
            self.faces.insert(key.clone(), face);
        }
        Ok(self.faces[&key].clone())
    }
    fn shape(&mut self, text: &Text, width: Option<f32>) -> Result<std::rc::Rc<Shaped>, RenderError> {
        // Width affects shaping only when elision is requested. Measurement and
        // painting must share the same run for ordinary numeric labels.
        let width=if text.elide {width}else{None};
        let elide=text.elide && width.is_some();
        let family = text.families.first().map(String::as_str).unwrap_or(JB);
        let fallback = text.families.get(1).map(String::as_str).unwrap_or(HM);
        let size = text.size.trunc().max(1.).to_bits();
        let weight = text.weight.min(800);
        let spacing = text.letter_spacing.to_bits();
        let width_bits = width.unwrap_or(f32::INFINITY).to_bits();
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::hash::DefaultHasher::new();
            (text.text.as_str(), family, fallback, size, weight, spacing, width_bits, elide).hash(&mut h);
            h.finish()
        };
        if let Some(hit)=SHAPED.with(|cache|cache.borrow().entries.get(&hash).and_then(|v|v.iter().find(|(k,_)|k.value==text.text && k.family==family && k.fallback==fallback && k.size==size && k.weight==weight && k.spacing==spacing && k.width==width_bits && k.elide==elide).map(|(_,run)|run.clone()))){return Ok(hit);}
        let _timing = crate::diagnostics::scope("text.shape_miss");
        let key = LayoutKey {
            value: text.text.clone(),
            family: family.into(),
            fallback: fallback.into(),
            size: text.size.trunc().max(1.).to_bits(),
            weight: text.weight.min(800),
            spacing: text.letter_spacing.to_bits(),
            width: width.unwrap_or(f32::INFINITY).to_bits(),
            elide,
        };
        let faces = [
            self.face(family, key.weight)?,
            self.face(fallback, key.weight)?,
        ];
        let sample = Sample {
            text: text.text.clone(),
            px: f32::from_bits(key.size),
            weight: key.weight,
            letter_spacing: text.letter_spacing as f64,
        };
        let full = layout(&sample, &faces);
        let metrics = line_metrics(&sample, &full, &faces)?;
        let mut line = full;
        if text.elide
            && width.is_some_and(|w| metrics.implicit_width > w)
            && sample.text.chars().count() > 1
        {
            use unicode_segmentation::UnicodeSegmentation;
            let ell = faces[0].typeface.unichar_to_glyph('…' as i32);
            let available =
                width.unwrap() as f64 - ft_linear_advance(&faces[0], ell, sample.px as f64);
            let mut e = sample.clone();
            let mut prefix = String::new();
            if available >= 0. {
                for grapheme in sample.text.graphemes(true) {
                    let candidate = format!("{prefix}{grapheme}");
                    // SkParagraph omits terminal whitespace from glyph visits.
                    // Shape with a sentinel ellipsis so spaces still contribute
                    // when testing Qt's strict prefix-width boundary.
                    e.text = format!("{candidate}…");
                    if layout(&e, &faces).width >= width.unwrap() as f64 {
                        break;
                    }
                    prefix = candidate;
                }
                e.text = prefix + "…";
            } else {
                e.text.clear();
            }
            line = layout(&e, &faces);
        }
        let shaped = std::rc::Rc::new(Shaped {
            sample,
            line,
            faces,
            metrics,
        });
        SHAPED.with(|cache|{
            let mut cache=cache.borrow_mut();
            let bytes=key.value.len()+key.family.len()+key.fallback.len()+shaped.line.glyphs.len()*std::mem::size_of::<Glyph>()+std::mem::size_of::<Shaped>();
            if cache.entries.len()>=2048 || cache.bytes+bytes>4*1024*1024 {*cache=ShapeCache::default();}
            if bytes<=4*1024*1024 {cache.bytes+=bytes;cache.entries.entry(hash).or_default().push((key,shaped.clone()));}
        });
        Ok(shaped)
    }
    /// Conservative local paint extent of `text` laid out in `width` (alignment
    /// offset, line width, implicit height), outset by the font size for glyph overhang.
    pub fn extent(&mut self, text: &Text, width: f32) -> Result<crate::geometry::Rect, RenderError> {
        let _timing=crate::diagnostics::scope("text.extent");
        let shaped = self.shape(text, Some(width))?;
        let line = &shaped.line;
        let align = match text.align {
            sk::textlayout::TextAlign::Center => (width as f64 - line.width) / 2.,
            sk::textlayout::TextAlign::Right => width as f64 - line.width,
            _ => 0.,
        } as f32;
        let pad = text.size.max(1.);
        Ok(crate::geometry::Rect::new(align - pad, -pad, line.width as f32 + 2. * pad, shaped.metrics.implicit_height + 2. * pad))
    }
    pub fn measure(&mut self, text: &Text) -> Result<TextMetrics, RenderError> {
        Ok(self.shape(text, None)?.metrics)
    }
    pub fn paint(
        &mut self,
        c: &sk::Canvas,
        text: &Text,
        width: f32,
        alpha: f32,
    ) -> Result<(), RenderError> {
        self.paint_for_output(c, text, width, alpha, None)
    }
    pub(crate) fn paint_for_output(
        &mut self, c: &sk::Canvas, text: &Text, width: f32, alpha: f32,
        output_dpr: Option<f64>,
    ) -> Result<(), RenderError> {
        let _timing=crate::diagnostics::scope("text.draw");
        crate::diagnostics::count("text.draws");
        let shaped = self.shape(text, Some(width))?;
        let sample = &shaped.sample;
        let line = &shaped.line;
        let matrix = c.local_to_device_as_3x3();
        // Only positive uniform axis-aligned items use Qt's device-space snapping.
        // For other transforms rasterize local glyph quads, then let Skia apply the
        // complete item matrix (including reflection/shear) to each mask.
        let device_snap = matrix.skew_x() == 0.
            && matrix.skew_y() == 0.
            && matrix.scale_x() > 0.
            && matrix.scale_x() == matrix.scale_y()
            && !matrix.has_perspective();
        // Native text under a scaled item (QSGTextMaskMaterial): glyphs rasterised at the
        // output DPR, each glyph quad mapped through the item matrix and snapped to device
        // pixels, then sampled Nearest. GL draws it natively; raster canvases below.
        let transformed = device_snap && text.render_type == RenderType::NativeRendering
            && output_dpr.is_some_and(|d| d > 0. && d != matrix.scale_x() as f64);
        let transformed_native = transformed && crate::native_gl::enabled(c);
        let origin = if device_snap && !transformed {
            matrix.map_point((0., 0.))
        } else {
            sk::Point::new(0., 0.)
        };
        let dpr = if transformed { output_dpr.unwrap() } else {
            (matrix.scale_x() as f64).hypot(matrix.skew_y() as f64)
                .max((matrix.skew_x() as f64).hypot(matrix.scale_y() as f64))
        };
        if dpr <= 0. {
            return Ok(());
        }
        let align = match text.align {
            sk::textlayout::TextAlign::Center => (width as f64 - line.width) / 2.,
            sk::textlayout::TextAlign::Right => width as f64 - line.width,
            _ => 0.,
        };
        let _restore = sk::AutoCanvasRestore::guard(c, true);
        if device_snap {
            c.reset_matrix();
        } else {
            c.scale((1. / dpr as f32, 1. / dpr as f32));
        }
        let mut paint = sk::Paint::default();
        // The Skia fallback preserves float colour. QRgb8 conversion belongs
        // to the explicitly selected native GL text material below.
        paint.set_color4f(sk::Color4f::new(text.color.r, text.color.g, text.color.b, text.color.a * alpha), None);
        let paint = crate::render::exact(paint);
        // QQuickText's QRgb8 material conversion is validated for translucent
        // color and inherited opacity against independent Qt GPU goldens.
        let raw_native = device_snap
            && text.render_type == RenderType::NativeRendering && crate::native_gl::enabled(c);
        let mut native_glyphs = Vec::new();
        for glyph in &line.glyphs {
            let face = &shaped.faces[glyph.face];
            if text.render_type == RenderType::NativeRendering {
                let (ix, sub, baseline) = qt_native_place(
                    origin.x as f64 / dpr,
                    origin.y as f64 / dpr,
                    glyph.x + align,
                    line.ascent,
                    dpr,
                    raw_native,
                );
                let k = AtlasKey {
                    face: face.id,
                    glyph: glyph.id,
                    px: sample.px.to_bits(),
                    dpr: dpr.to_bits(),
                    sub: (sub * 4.) as u8,
                    weight: sample.weight,
                };
                if !self.atlas.contains_key(&k) {
                    let mask = native_mask(
                        face,
                        glyph.id,
                        sample.px,
                        dpr,
                        sub,
                        sample.weight,
                    )?;
                    if self.atlas.len() > 8192 {
                        self.atlas.clear();
                    }
                    self.atlas.insert(k, mask);
                }
                if let Some(mask) = &self.atlas[&k] {
                    if raw_native || transformed {
                        native_glyphs.push(crate::native_gl::Glyph { image:mask.image.clone(),x:(ix+mask.left as i64)as f32,y:(baseline-mask.top as i64)as f32 });
                    } else {
                    c.draw_image(
                        &mask.image,
                        (
                            (ix + mask.left as i64) as f32,
                            (baseline - mask.top as i64) as f32,
                        ),
                        Some(&paint),
                    );
                    }
                }
            } else {
                let fkey = (face.id, glyph.id);
                if !self.fields.contains_key(&fkey) {
                    if self.fields.len() > 4096 {
                        self.fields.clear();
                    }
                    self.fields.insert(fkey, make_df(face, glyph.id));
                }
                let Some(df) = &self.fields[&fkey] else {
                    continue;
                };
                let fs = sample.px as f64 / 54.;
                let (amin, amax) = df_threshold(fs * dpr);
                let qx0 = origin.x as f64 + (glyph.x + align + (df.bx - 5.) * fs) * dpr;
                let qy0 = origin.y as f64 + (line.ascent + (df.by - 5.) * fs) * dpr;
                let qx1 = qx0 + df.w as f64 * fs * dpr;
                let qy1 = qy0 + df.h as f64 * fs * dpr;
                let mkey = DfMaskKey {
                    face: face.id,
                    glyph: glyph.id,
                    qx0: qx0.to_bits(),
                    qy0: qy0.to_bits(),
                    scale: (fs * dpr).to_bits(),
                    amin: amin.to_bits(),
                    amax: amax.to_bits(),
                };
                if !self.df_masks.contains_key(&mkey) {
                    if self.df_masks.len() > 4096 {
                        self.df_masks.clear();
                    }
                    self.df_masks.insert(mkey, df_quad(df, qx0, qy0, qx1, qy1, fs * dpr, (amin, amax))?);
                }
                let Some((image, x0, y0)) = &self.df_masks[&mkey] else {
                    continue;
                };
                let (image, x0, y0) = (image, *x0, *y0);
                c.draw_image(image, (x0 as f32, y0 as f32), Some(&paint));
            }
        }
        if !native_glyphs.is_empty() {
            // QQuickText::setColor stores QColor::rgba() (QRgb, 8-bit),
            // then its scene graph material premultiplies the decoded vector.
            let q = |v: f32| (v.clamp(0.,1.)*255.).round()/255.;
            let a=q(text.color.a)*alpha;
            let color=[q(text.color.r)*a,q(text.color.g)*a,q(text.color.b)*a,a];
            if transformed_native {
                crate::native_gl::draw_transformed(c,&native_glyphs,color,&matrix,dpr as f32)?;
            } else if transformed {
                let snap = |v: f32| (v + 0.5).floor();
                let nearest = sk::SamplingOptions::from(sk::FilterMode::Nearest);
                for g in &native_glyphs {
                    let (w, h) = (g.image.width() as f32, g.image.height() as f32);
                    let p0 = matrix.map_point((g.x / dpr as f32, g.y / dpr as f32));
                    let p1 = matrix.map_point(((g.x + w) / dpr as f32, (g.y + h) / dpr as f32));
                    let (x, y) = (snap(p0.x), snap(p0.y));
                    let size = (snap(p1.x) - x, snap(p1.y) - y);
                    if size.0 > 0. && size.1 > 0. {
                        c.draw_image_rect_with_sampling_options(&g.image, None, sk::Rect::from_xywh(x, y, size.0, size.1), nearest, &paint);
                    }
                }
            } else { crate::native_gl::draw(c,&native_glyphs,color)?; }
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        SHAPED.with(|cache|*cache.borrow_mut()=ShapeCache::default());
        self.atlas.clear();
        self.fields.clear();
        self.df_masks.clear();
    }
}
fn mask_image(w: usize, h: usize, data: &[u8]) -> Result<sk::Image, RenderError> {
    let info = sk::ImageInfo::new(
        (w as i32, h as i32),
        sk::ColorType::Alpha8,
        sk::AlphaType::Premul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(data), w).ok_or(RenderError::Skia)
}
fn native_mask(
    f: &Face,
    glyph: u16,
    px: f32,
    dpr: f64,
    sub: f64,
    weight: i32,
) -> Result<Option<Mask>, RenderError> {
    let err = |e: ft::Error| RenderError::Font(e.to_string());
    with_face(f, |face| {
    face.set_char_size(0, (px as f64 * 64.) as isize, 0, 0)
        .map_err(err)?;
    let k = (dpr * 65536.).round() as ft::ffi::FT_Fixed;
    let mut matrix = ft::Matrix {
        xx: k,
        xy: 0,
        yx: 0,
        yy: k,
    };
    let mut delta = ft::Vector {
        x: (sub * 64.) as ft::ffi::FT_Pos,
        y: 0,
    };
    face.set_transform(&mut matrix, &mut delta);
    let flags = if dpr == 1. {
        ft::face::LoadFlag::TARGET_LIGHT | ft::face::LoadFlag::FORCE_AUTOHINT
    } else {
        ft::face::LoadFlag::NO_HINTING
            | ft::face::LoadFlag::NO_BITMAP
            | ft::face::LoadFlag::FORCE_AUTOHINT
    };
    face.load_glyph(glyph as u32, flags).map_err(err)?;
    let slot = face.glyph();
    if qt_embolden(face, weight, px as f64) {
        unsafe { ft::ffi::FT_GlyphSlot_Embolden(slot.raw() as *const _ as *mut _) }
    }
    slot.render_glyph(if dpr == 1. {
        ft::RenderMode::Light
    } else {
        ft::RenderMode::Normal
    })
    .map_err(err)?;
    let bitmap = slot.bitmap();
    let (w, h) = (bitmap.width() as usize, bitmap.rows() as usize);
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let pitch = bitmap.pitch();
    let bytes = bitmap.buffer();
    let mut data = vec![0u8; w * h];
    for y in 0..h {
        let row = if pitch >= 0 { y } else { h - 1 - y };
        let offset = row * pitch.unsigned_abs() as usize;
        data[y * w..(y + 1) * w].copy_from_slice(&bytes[offset..offset + w]);
    }
    Ok(Some(Mask {
        image: mask_image(w, h, &data)?,
        left: slot.bitmap_left(),
        top: slot.bitmap_top(),
    }))
    })
}
fn line_metrics(
    s: &Sample,
    line: &Line,
    faces: &[Face; 2],
) -> Result<TextMetrics, RenderError> {
    let descent = faces
        .iter()
        .map(|f| trunc64(f.descender * s.px as f64 / f.upem))
        .fold(0., f64::max);
    let mut overhang = 0.;
    if let Some(g) = line.glyphs.last() {
        let f = &faces[g.face];
        let key=(f.id,g.id,(s.px as f64).to_bits());
        let right=if let Some(right)=RIGHT_EDGES.with(|cache|cache.borrow().get(&key).copied()){right}else{
        let right = with_face(f, |face| {
        face.set_char_size(0, (s.px as f64 * 64.) as isize, 0, 0)
            .map_err(|e| RenderError::Font(e.to_string()))?;
        face.load_glyph(
            g.id as u32,
            ft::face::LoadFlag::TARGET_LIGHT | ft::face::LoadFlag::FORCE_AUTOHINT,
        )
        .map_err(|e| RenderError::Font(e.to_string()))?;
        let metrics = face.glyph().metrics();
        Ok(((metrics.horiBearingX + metrics.width) as f64 / 64.).ceil())
        })?;
        RIGHT_EDGES.with(|cache|{let mut cache=cache.borrow_mut();if cache.len()>=8192{cache.clear();}cache.insert(key,right);});
        right
        };
        overhang = (right - ft_linear_advance(f, g.id, s.px as f64).round()).max(0.);
    }
    Ok(TextMetrics {
        implicit_width: (line.width + overhang) as f32,
        implicit_height: (line.ascent + descent).ceil() as f32,
        advance: line.width as f32,
        ascent: line.ascent as f32,
        descent: descent as f32,
    })
}
thread_local! {static METRICS:std::cell::RefCell<Option<TextEngine>>=const{std::cell::RefCell::new(None)};}
pub fn measure(text: &Text) -> Result<TextMetrics, RenderError> {
    let _timing = crate::diagnostics::scope("text.measure");
    METRICS.with(|m| {
        let mut m = m.borrow_mut();
        if m.is_none() {
            *m = Some(TextEngine::new()?);
        }
        m.as_mut().unwrap().measure(text)
    })
}

/// Skia paragraph metrics used for wrapped QML labels. This is separate from
/// Qt native glyph metrics: wrapping must retain SkParagraph's line breaking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParagraphMetrics {
    pub height: f32,
    pub max_intrinsic_width: f32,
    pub lines: usize,
}
#[derive(Clone, Hash, PartialEq, Eq)]
struct WrappedKey {
    text: String,
    families: Vec<String>,
    size: u32,
    spacing: u32,
    width: u32,
    weight: i32,
    max_lines: Option<usize>,
    elide: bool,
    align: i32,
}
#[derive(Default)]
struct WrappedCache {
    fonts: HashMap<String, FontCollection>,
    entries: HashMap<WrappedKey, ParagraphMetrics>,
    bytes: usize,
}
thread_local! {
    static WRAPPED: std::cell::RefCell<WrappedCache> = std::cell::RefCell::new(WrappedCache::default());
}
/// Measure a wrapped paragraph without repeatedly creating Fontconfig managers.
/// The UI-thread cache holds at most 512 metric entries / 1 MiB of text keys and
/// 16 default-family font collections. Colour and rendering mode do not affect
/// SkParagraph's geometry. This does not alter `TextEngine` glyph placement.
pub fn measure_wrapped(text: &Text, width: f32) -> Result<ParagraphMetrics, RenderError> {
    if !width.is_finite() || width <= 0. || !text.size.is_finite() || text.size <= 0. {
        return Err(RenderError::InvalidSize);
    }
    let key = WrappedKey { text: text.text.clone(), families: text.families.clone(), size: text.size.to_bits(), spacing: text.letter_spacing.to_bits(), width: width.to_bits(), weight: text.weight, max_lines: text.max_lines, elide: text.elide, align: text.align as i32 };
    WRAPPED.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some(metrics) = cache.entries.get(&key) { return Ok(*metrics); }
        let _timing = crate::diagnostics::scope("text.wrap_miss");
        let family = text.families.first().map_or("Noto Sans", String::as_str);
        if !cache.fonts.contains_key(family) {
            if cache.fonts.len() == 16 { cache.fonts.clear(); }
            let mut fonts = FontCollection::new();
            fonts.set_default_font_manager(font_manager(), Some(family));
            cache.fonts.insert(family.into(), fonts);
        }
        let mut style = ParagraphStyle::new();
        style.set_max_lines(text.max_lines);
        style.set_text_align(text.align);
        if text.elide { style.set_ellipsis("…"); }
        let mut ts = TextStyle::new();
        ts.set_font_families(&text.families);
        ts.set_font_size(text.size);
        ts.set_letter_spacing(text.letter_spacing);
        ts.set_font_style(sk::FontStyle::new(sk::font_style::Weight::from(text.weight), sk::font_style::Width::NORMAL, sk::font_style::Slant::Upright));
        let mut builder = ParagraphBuilder::new(&style, cache.fonts[family].clone());
        builder.push_style(&ts);
        builder.add_text(&text.text);
        let mut paragraph = builder.build();
        paragraph.layout(width);
        let metrics = ParagraphMetrics { height: paragraph.height(), max_intrinsic_width: paragraph.max_intrinsic_width(), lines: paragraph.line_number() };
        let bytes = key.text.len() + key.families.iter().map(String::len).sum::<usize>();
        if cache.entries.len() >= 512 || cache.bytes + bytes > 1024 * 1024 {
            cache.entries.clear(); cache.bytes = 0;
        }
        if bytes <= 1024 * 1024 { cache.bytes += bytes; cache.entries.insert(key, metrics); }
        Ok(metrics)
    })
}
/// Prepare font matching/shaping before accepting input. The shell supplies its
/// common styles; content/layout caches remain data-driven. This also shares
/// resolved typefaces with subsequent renderer TextEngines.
pub fn prepare_fonts(styles: &[Text]) -> Result<(), RenderError> {
    for text in styles {
        measure(text)?;
    }
    Ok(())
}
/// Clear UI-thread measurement caches, including wrapped paragraph metrics.
pub fn clear_measurement_caches() {
    ADVANCES.with(|cache|cache.borrow_mut().clear());
    RIGHT_EDGES.with(|cache|cache.borrow_mut().clear());
    SHAPED.with(|cache|*cache.borrow_mut()=ShapeCache::default());
    METRICS.with(|cache| *cache.borrow_mut() = None);
    WRAPPED.with(|cache| *cache.borrow_mut() = WrappedCache::default());
}

/// Distance-field glyph quad rasterised at its exact device placement.
fn df_quad(df: &Df, qx0: f64, qy0: f64, qx1: f64, qy1: f64, scale: f64, (amin, amax): (f64, f64)) -> Result<Option<(sk::Image, i32, i32)>, RenderError> {
    let (x0, y0, x1, y1) = (qx0.floor() as i32, qy0.floor() as i32, qx1.ceil() as i32, qy1.ceil() as i32);
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let mut data = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let px = x0 as f64 + x as f64 + 0.5;
            let py = y0 as f64 + y as f64 + 0.5;
            if px >= qx0 && px < qx1 && py >= qy0 && py < qy1 {
                data[y * w + x] = (smoothstep(amin, amax, sample_df(df, (px - qx0) / scale, (py - qy0) / scale)) * 255.).round() as u8;
            }
        }
    }
    Ok(Some((mask_image(w, h, &data)?, x0, y0)))
}

#[cfg(test)]
mod cache_regression {
    use super::*;
    #[test]
    fn changing_numeric_labels_reuses_design_metrics_without_changing_positions(){
        clear_measurement_caches();ADVANCE_LOADS.with(|count|count.set(0));
        let mut engine=TextEngine::new().unwrap();let mut lines=Vec::new();
        for value in 0..100{
            let text=Text{text:format!("\u{f2db} CPU {value}%"),size:16.,weight:700,elide:false,..Default::default()};
            lines.push(engine.shape(&text,None).unwrap());
        }
        let cached_loads=ADVANCE_LOADS.with(|count|count.get());
        assert!(cached_loads<30,"unchanged glyphs must not repeatedly load outlines: {cached_loads}");
        ADVANCE_LOADS.with(|count|count.set(0));
        for shaped in lines{
            let mut pen=0.;
            for glyph in &shaped.line.glyphs{
                assert_eq!(glyph.x,pen);
                pen+=ft_linear_advance_uncached(&shaped.faces[glyph.face],glyph.id,16.);
            }
            assert_eq!(shaped.line.width,pen);
        }
        assert!(ADVANCE_LOADS.with(|count|count.get())>800);
    }
    #[test]
    fn measurement_and_rendering_share_non_elided_runs_but_keep_elision_width(){
        let mut measure=TextEngine::new().unwrap();let mut paint=TextEngine::new().unwrap();
        let mut text=Text{text:"\u{f2db} 19%".into(),size:16.,weight:700,elide:false,..Default::default()};
        let measured=measure.shape(&text,None).unwrap();let drawn=paint.shape(&text,Some(100.)).unwrap();
        assert!(std::rc::Rc::ptr_eq(&measured,&drawn));
        text.elide=true;
        let short=paint.shape(&text,Some(20.)).unwrap();let long=paint.shape(&text,Some(200.)).unwrap();
        assert!(!std::rc::Rc::ptr_eq(&short,&long));assert!(short.line.width<long.line.width);
        clear_measurement_caches();assert!(SHAPED.with(|cache|cache.borrow().entries.is_empty()));
    }
    #[test]
    fn named_weights_share_font_mapping_and_preserve_freetype_pixels() {
        let lib = ft::Library::init().unwrap();
        let mut backing = None;
        for weight in [400, 500, 600, 700] {
            let face = resolve(HM, weight, &lib).unwrap();
            let mapping = font_mapping(HM, weight).unwrap();
            assert_eq!(mapping.file, face.file);
            assert_eq!(mapping.index, face.index);
            let bytes = mapping.bytes;
            assert_eq!(bytes.as_ptr(), mapped_font_bytes(&face.file).unwrap().as_ptr());
            assert_eq!(*backing.get_or_insert(bytes.as_ptr()), bytes.as_ptr());
            let file_face = lib.new_face(&face.file, face.index).unwrap();
            let raster = |face: &ft::Face<_>| {
                face.set_char_size(0, 13 * 64, 0, 0).unwrap();
                face.load_char('日' as usize, ft::face::LoadFlag::NO_HINTING
                    | ft::face::LoadFlag::NO_BITMAP | ft::face::LoadFlag::FORCE_AUTOHINT).unwrap();
                face.glyph().render_glyph(ft::RenderMode::Normal).unwrap();
                (face.glyph().bitmap().buffer().to_vec(), face.glyph().advance().x)
            };
            let expected = raster(&file_face);
            with_face(&face, |cached| {
                // SAFETY: FreeType owns a live stream for this cached face.
                let stream = unsafe { &*cached.raw().stream };
                assert_eq!(stream.base.cast_const(), bytes.as_ptr(),
                    "all named instances must borrow the one mapping");
                cached.set_char_size(0, 13 * 64, 0, 0).unwrap();
                cached.load_char('日' as usize, ft::face::LoadFlag::NO_HINTING
                    | ft::face::LoadFlag::NO_BITMAP | ft::face::LoadFlag::FORCE_AUTOHINT).unwrap();
                cached.glyph().render_glyph(ft::RenderMode::Normal).unwrap();
                assert_eq!((cached.glyph().bitmap().buffer().to_vec(), cached.glyph().advance().x), expected);
                Ok(())
            }).unwrap();
        }
    }
    #[test]
    fn qt_distance_field_cache_bytes() {
        let lib = ft::Library::init().unwrap();
        for (family, weight, json) in [
            (JB, 700, include_str!("../tests/text/qt-cache-jb.json")),
            (HM, 400, include_str!("../tests/text/qt-cache-cjk.json")),
        ] {
            let face = resolve(family, weight, &lib).unwrap();
            let values: serde_json::Value = serde_json::from_str(json).unwrap();
            for item in values.as_array().unwrap() {
                let glyph = item["glyph"].as_u64().unwrap() as u16;
                let df = make_df(&face, glyph).unwrap();
                assert_eq!(df.w, item["w"].as_u64().unwrap() as usize);
                assert_eq!(df.h, item["h"].as_u64().unwrap() as usize);
                let expected: Vec<u8> = item["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect();
                let worst = df
                    .data
                    .iter()
                    .zip(expected.iter())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    worst <= 1,
                    "{family} glyph {glyph}: max cache delta={worst}"
                );
            }
        }
    }
}

#[cfg(test)]
mod review_r2_clear {
    use super::*;
    #[test]
    fn review_r2_text_clear_releases_df_masks() {
        let mut e = TextEngine::new().unwrap();
        let mut s = sk::surfaces::raster_n32_premul((100, 50)).unwrap();
        let t = Text {
            text: "CACHE".into(),
            families: vec!["JetBrainsMono Nerd Font".into()],
            ..Default::default()
        };
        e.paint(s.canvas(), &t, 100., 1.).unwrap();
        let before = e.df_masks.len();
        assert!(before > 0);
        e.clear();
        assert_eq!(
            e.df_masks.len(),
            0,
            "clear left {before} distance-field raster masks alive"
        );
    }
}

#[cfg(test)]
mod wrapped_regressions {
    use super::*;
    #[test]
    fn font_preparation_does_not_retain_synthetic_wrapped_paragraphs() {
        clear_measurement_caches();
        let styles: Vec<_> = [500, 600, 700].into_iter().map(|weight| Text {
            text: "M电池".into(), families: vec![JB.into(), HM.into()],
            size: 13., weight, ..Default::default()
        }).collect();
        prepare_fonts(&styles).unwrap();
        WRAPPED.with(|cache| {
            let cache = cache.borrow();
            assert!(cache.fonts.is_empty());
            assert!(cache.entries.is_empty());
        });
        let actual = Text { text: "Charging: 3 hours 10 minutes".into(), ..styles[0].clone() };
        let metrics = measure_wrapped(&actual, 118.).unwrap();
        assert!(metrics.height > 0.);
        assert_eq!(measure_wrapped(&actual, 118.).unwrap(), metrics);
    }
    #[test]
    fn cached_wrapping_preserves_legacy_paragraph_geometry() {
        for value in ["Power Supply", "Charging: 3 hours 10 minutes", "电池当前充电状态，剩余时间约三小时", "Long line\nExplicit second line"] {
            for (px,weight,width) in [(13.,600,118.),(13.,700,170.),(11.,500,300.)] {
                let text=Text { text:value.into(), families:vec![JB.into(),HM.into()], size:px, weight, max_lines:None, elide:false, ..Default::default() };
                let mut fonts=FontCollection::new(); fonts.set_default_font_manager(sk::FontMgr::default(),Some(JB));
                let mut style=ParagraphStyle::new();style.set_max_lines(None);
                let mut ts=TextStyle::new();ts.set_font_families(&[JB,HM]);ts.set_font_size(px);
                ts.set_font_style(sk::FontStyle::new(sk::font_style::Weight::from(weight),sk::font_style::Width::NORMAL,sk::font_style::Slant::Upright));
                let mut builder=ParagraphBuilder::new(&style,fonts);builder.push_style(&ts);builder.add_text(value);
                let mut paragraph=builder.build();paragraph.layout(width);
                let first=measure_wrapped(&text,width).unwrap();
                assert_eq!(first.height,paragraph.height()); assert_eq!(first.lines,paragraph.line_number());
                assert_eq!(measure_wrapped(&text,width).unwrap(),first);
            }
        }
    }
    #[test]
    fn wrapped_cache_is_bounded_and_flushable() {
        clear_measurement_caches();
        for i in 0..530 { let t=Text{text:format!("Sample {i}"),max_lines:None,..Default::default()};measure_wrapped(&t,100.).unwrap(); }
        WRAPPED.with(|c|{let c=c.borrow();assert!(c.entries.len()<=512);assert!(c.bytes<=1024*1024);assert!(c.fonts.len()<=16);});
        clear_measurement_caches(); WRAPPED.with(|c|assert!(c.borrow().entries.is_empty()));
        assert!(measure_wrapped(&Text::default(),f32::NAN).is_err());
    }
}

#[cfg(test)]
mod island_elision_regression {
    use super::*;
    #[test]
    fn qfixed_point_truncates_before_subpixel_selection() {
        // qfixed_p.h:36,169; a rounded conversion incorrectly selects phase .5.
        assert_eq!(qt_native_place(0.,0.,173.48109,9.171875,1.,true),(173,0.25,9));
    }
    #[test]
    fn qt_mid_morph_title_elision() {
        let mut engine=TextEngine::new().unwrap();
        let text=Text {text:"Midnight City".into(),families:vec![JB.into(),HM.into()],size:18.,weight:700,elide:true,..Default::default()};
        let shaped=engine.shape(&text,Some(101.28863)).unwrap();
        let expected=layout(&Sample{text:"Midnight…".into(),px:18.,weight:700,letter_spacing:0.}, &shaped.faces);
        assert_eq!(shaped.line.glyphs.iter().map(|g|(g.face,g.id,g.x)).collect::<Vec<_>>(),expected.glyphs.iter().map(|g|(g.face,g.id,g.x)).collect::<Vec<_>>());
    }
}
