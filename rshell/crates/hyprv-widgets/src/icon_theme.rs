//! Freedesktop theme icons exactly as the original shell gets them: a port of Qt 6's
//! `QIconLoader` lookup (qiconloader.cpp) as reached through Quickshell's
//! `image://icon/<name>` provider, `QIcon::fromTheme(name).pixmap(size)`.
//!
//! - **Theme.** The live quickshell runs with `QT_QPA_PLATFORMTHEME=qt6ct`, but qt6ct-kde
//!   0.11 fails to load under Qt 6.12, so Qt uses its generic Unix theme. That theme gives
//!   no system icon theme name, so `QIconLoader` uses the fallback theme `hicolor`
//!   (verified against the nested references: `discord` resolves from
//!   hicolor/256x256, while `telegram`, present only in Fluent/Papirus-style themes, is
//!   missing). [`system_theme`] returns that, overridable with `HYPRV_ICON_THEME`.
//! - **Search paths** (`QGenericUnixTheme::xdgIconThemePaths`): `~/.icons` if present,
//!   then every `icons` dir of the generic data locations (`$XDG_DATA_HOME` or
//!   `~/.local/share`, then `$XDG_DATA_DIRS` or `/usr/local/share:/usr/share`).
//!   Unthemed fallback (`iconFallbackPaths`): the `pixmaps` dirs of the same locations.
//! - **Lookup** (`findIconHelper`, ported from qtbase 6.12 qiconloader.cpp): the theme's
//!   directories are every index.theme section with a `Size`, in `QSettings::allKeys()`
//!   (sorted) order; a valid GTK `icon-theme.cache` narrows and orders them per icon. Over
//!   all content dirs, `name.png` entries are inserted at the *front* (reverse discovery
//!   order) and `name.svg` entries appended. Then `Inherits` (+ the fallback theme,
//!   hicolor last), the dash fallback (Applications/MimeTypes dirs skipped), the fallback
//!   theme, and the unthemed `pixmaps` paths.
//! - **Size** (`entryForSize`/`directorySizeDelta`): verbatim, including the
//!   order-dependent "prefer downscaled" updates. Verified against real Qt 6.12 on a
//!   synthetic theme (12 icons × 9 requests) and on this machine's hicolor.
//! - **Request**: Qt hands the provider the request in *device* pixels (an 18-px
//!   `IconImage` at DPR 2 asks for 36×36), and the live shell resolves it at scale 1
//!   (measured on nested live-shell captures, `docs/requests/parity-capture.md`: chrome
//!   stays hicolor 32×32, discord 256 → 36). Entries are therefore chosen at size 36 /
//!   scale 1 and pixmaps are at most 36 px; [`tray_raster`] does exactly that. The scene
//!   graph then draws the texture into the 36-px quad (`mipmap: true`, linear magnify).
//! - **Pixmap** (`PixmapEntry::pixmap`): raster entries larger than size×scale are
//!   smooth-downscaled with Qt's `qSmoothScaleImage` ([`smooth_scale_down`]),
//!   IgnoreAspectRatio to the `adjustSize` result; smaller ones are never upscaled. SVG
//!   entries render at size×scale (`QSvgIconEngine`).
//!
//! Quickshell draws [`missing_icon`] when no icon is found.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// `QIconLoader::fallbackThemeName()` of Qt's generic Unix theme.
const FALLBACK_THEME: &str = "hicolor";

/// Icon theme the live shell's Qt resolves (see module docs).
pub fn system_theme() -> String {
    std::env::var("HYPRV_ICON_THEME")
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "hicolor".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirType {
    Fixed,
    Scalable,
    Threshold,
    /// Unthemed `pixmaps` fallback: matches every size.
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconDir {
    pub path: String,
    /// `Context` is `Applications` or `MimeTypes` (skipped during dash fallback).
    pub app_or_mime: bool,
    pub size: i32,
    pub min_size: i32,
    pub max_size: i32,
    pub threshold: i32,
    pub scale: i32,
    pub kind: DirType,
}

impl IconDir {
    fn fallback() -> Self {
        Self {
            path: String::new(),
            app_or_mime: false,
            size: 0,
            min_size: 0,
            max_size: 0,
            threshold: 0,
            scale: 1,
            kind: DirType::Fallback,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Pixmap,
    Scalable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconEntry {
    pub file: PathBuf,
    pub dir: IconDir,
    pub kind: EntryKind,
}

#[derive(Clone, Debug)]
struct Theme {
    valid: bool,
    content_dirs: Vec<PathBuf>,
    /// `icon-theme.cache` per content dir, when Qt would consider it valid.
    caches: Vec<Option<std::sync::Arc<Vec<u8>>>>,
    dirs: Vec<IconDir>,
    parents: Vec<String>,
}

/// Open once, reject non-regular files, and enforce the limit while reading.
/// O_NONBLOCK prevents a pathname swapped to a FIFO from blocking the loader.
pub fn read_bounded_file(path: &std::path::Path, limit: usize) -> Option<Vec<u8>> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return None;
    }
    let mut data = Vec::new();
    file.take((limit as u64).checked_add(1)?)
        .read_to_end(&mut data)
        .ok()?;
    (data.len() <= limit).then_some(data)
}
const MAX_THEME_BYTES: usize = 1024 * 1024;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHE_CHAIN: usize = 4096;

/// Qt's `QIconCacheGtkReader`: a content dir's `icon-theme.cache` is used only if it is
/// version 1 and not older than the theme dir or any directory it lists.
pub fn gtk_cache(theme_dir: &std::path::Path) -> Option<Vec<u8>> {
    let file = theme_dir.join("icon-theme.cache");
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let cache_time = modified(&file)?;
    if modified(theme_dir).is_some_and(|t| cache_time < t) {
        return None;
    }
    let data = read_bounded_file(&file, MAX_CACHE_BYTES)?;
    if read16(&data, 0)? != 1 {
        return None;
    }
    let list = read32(&data, 8)? as usize;
    let count = table_count(&data, list, 4)?;
    for i in 0..count {
        let sub = cstr(&data, read32(&data, list + 4 + 4 * i)? as usize)?;
        if modified(&theme_dir.join(sub)).is_some_and(|t| cache_time < t) {
            return None;
        }
    }
    Some(data)
}
fn read16(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        d.get(o..o.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn read32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        d.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}
// Count-prefixed table: validate the entire span before iterating or allocating.
fn table_count(d: &[u8], offset: usize, stride: usize) -> Option<usize> {
    let count = read32(d, offset)? as usize;
    if count > 65_536 {
        return None;
    }
    let start = offset.checked_add(4)?;
    d.get(start..start.checked_add(count.checked_mul(stride)?)?)?;
    Some(count)
}
fn cstr(d: &[u8], o: usize) -> Option<&str> {
    let rest = d.get(o..)?;
    let rest = &rest[..rest.len().min(4096)];
    std::str::from_utf8(&rest[..rest.iter().position(|&b| b == 0)?]).ok()
}
/// `QIconCacheGtkReader::lookup`: the subdirectories that contain `name` (None if the
/// cache is malformed, which Qt treats as invalid → full scan).
pub fn gtk_cache_lookup(d: &[u8], name: &str) -> Option<Vec<String>> {
    let bytes = name.as_bytes();
    let first = *bytes.first()?;
    let mut h = (first as i8) as i32 as u32;
    for &c in &bytes[1..] {
        h = (h << 5).wrapping_sub(h).wrapping_add(c as u32);
    }
    let hash_off = read32(d, 4)? as usize;
    let buckets = table_count(d, hash_off, 4)? as u32;
    if buckets == 0 {
        return None;
    }
    let mut bucket = read32(d, hash_off + 4 + (h % buckets) as usize * 4)? as usize;
    let mut visited = std::collections::HashSet::new();
    while bucket != 0 && bucket != u32::MAX as usize {
        if visited.len() >= MAX_CACHE_CHAIN || !visited.insert(bucket) {
            return None;
        }
        d.get(bucket..bucket.checked_add(12)?)?;
        if cstr(d, read32(d, bucket + 4)? as usize)? == name {
            let dir_list = read32(d, 8)? as usize;
            let dir_count = table_count(d, dir_list, 4)? as u32;
            let list = read32(d, bucket + 8)? as usize;
            let mut out = Vec::new();
            for j in 0..table_count(d, list, 8)? {
                let index = read16(d, list + 4 + 8 * j)? as u32;
                if index >= dir_count {
                    return None;
                }
                out.push(
                    cstr(d, read32(d, dir_list + 4 + 4 * index as usize)? as usize)?.to_string(),
                );
            }
            return Some(out);
        }
        bucket = read32(d, bucket)? as usize;
    }
    Some(vec![])
}

/// Search roots: `(icon theme paths, unthemed fallback paths)`.
/// `HYPRV_ICON_SEARCH_PATHS` / `HYPRV_ICON_FALLBACK_PATHS` (colon-separated) replace them,
/// e.g. so an isolated test child with a private HOME searches the live shell's roots.
pub fn search_paths() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let split = |v: std::ffi::OsString| -> Vec<PathBuf> {
        std::env::split_paths(&v)
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    };
    if let Some(icons) = std::env::var_os("HYPRV_ICON_SEARCH_PATHS") {
        let fallback = std::env::var_os("HYPRV_ICON_FALLBACK_PATHS")
            .map(split)
            .unwrap_or_default();
        return (split(icons), fallback);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut data = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(d) => data.push(PathBuf::from(d)),
        None => {
            if let Some(h) = &home {
                data.push(h.join(".local/share"));
            }
        }
    }
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    let mut icons = Vec::new();
    if let Some(h) = &home {
        let d = h.join(".icons");
        if d.is_dir() {
            icons.push(d);
        }
    }
    let mut pixmaps = Vec::new();
    for d in data {
        let (i, p) = (d.join("icons"), d.join("pixmaps"));
        if i.is_dir() && !icons.contains(&i) {
            icons.push(i);
        }
        if p.is_dir() && !pixmaps.contains(&p) {
            pixmaps.push(p);
        }
    }
    (icons, pixmaps)
}

/// Minimal index.theme reader: `[section]` → key → value.
fn parse_ini(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            out.entry(section.clone())
                .or_default()
                .insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

fn load_theme(name: &str, roots: &[PathBuf]) -> Theme {
    let content_dirs: Vec<PathBuf> = roots
        .iter()
        .map(|r| r.join(name))
        .filter(|d| d.is_dir())
        .collect();
    // QIconTheme: the first content dir that has index.theme defines the theme.
    let Some(text) = content_dirs.iter().find_map(|d| {
        String::from_utf8(read_bounded_file(&d.join("index.theme"), MAX_THEME_BYTES)?).ok()
    }) else {
        return Theme {
            valid: false,
            content_dirs,
            caches: vec![],
            dirs: vec![],
            parents: vec![],
        };
    };
    let ini = parse_ini(&text);
    let main = ini.get("Icon Theme").cloned().unwrap_or_default();
    let int = |s: &HashMap<String, String>, k: &str, d: i32| {
        s.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d)
    };
    // QIconTheme: every section with a non-zero Size, in QSettings::allKeys() order, i.e.
    // sorted "<section>/Size" keys (not the Directories= order).
    let mut keys: Vec<(String, &String)> = ini
        .iter()
        .filter(|(_, v)| v.contains_key("Size"))
        .map(|(k, _)| (format!("{k}/Size"), k))
        .collect();
    keys.sort();
    let mut dirs = Vec::new();
    for (_, path) in keys {
        let s = &ini[path];
        let size = int(s, "Size", 0);
        if size == 0 {
            continue;
        }
        let kind = match s.get("Type").map(|t| t.as_str()) {
            Some("Fixed") => DirType::Fixed,
            Some("Scalable") => DirType::Scalable,
            _ => DirType::Threshold,
        };
        dirs.push(IconDir {
            path: path.to_string(),
            app_or_mime: matches!(
                s.get("Context").map(|c| c.as_str()),
                Some("Applications" | "MimeTypes")
            ),
            size,
            min_size: int(s, "MinSize", size),
            max_size: int(s, "MaxSize", size),
            threshold: int(s, "Threshold", 2),
            scale: int(s, "Scale", 1),
            kind,
        });
    }
    let mut parents: Vec<String> = main
        .get("Inherits")
        .map(|v| {
            v.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // QIconTheme::parents(): Inherits, then the fallback theme, with hicolor moved last.
    parents.push(FALLBACK_THEME.into());
    parents.retain(|p| p != "hicolor");
    parents.push("hicolor".into());
    let caches = content_dirs
        .iter()
        .map(|d| gtk_cache(d).map(std::sync::Arc::new))
        .collect();
    Theme {
        valid: true,
        content_dirs,
        caches,
        dirs,
        parents,
    }
}

fn theme_cached(name: &str, roots: &[PathBuf]) -> Theme {
    type ThemeCache = Mutex<HashMap<(String, Vec<PathBuf>), Theme>>;
    static CACHE: OnceLock<ThemeCache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = (name.to_string(), roots.to_vec());
    if let Some(t) = cache.lock().unwrap().get(&key) {
        return t.clone();
    }
    let t = load_theme(name, roots);
    cache.lock().unwrap().insert(key, t.clone());
    t
}

/// `QIconLoader::findIconHelper`. `generic_fallback` = this is a dash-shortened name
/// (Qt then skips Applications/MimeTypes directories).
fn find_in_theme(
    theme_name: &str,
    icon: &str,
    roots: &[PathBuf],
    visited: &mut Vec<String>,
    dash: bool,
    generic_fallback: bool,
) -> Vec<IconEntry> {
    visited.push(theme_name.to_string());
    let theme = theme_cached(theme_name, roots);
    if !theme.valid {
        return vec![];
    }
    let mut entries: Vec<IconEntry> = Vec::new();
    for (i, content) in theme.content_dirs.iter().enumerate() {
        // A valid GTK cache replaces the directory list (in the cache's order).
        let listed = theme
            .caches
            .get(i)
            .and_then(|c| c.as_ref())
            .and_then(|c| gtk_cache_lookup(c, icon));
        let dirs: Vec<&IconDir> = match &listed {
            Some(paths) => paths
                .iter()
                .filter_map(|p| theme.dirs.iter().find(|d| &d.path == p))
                .collect(),
            None => theme.dirs.iter().collect(),
        };
        for dir in dirs {
            if generic_fallback && dir.app_or_mime {
                continue;
            }
            let base = content.join(&dir.path);
            let png = base.join(format!("{icon}.png"));
            if png.is_file() {
                // Qt inserts PNG entries at the front (so they end up in reverse
                // discovery order) and appends SVG entries.
                entries.insert(
                    0,
                    IconEntry {
                        file: png,
                        dir: dir.clone(),
                        kind: EntryKind::Pixmap,
                    },
                );
                continue;
            }
            let svg = base.join(format!("{icon}.svg"));
            if svg.is_file() {
                entries.push(IconEntry {
                    file: svg,
                    dir: dir.clone(),
                    kind: EntryKind::Scalable,
                });
            }
        }
    }
    if entries.is_empty() {
        for parent in &theme.parents {
            if !visited.iter().any(|v| v == parent) {
                entries = find_in_theme(parent, icon, roots, visited, false, generic_fallback);
            }
            if !entries.is_empty() {
                break;
            }
        }
    }
    if dash && entries.is_empty() {
        if let Some(i) = icon.rfind('-') {
            let mut fresh = Vec::new();
            entries = find_in_theme(theme_name, &icon[..i], roots, &mut fresh, true, true);
        }
    }
    entries
}

/// `QIconLoader::loadIcon`: the theme (with parents and dash fallback), the fallback
/// theme, then the unthemed fallback paths (`.png`, `.xpm`, `.svg`; first dir that has one).
pub fn lookup_entries(
    name: &str,
    theme: &str,
    roots: &[PathBuf],
    fallback_roots: &[PathBuf],
) -> Vec<IconEntry> {
    if name.is_empty() {
        return vec![];
    }
    let mut visited = Vec::new();
    let mut entries = find_in_theme(theme, name, roots, &mut visited, true, false);
    if entries.is_empty() && !visited.iter().any(|v| v == FALLBACK_THEME) {
        entries = find_in_theme(FALLBACK_THEME, name, roots, &mut visited, true, false);
    }
    if !entries.is_empty() {
        return entries;
    }
    for root in fallback_roots {
        for (ext, kind) in [
            ("png", EntryKind::Pixmap),
            ("xpm", EntryKind::Pixmap),
            ("svg", EntryKind::Scalable),
        ] {
            let f = root.join(format!("{name}.{ext}"));
            if f.is_file() {
                return vec![IconEntry {
                    file: f,
                    dir: IconDir::fallback(),
                    kind,
                }];
            }
        }
    }
    vec![]
}

/// `directoryMatchesSizeAndScale`.
fn matches(dir: &IconDir, size: i32, scale: i32) -> bool {
    if dir.scale != scale {
        return false;
    }
    match dir.kind {
        DirType::Fixed => dir.size == size,
        DirType::Scalable => size <= dir.max_size && size >= dir.min_size,
        DirType::Threshold => size >= dir.size - dir.threshold && size <= dir.size + dir.threshold,
        DirType::Fallback => false,
    }
}

/// `directorySizeDelta` (Qt 6.12): signed only for Fixed directories; Scalable and
/// Threshold deltas are distances (≥ 0), Fallback is `INT_MAX`.
fn delta(dir: &IconDir, size: i32, scale: i32) -> i32 {
    let scaled = size * scale;
    match dir.kind {
        DirType::Fixed => dir.size * dir.scale - scaled,
        DirType::Scalable => {
            if scaled < dir.min_size * dir.scale {
                dir.min_size * dir.scale - scaled
            } else if scaled > dir.max_size * dir.scale {
                scaled - dir.max_size * dir.scale
            } else {
                0
            }
        }
        DirType::Threshold => {
            if scaled < (dir.size - dir.threshold) * dir.scale {
                dir.min_size * dir.scale - scaled
            } else if scaled > (dir.size + dir.threshold) * dir.scale {
                scaled - dir.max_size * dir.scale
            } else {
                0
            }
        }
        DirType::Fallback => i32::MAX,
    }
}

/// `QIconLoaderEngine::entryForSize` (Qt 6.12), verbatim including its order-dependent
/// "prefer downscaled" updates: a delta-0 entry can be replaced by a later positive one.
pub fn entry_for_size(entries: &[IconEntry], size: i32, scale: i32) -> Option<&IconEntry> {
    if entries.len() <= 1 {
        return entries.first();
    }
    let mut minimal = i32::MIN;
    let mut closest: Option<&IconEntry> = None;
    for e in entries {
        if matches(&e.dir, size, scale) {
            return Some(e);
        }
        let d = delta(&e.dir, size, scale);
        if (d > minimal && minimal <= 0)
            || (d > 0 && d < minimal.unsigned_abs().min(i32::MAX as u32) as i32)
            || d == 0
        {
            minimal = d;
            closest = Some(e);
        }
    }
    closest.or(entries.first())
}

/// The entry `QIcon::fromTheme(name).pixmap(size, dpr)` renders from, with the default
/// theme and search paths: `size` is the requested (logical) size, `scale` = ceil(dpr).
pub fn lookup(name: &str, size: u32, scale: u32) -> Option<IconEntry> {
    let (roots, fallback) = search_paths();
    let entries = lookup_entries(name, &system_theme(), &roots, &fallback);
    entry_for_size(&entries, size as i32, scale.max(1) as i32).cloned()
}

/// `QPixmapIconEngine::adjustSize`: downscale `actual` to fit `expected` keeping the
/// aspect ratio (QSize::scale, KeepAspectRatio); never upscale.
pub fn adjust_size(expected: (u32, u32), actual: (u32, u32)) -> (u32, u32) {
    let (ew, eh) = (expected.0 as i64, expected.1 as i64);
    let (aw, ah) = (actual.0 as i64, actual.1 as i64);
    if aw <= ew && ah <= eh || aw == 0 || ah == 0 {
        return actual;
    }
    // QSize::scaled(KeepAspectRatio): rw = ew*ah/aw rounded down (qint64 division)…
    let rw = eh * aw / ah;
    let (w, h) = if rw <= ew {
        (rw, eh)
    } else {
        (ew, ew * ah / aw)
    };
    (w.max(1) as u32, h.max(1) as u32)
}

/// `QSize::scaled(target, KeepAspectRatio)` (up or down), as `QSvgIconEngine` sizes a
/// document's default size to the requested icon size.
pub fn fit_size(target: (u32, u32), size: (u32, u32)) -> (u32, u32) {
    let (tw, th) = (target.0 as i64, target.1 as i64);
    let (w, h) = (size.0.max(1) as i64, size.1.max(1) as i64);
    let rw = th * w / h;
    let (w, h) = if rw <= tw { (rw, th) } else { (tw, tw * h / w) };
    (w.max(1) as u32, h.max(1) as u32)
}

fn calc_points(s: usize, d: usize) -> Vec<usize> {
    let (s, d) = (s as i64, d as i64);
    let up = d >= s;
    let mut val: i64 = if up { 0x8000 * s / d - 0x8000 } else { 0 };
    let inc = (s << 16) / d;
    (0..d)
        .map(|_| {
            let p = (val >> 16).max(0) as usize;
            val += inc;
            p
        })
        .collect()
}

fn calc_apoints(s: usize, d: usize) -> Vec<i64> {
    // Scaling down only (callers never upscale).
    let (s, d) = (s as i64, d as i64);
    let mut val: i64 = 0;
    let inc = (s << 16) / d;
    let cp = ((d << 14) + s - 1) / s;
    (0..d)
        .map(|_| {
            let ap = ((0x10000 - (val & 0xffff)) * cp) >> 16;
            val += inc;
            ap | (cp << 16)
        })
        .collect()
}

/// Qt's `qSmoothScaleImage` for a downscale on both axes
/// (`qt_qimageScaleAARGBA_down_xy`): 14-bit fixed-point area averaging over
/// **premultiplied** RGBA8 (QImage::smoothScaled converts to ARGB32_Premultiplied).
/// `src` is `sw*sh*4` premultiplied RGBA; returns `dw*dh*4`.
pub fn smooth_scale_down(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    assert!(dw > 0 && dh > 0 && dw <= sw && dh <= sh && src.len() >= sw * sh * 4);
    let xp = calc_points(sw, dw);
    let yp = calc_points(sh, dh);
    let xap = calc_apoints(sw, dw);
    let yap = calc_apoints(sh, dh);
    // Reads past the last row/column (the final weight is 0 there) are clamped.
    let px = |x: usize, y: usize| -> [i64; 4] {
        let o = (y.min(sh - 1) * sw + x.min(sw - 1)) * 4;
        [
            src[o] as i64,
            src[o + 1] as i64,
            src[o + 2] as i64,
            src[o + 3] as i64,
        ]
    };
    let row = |y: usize, x: usize, xa: i64, cx: i64| -> [i64; 4] {
        let mut acc = px(x, y).map(|c| c * xa);
        let mut x = x;
        let mut j = (1 << 14) - xa;
        while j > cx {
            x += 1;
            let p = px(x, y);
            for c in 0..4 {
                acc[c] += p[c] * cx;
            }
            j -= cx;
        }
        x += 1;
        let p = px(x, y);
        for c in 0..4 {
            acc[c] += p[c] * j;
        }
        acc
    };
    let mut out = vec![0u8; dw * dh * 4];
    for y in 0..dh {
        let cy = yap[y] >> 16;
        let ya = yap[y] & 0xffff;
        for x in 0..dw {
            let cx = xap[x] >> 16;
            let xa = xap[x] & 0xffff;
            let mut sy = yp[y];
            let mut acc = row(sy, xp[x], xa, cx).map(|v| (v >> 4) * ya);
            let mut j = (1 << 14) - ya;
            while j > cy {
                sy += 1;
                let r = row(sy, xp[x], xa, cx);
                for c in 0..4 {
                    acc[c] += (r[c] >> 4) * cy;
                }
                j -= cy;
            }
            sy += 1;
            let r = row(sy, xp[x], xa, cx);
            for c in 0..4 {
                acc[c] += (r[c] >> 4) * j;
            }
            let o = (y * dw + x) * 4;
            for c in 0..4 {
                out[o + c] = ((acc[c] >> 24) & 0xff) as u8;
            }
        }
    }
    out
}

/// Quickshell's missing-icon pixmap for a `size`-px request (measured on the live shell
/// at 36 px: even size, black top-left/bottom-right and #d900d8 top-right/bottom-left
/// quadrants). Opaque RGBA8 (premultiplied == straight); returns `(side, pixels)`.
pub fn missing_icon(size: u32) -> (u32, Vec<u8>) {
    let side = size.max(2).div_ceil(2) * 2;
    let half = side / 2;
    let mut px = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let o = ((y * side + x) * 4) as usize;
            let magenta = (x >= half) != (y >= half);
            let c = if magenta {
                [0xd9, 0x00, 0xd8, 0xff]
            } else {
                [0, 0, 0, 0xff]
            };
            px[o..o + 4].copy_from_slice(&c);
        }
    }
    (side, px)
}

/// The tray icon pixmap the live shell gets for theme icon `name`:
/// `QIcon::fromTheme(name).pixmap(request)` with `request` in **device** pixels (the
/// `IconImage` sourceSize × DPR, e.g. 36 for the 18-px tray slot at DPR 2), resolved at
/// scale 1. Raster entries larger than the request are smooth-downscaled with Qt's area
/// averaging ([`adjust_size`] + [`smooth_scale_down`]); smaller ones are returned unchanged
/// (never upscaled — Qt's scene graph stretches them into the quad). Premultiplied RGBA8.
/// `None` when nothing resolves or the entry is scalable (callers render SVG themselves).
pub fn tray_raster(name: &str, request: u32) -> Option<hyprv_ui::skia::Image> {
    use hyprv_ui::skia as sk;
    let entry = lookup(name, request.max(1), 1)?;
    if entry.kind != EntryKind::Pixmap {
        return None;
    }
    let bytes = read_bounded_file(&entry.file, MAX_CACHE_BYTES)?;
    let image = sk::Image::from_encoded(sk::Data::new_copy(&bytes))?;
    let (w, h) = (image.width() as u32, image.height() as u32);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let (dw, dh) = adjust_size((request, request), (w, h));
    let info = |w: u32, h: u32| {
        sk::ImageInfo::new(
            (w as i32, h as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        )
    };
    let mut src = vec![0u8; (w * h * 4) as usize];
    if !image.read_pixels(
        &info(w, h),
        &mut src,
        w as usize * 4,
        (0, 0),
        sk::image::CachingHint::Allow,
    ) {
        return None;
    }
    let out = if (dw, dh) == (w, h) {
        src
    } else {
        smooth_scale_down(&src, w as usize, h as usize, dw as usize, dh as usize)
    };
    sk::images::raster_from_data(&info(dw, dh), sk::Data::new_copy(&out), dw as usize * 4)
}

/// The parsed `Directories` of a theme (diagnostics).
pub fn theme_directories(name: &str, roots: &[PathBuf]) -> Vec<IconDir> {
    load_theme(name, roots).dirs
}

// WIDGETS-R2-1/R2-2 regressions (cache cycles, chain cap, bounded reads).
#[cfg(test)]
#[path = "icon_theme_regressions.rs"]
mod icon_theme_regressions;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hyprv-icon-theme-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// The synthetic, colour-coded theme of the Qt probe (directories in index order;
    /// Qt reorders them).
    fn synthetic(tag: &str) -> (PathBuf, Vec<PathBuf>) {
        let root = tmp(tag);
        type DirectoryFixture<'a> = (&'a str, &'a str, i32, Option<(i32, i32)>, i32);
        let dirs: [DirectoryFixture<'_>; 12] = [
            ("16x16/apps", "Threshold", 16, None, 1),
            ("22x22/apps", "Threshold", 22, None, 1),
            ("24x24/apps", "Threshold", 24, None, 1),
            ("32x32/apps", "Threshold", 32, None, 1),
            ("36x36/apps", "Threshold", 36, None, 1),
            ("48x48/apps", "Threshold", 48, None, 1),
            ("64x64/apps", "Threshold", 64, None, 1),
            ("128x128/apps", "Threshold", 128, None, 1),
            ("256x256/apps", "Scalable", 256, Some((64, 256)), 1),
            ("512x512/apps", "Scalable", 512, Some((64, 512)), 1),
            ("scalable/apps", "Scalable", 128, Some((1, 256)), 1),
            ("32x32@2/apps", "Threshold", 32, None, 2),
        ];
        let mut ini = format!(
            "[Icon Theme]\nName=Hicolor\nDirectories={}\n\n",
            dirs.iter().map(|d| d.0).collect::<Vec<_>>().join(",")
        );
        for (path, kind, size, minmax, scale) in dirs {
            ini += &format!("[{path}]\nSize={size}\n");
            if let Some((mn, mx)) = minmax {
                ini += &format!("MinSize={mn}\nMaxSize={mx}\n");
            }
            if scale != 1 {
                ini += &format!("Scale={scale}\n");
            }
            ini += &format!("Context=Applications\nType={kind}\n\n");
        }
        let sys = root.join("sys/icons");
        let user = root.join("user/icons");
        std::fs::create_dir_all(sys.join("hicolor")).unwrap();
        std::fs::write(sys.join("hicolor/index.theme"), ini).unwrap();
        let put = |base: &Path, d: &str, name: &str| {
            let ext = if d.starts_with("scalable") {
                "svg"
            } else {
                "png"
            };
            touch(&base.join("hicolor").join(d).join(format!("{name}.{ext}")));
        };
        for d in [
            "64x64/apps",
            "48x48/apps",
            "32x32/apps",
            "256x256/apps",
            "24x24/apps",
            "16x16/apps",
            "128x128/apps",
        ] {
            put(&sys, d, "chromeish");
        }
        put(&sys, "256x256/apps", "discordish");
        put(&sys, "scalable/apps", "kpish");
        put(&sys, "256x256/apps", "kpish");
        for d in [
            "64x64/apps",
            "48x48/apps",
            "32x32/apps",
            "256x256/apps",
            "16x16/apps",
            "128x128/apps",
            "512x512/apps",
        ] {
            put(&sys, d, "wechatish");
        }
        put(&user, "256x256/apps", "wechatish");
        for (n, d) in [
            ("only32", "32x32/apps"),
            ("only48", "48x48/apps"),
            ("only64", "64x64/apps"),
            ("only128", "128x128/apps"),
        ] {
            put(&sys, d, n);
        }
        put(&sys, "32x32@2/apps", "at2");
        put(&sys, "32x32/apps", "at2");
        put(&sys, "48x48/apps", "pair4864");
        put(&sys, "64x64/apps", "pair4864");
        put(&sys, "16x16/apps", "pair16_128");
        put(&sys, "128x128/apps", "pair16_128");
        put(&sys, "256x256/apps", "big256");
        put(&sys, "512x512/apps", "big256");
        (root, vec![user, sys])
    }

    #[test]
    fn entry_choice_matches_real_qt_6_12_on_a_synthetic_theme() {
        // Ground truth: QIcon::fromTheme(name).pixmap(QSize(size, size), dpr) in real Qt
        // 6.12 (Wayland platform, generic theme) on this theme; each file was a distinct
        // colour, so the returned pixmap named the chosen directory ("user:" = the
        // user-level content dir). Requests: 36@1 18@2 36@2 72@1 20@2 30@2 40@2 50@1 100@1.
        let (root, roots) = synthetic("qt612");
        let reqs = [
            (36, 1),
            (18, 2),
            (36, 2),
            (72, 1),
            (20, 2),
            (30, 2),
            (40, 2),
            (50, 1),
            (100, 1),
        ];
        let truth: [(&str, [&str; 9]); 12] = [
            (
                "chromeish",
                [
                    "32x32", "32x32", "24x24", "256x256", "48x48", "64x64", "128x128", "48x48",
                    "256x256",
                ],
            ),
            ("discordish", ["256x256"; 9]),
            (
                "kpish",
                [
                    "scalable", "scalable", "scalable", "256x256", "scalable", "scalable",
                    "scalable", "scalable", "256x256",
                ],
            ),
            (
                "wechatish",
                [
                    "32x32",
                    "32x32",
                    "user:256x256",
                    "512x512",
                    "48x48",
                    "64x64",
                    "user:256x256",
                    "48x48",
                    "512x512",
                ],
            ),
            ("only32", ["32x32"; 9]),
            ("only48", ["48x48"; 9]),
            ("only64", ["64x64"; 9]),
            ("only128", ["128x128"; 9]),
            (
                "at2",
                [
                    "32x32", "32x32", "32x32@2", "32x32@2", "32x32", "32x32@2", "32x32@2",
                    "32x32@2", "32x32@2",
                ],
            ),
            (
                "pair4864",
                [
                    "48x48", "48x48", "64x64", "64x64", "48x48", "64x64", "64x64", "48x48", "64x64",
                ],
            ),
            (
                "pair16_128",
                [
                    "16x16", "16x16", "16x16", "16x16", "16x16", "16x16", "128x128", "16x16",
                    "128x128",
                ],
            ),
            (
                "big256",
                [
                    "512x512", "512x512", "256x256", "512x512", "512x512", "512x512", "256x256",
                    "512x512", "512x512",
                ],
            ),
        ];
        for (name, want) in truth {
            let entries = lookup_entries(name, "hicolor", &roots, &[]);
            for ((size, scale), want) in reqs.iter().zip(want) {
                let e = entry_for_size(&entries, *size, *scale).unwrap();
                let user = e.file.starts_with(&roots[0]);
                let got = format!(
                    "{}{}",
                    if user { "user:" } else { "" },
                    e.dir.path.trim_end_matches("/apps")
                );
                assert_eq!(got, want, "{name} at {size}@{scale}");
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn theme_chain_dash_and_unthemed_fallbacks() {
        let root = tmp("chain");
        let icons = root.join("icons");
        std::fs::create_dir_all(icons.join("hicolor")).unwrap();
        std::fs::write(
            icons.join("hicolor/index.theme"),
            "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps,48x48/status\n[48x48/apps]\nSize=48\nContext=Applications\nType=Threshold\n[48x48/status]\nSize=48\nContext=Status\nType=Threshold\n",
        )
        .unwrap();
        touch(&icons.join("hicolor/48x48/apps/fcitx.png"));
        touch(&icons.join("hicolor/48x48/apps/google-chrome.png"));
        touch(&icons.join("hicolor/48x48/status/network.png"));
        std::fs::create_dir_all(icons.join("Fluent/scalable/apps")).unwrap();
        std::fs::write(
            icons.join("Fluent/index.theme"),
            "[Icon Theme]\nName=Fluent\nInherits=Child\nDirectories=scalable/apps\n[scalable/apps]\nSize=16\nMinSize=16\nMaxSize=512\nContext=Applications\nType=Scalable\n",
        )
        .unwrap();
        touch(&icons.join("Fluent/scalable/apps/telegram.svg"));
        let pixmaps = root.join("pixmaps");
        touch(&pixmaps.join("legacy-app.png"));
        let (r, f) = (vec![icons.clone()], vec![pixmaps.clone()]);
        let pick = |name: &str, theme: &str| {
            entry_for_size(&lookup_entries(name, theme, &r, &f), 36, 2).map(|e| e.file.clone())
        };
        // Only in Fluent: missing for the live shell's hicolor.
        assert_eq!(pick("telegram", "hicolor"), None);
        assert_eq!(
            pick("telegram", "Fluent"),
            Some(icons.join("Fluent/scalable/apps/telegram.svg"))
        );
        // Inherits (missing "Child") then hicolor.
        assert_eq!(
            pick("fcitx", "Fluent"),
            Some(icons.join("hicolor/48x48/apps/fcitx.png"))
        );
        // Dash fallback skips Applications/MimeTypes directories (Qt's generic fallback).
        assert_eq!(pick("google-chrome-beta", "hicolor"), None);
        assert_eq!(
            pick("network-wired", "hicolor"),
            Some(icons.join("hicolor/48x48/status/network.png"))
        );
        // Unthemed pixmaps fallback.
        assert_eq!(
            pick("legacy-app", "hicolor"),
            Some(pixmaps.join("legacy-app.png"))
        );
        assert_eq!(pick("", "hicolor"), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gtk_cache_reader_matches_qt_on_the_system_cache() {
        // Values from a Python port of QIconCacheGtkReader on this machine's cache;
        // skipped when the system has no (valid) cache or lacks these icons.
        let dir = PathBuf::from("/usr/share/icons/hicolor");
        let Some(cache) = gtk_cache(&dir) else { return };
        // The system cache can legitimately omit an uninstalled icon; this
        // optional integration check must follow its documented absence rule.
        if let Some(dirs) = gtk_cache_lookup(&cache, "telegram") {
            assert!(dirs.is_empty());
        }
        if dir.join("256x256/apps/discord.png").is_file()
            && !dir.join("scalable/apps/discord.svg").exists()
        {
            assert_eq!(
                gtk_cache_lookup(&cache, "discord"),
                Some(vec!["256x256/apps".to_string()])
            );
        }
        if let Some(dirs) = gtk_cache_lookup(&cache, "google-chrome") {
            for d in &dirs {
                assert!(
                    dir.join(d).join("google-chrome.png").is_file()
                        || dir.join(d).join("google-chrome.svg").is_file(),
                    "{d}"
                );
            }
        }
    }

    #[test]
    fn pixmaps_are_downscaled_keeping_aspect_but_never_upscaled() {
        assert_eq!(adjust_size((36, 36), (32, 32)), (32, 32));
        assert_eq!(adjust_size((36, 36), (256, 256)), (36, 36));
        assert_eq!(adjust_size((36, 36), (256, 128)), (36, 18));
        assert_eq!(adjust_size((36, 36), (100, 300)), (12, 36));
    }

    #[test]
    fn smooth_scale_down_matches_qt_area_averaging() {
        // Uniform image stays uniform (weights sum to 1<<14 per axis).
        let src = vec![200u8; 256 * 256 * 4];
        let out = smooth_scale_down(&src, 256, 256, 36, 36);
        assert!(
            out.iter().all(|&v| (198..=200).contains(&v)),
            "{:?}",
            &out[..8]
        );
        // 2:1 averages pairs: [0,255] columns → ~127.
        let mut s = vec![0u8; 4 * 2 * 4];
        for y in 0..2 {
            for x in 0..4 {
                let v = if x % 2 == 1 { 255 } else { 0 };
                s[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let o = smooth_scale_down(&s, 4, 2, 2, 1);
        assert!((126..=128).contains(&o[0]) && o[3] >= 253, "{o:?}");
    }

    #[test]
    fn smooth_scale_down_golden_against_the_reference_validated_port() {
        // Goldens from the Python port of qt_qimageScaleAARGBA_down_xy that reproduces the
        // live shell's discord tray icon (256 -> 36) to max channel diff 4 over the
        // reference capture (parity/fullscreen); pattern is premultiplied, 50x50 -> 18x11.
        let (w, h) = (50usize, 50usize);
        let mut src = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let a = (x * 7 + y * 3) % 256;
                let px = [
                    (x * 13) % 256 * a / 255,
                    (y * 17) % 256 * a / 255,
                    ((x ^ y) * 5) % 256 * a / 255,
                    a,
                ];
                for c in 0..4 {
                    src[(y * w + x) * 4 + c] = px[c] as u8;
                }
            }
        }
        let out = smooth_scale_down(&src, w, h, 18, 11);
        let at = |x: usize, y: usize| out[(y * 18 + x) * 4..(y * 18 + x) * 4 + 4].to_vec();
        assert_eq!(at(0, 0), [0, 1, 0, 11]);
        assert_eq!(at(5, 3), [112, 79, 44, 149]);
        assert_eq!(at(17, 10), [98, 55, 69, 222]);
        assert_eq!(at(9, 7), [8, 5, 2, 26]);
    }

    #[test]
    fn real_hicolor_matches_real_qt_on_this_machine() {
        // Real Qt 6.12, QIcon::fromTheme(name).pixmap(36, 36) at qApp DPR 2 on this
        // machine: google-chrome → 64x64 (hicolor 64x64 PNG), discord → 72 (256 PNG).
        // Skipped where the system theme or these files are absent.
        let roots = vec![PathBuf::from("/usr/share/icons")];
        let base = roots[0].join("hicolor");
        if !base.join("index.theme").is_file() || gtk_cache(&base).is_none() {
            return;
        }
        let pick = |n: &str| {
            entry_for_size(&lookup_entries(n, "hicolor", &roots, &[]), 36, 2)
                .map(|e| e.file.clone())
        };
        if base.join("64x64/apps/google-chrome.png").is_file()
            && base.join("256x256/apps/google-chrome.png").is_file()
        {
            assert_eq!(
                pick("google-chrome").unwrap(),
                base.join("64x64/apps/google-chrome.png")
            );
        }
        if base.join("256x256/apps/discord.png").is_file()
            && !base.join("scalable/apps/discord.svg").exists()
        {
            assert_eq!(
                pick("discord").unwrap(),
                base.join("256x256/apps/discord.png")
            );
        }
    }

    #[test]
    fn missing_icon_is_quickshells_checker() {
        let (side, px) = missing_icon(36);
        assert_eq!(side, 36);
        let at =
            |x: u32, y: u32| &px[((y * side + x) * 4) as usize..((y * side + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0), [0, 0, 0, 255]);
        assert_eq!(at(35, 0), [0xd9, 0, 0xd8, 255]);
        assert_eq!(at(0, 35), [0xd9, 0, 0xd8, 255]);
        assert_eq!(at(35, 35), [0, 0, 0, 255]);
        assert_eq!(missing_icon(35).0, 36);
        assert_eq!(missing_icon(0).0, 2);
    }

    #[test]
    fn system_theme_is_qt_generic_fallback() {
        if std::env::var_os("HYPRV_ICON_THEME").is_none() {
            assert_eq!(system_theme(), "hicolor");
        }
    }

    /// Diagnostic: `HYPRV_ICON_LOOKUP=name,… cargo test -p hyprv-widgets --lib
    /// lookup_dump -- --ignored --nocapture` prints the file Qt's rules pick at 36 @ 2.
    #[test]
    #[ignore]
    fn lookup_dump() {
        let Ok(names) = std::env::var("HYPRV_ICON_LOOKUP") else { return };
        let (roots, fallback) = search_paths();
        println!("roots {roots:?} fallback {fallback:?}");
        for n in names.split(',') {
            println!("{n}: {:?}", lookup(n, 36, 2).map(|e| e.file));
        }
    }
}
