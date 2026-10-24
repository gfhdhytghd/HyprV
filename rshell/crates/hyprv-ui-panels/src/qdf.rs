//! Qt's distance-field glyph generation, ported exactly (Qt 6.12):
//!
//! - glyph outline as `QRawFont::pathForGlyph` at 54·16 px builds it
//!   (QFontEngineFT::addGlyphsToPath + QFreetypeFace::addGlyphToPath, qfontengine_ft.cpp),
//! - `QPainterPath` element rules and `boundingRect()` (qpainterpath.cpp),
//! - `qSimplifyPath` (qpathsimplifier.cpp, incl. QRBTree from qrbtree_p.h),
//! - `makeDistanceField` (qdistancefield.cpp): 24.8 fixed point polygon fill, edge
//!   ramps and 15° join fans, `byte = (0x7f80 − v) >> 8`.
//!
//! All integer arithmetic follows the C++ (i32, arithmetic shifts, truncating division).
#![allow(clippy::too_many_arguments)]
use freetype as ft;

pub const BASE: i32 = 54;
pub const SCALE: i32 = 16;
pub const RADIUS: i32 = 80;
/// Field margin in base pixels (RADIUS / SCALE).
pub const MARGIN: i32 = RADIUS / SCALE;

// ------------------------------------------------------------------ QPainterPath

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    MoveTo,
    LineTo,
    CurveTo,
    CurveData,
}

#[derive(Clone, Copy, Debug)]
pub struct El {
    pub x: f64,
    pub y: f64,
    pub t: Ty,
}

fn fuzzy_compare(a: f64, b: f64) -> bool {
    // QtPrivate::fuzzyCompare as used by qFuzzyCompare(QPointF, QPointF)
    if a == 0. || b == 0. {
        (a - b).abs() <= 0.000000000001
    } else {
        (a - b).abs() * 1000000000000. <= a.abs().min(b.abs())
    }
}

#[derive(Default)]
pub struct Path {
    pub els: Vec<El>,
    c_start: usize,
    require_move_to: bool,
}

impl Path {
    fn ensure(&mut self) {
        if self.els.is_empty() {
            self.els.push(El { x: 0., y: 0., t: Ty::MoveTo });
        }
    }
    fn maybe_move_to(&mut self) {
        if self.require_move_to {
            let mut e = *self.els.last().unwrap();
            e.t = Ty::MoveTo;
            self.els.push(e);
            self.require_move_to = false;
        }
    }
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.ensure();
        self.require_move_to = false;
        let last = self.els.last_mut().unwrap();
        if last.t == Ty::MoveTo {
            last.x = x;
            last.y = y;
        } else {
            self.els.push(El { x, y, t: Ty::MoveTo });
        }
        self.c_start = self.els.len() - 1;
    }
    fn eq_last(&self, x: f64, y: f64) -> bool {
        let l = self.els.last().unwrap();
        fuzzy_compare(l.x, x) && fuzzy_compare(l.y, y)
    }
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.ensure();
        self.maybe_move_to();
        if self.eq_last(x, y) {
            return;
        }
        self.els.push(El { x, y, t: Ty::LineTo });
    }
    pub fn cubic_to(&mut self, c1: (f64, f64), c2: (f64, f64), e: (f64, f64)) {
        self.ensure();
        self.maybe_move_to();
        let eq = |a: (f64, f64), b: (f64, f64)| fuzzy_compare(a.0, b.0) && fuzzy_compare(a.1, b.1);
        let l = self.els.last().unwrap();
        if eq((l.x, l.y), c1) && eq(c1, c2) && eq(c2, e) {
            return;
        }
        self.els.push(El { x: c1.0, y: c1.1, t: Ty::CurveTo });
        self.els.push(El { x: c2.0, y: c2.1, t: Ty::CurveData });
        self.els.push(El { x: e.0, y: e.1, t: Ty::CurveData });
    }
    pub fn close(&mut self) {
        if self.els.is_empty() {
            return;
        }
        self.require_move_to = true;
        let first = self.els[self.c_start];
        let last = self.els.len() - 1;
        if first.x != self.els[last].x || first.y != self.els[last].y {
            if fuzzy_compare(first.x, self.els[last].x) && fuzzy_compare(first.y, self.els[last].y) {
                self.els[last].x = first.x;
                self.els[last].y = first.y;
            } else {
                self.els.push(El { x: first.x, y: first.y, t: Ty::LineTo });
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.els.is_empty() || (self.els.len() == 1 && self.els[0].t == Ty::MoveTo)
    }
    pub fn translate(&mut self, dx: f64, dy: f64) {
        if dx == 0. && dy == 0. {
            return;
        }
        for e in &mut self.els {
            e.x += dx;
            e.y += dy;
        }
    }
    /// QPainterPath::boundingRect (x, y, w, h).
    pub fn bounding_rect(&self) -> (f64, f64, f64, f64) {
        if self.els.is_empty() {
            return (0., 0., 0., 0.);
        }
        let (mut minx, mut maxx) = (self.els[0].x, self.els[0].x);
        let (mut miny, mut maxy) = (self.els[0].y, self.els[0].y);
        let mut i = 1;
        while i < self.els.len() {
            let e = self.els[i];
            match e.t {
                Ty::MoveTo | Ty::LineTo => {
                    if e.x > maxx {
                        maxx = e.x;
                    } else if e.x < minx {
                        minx = e.x;
                    }
                    if e.y > maxy {
                        maxy = e.y;
                    } else if e.y < miny {
                        miny = e.y;
                    }
                }
                Ty::CurveTo => {
                    let p = self.els[i - 1];
                    let b = [p.x, p.y, e.x, e.y, self.els[i + 1].x, self.els[i + 1].y, self.els[i + 2].x, self.els[i + 2].y];
                    let (rx, ry, rw, rh) = bezier_extrema(&b);
                    let (right, bottom) = (rx + rw, ry + rh);
                    if rx < minx {
                        minx = rx;
                    }
                    if right > maxx {
                        maxx = right;
                    }
                    if ry < miny {
                        miny = ry;
                    }
                    if bottom > maxy {
                        maxy = bottom;
                    }
                    i += 2;
                }
                Ty::CurveData => {}
            }
            i += 1;
        }
        (minx, miny, maxx - minx, maxy - miny)
    }
}

fn fuzzy_is_null(v: f64) -> bool {
    v.abs() <= 0.000000000001
}

fn bezier_point(b: &[f64; 8], t: f64) -> (f64, f64) {
    let m_t = 1. - t;
    let x = {
        let mut a = b[0] * m_t + b[2] * t;
        let mut bb = b[2] * m_t + b[4] * t;
        let c = b[4] * m_t + b[6] * t;
        a = a * m_t + bb * t;
        bb = bb * m_t + c * t;
        a * m_t + bb * t
    };
    let y = {
        let mut a = b[1] * m_t + b[3] * t;
        let mut bb = b[3] * m_t + b[5] * t;
        let c = b[5] * m_t + b[7] * t;
        a = a * m_t + bb * t;
        bb = bb * m_t + c * t;
        a * m_t + bb * t
    };
    (x, y)
}

/// qt_painterpath_bezier_extrema
fn bezier_extrema(b: &[f64; 8]) -> (f64, f64, f64, f64) {
    let (x1, y1, x2, y2, x3, y3, x4, y4) = (b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]);
    let (mut minx, mut maxx) = if x1 < x4 { (x1, x4) } else { (x4, x1) };
    let (mut miny, mut maxy) = if y1 < y4 { (y1, y4) } else { (y4, y1) };
    let check = |t: f64, minx: &mut f64, maxx: &mut f64, miny: &mut f64, maxy: &mut f64| {
        if (0. ..=1.).contains(&t) {
            let p = bezier_point(b, t);
            if p.0 < *minx {
                *minx = p.0;
            } else if p.0 > *maxx {
                *maxx = p.0;
            }
            if p.1 < *miny {
                *miny = p.1;
            } else if p.1 > *maxy {
                *maxy = p.1;
            }
        }
    };
    {
        let ax = 3. * (-x1 + 3. * x2 - 3. * x3 + x4);
        let bx = 6. * (x1 - 2. * x2 + x3);
        let cx = 3. * (-x1 + x2);
        if fuzzy_is_null(ax) {
            if !fuzzy_is_null(bx) {
                check(-cx / bx, &mut minx, &mut maxx, &mut miny, &mut maxy);
            }
        } else {
            let tx = bx * bx - 4. * ax * cx;
            if tx >= 0. {
                let temp = tx.sqrt();
                let rcp = 1. / (2. * ax);
                check((-bx + temp) * rcp, &mut minx, &mut maxx, &mut miny, &mut maxy);
                check((-bx - temp) * rcp, &mut minx, &mut maxx, &mut miny, &mut maxy);
            }
        }
    }
    {
        let ay = 3. * (-y1 + 3. * y2 - 3. * y3 + y4);
        let by = 6. * (y1 - 2. * y2 + y3);
        let cy = 3. * (-y1 + y2);
        if fuzzy_is_null(ay) {
            if !fuzzy_is_null(by) {
                check(-cy / by, &mut minx, &mut maxx, &mut miny, &mut maxy);
            }
        } else {
            let ty = by * by - 4. * ay * cy;
            if ty > 0. {
                let temp = ty.sqrt();
                let rcp = 1. / (2. * ay);
                check((-by + temp) * rcp, &mut minx, &mut maxx, &mut miny, &mut maxy);
                check((-by - temp) * rcp, &mut minx, &mut maxx, &mut miny, &mut maxy);
            }
        }
    }
    (minx, miny, maxx - minx, maxy - miny)
}

// ------------------------------------------------------------------ glyph path

/// `QRawFont::pathForGlyph` for a reference font of `pixel_size` px: unscaled load
/// (FT_LOAD_NO_BITMAP at units_per_EM ppem, default hinting), optional
/// FT_GlyphSlot_Embolden, then scaled with FT_MulFix and converted to a path.
pub fn glyph_path(face: &ft::Face<&'static [u8]>, glyph: u32, pixel_size: i32, embolden: bool) -> Path {
    let mut path = Path::default();
    unsafe {
        let raw = face.raw() as *const ft::ffi::FT_FaceRec as *mut ft::ffi::FT_FaceRec;
        let upem = (*raw).units_per_EM as ft::ffi::FT_F26Dot6;
        if ft::ffi::FT_Set_Char_Size(raw, upem << 6, upem << 6, 0, 0) != 0 {
            return path;
        }
        let mut m = ft::ffi::FT_Matrix { xx: 0x10000, xy: 0, yx: 0, yy: 0x10000 };
        ft::ffi::FT_Set_Transform(raw, &mut m, std::ptr::null_mut());
        if ft::ffi::FT_Load_Glyph(raw, glyph, ft::face::LoadFlag::NO_BITMAP.bits()) != 0 {
            return path;
        }
        let g = (*raw).glyph;
        if (*g).format != ft::ffi::FT_GLYPH_FORMAT_OUTLINE {
            return path;
        }
        if embolden {
            ft::ffi::FT_GlyphSlot_Embolden(g);
        }
        // xsize = ysize = qRound(pixelSize * 64)
        let size = (pixel_size as i64 * 64) as ft::ffi::FT_Fixed;
        let x_scale = ft::ffi::FT_MulDiv(size, 1 << 10, upem as ft::ffi::FT_Long);
        let y_scale = x_scale;
        let outline = &mut (*g).outline;
        let n = outline.n_points as usize;
        let pts = std::slice::from_raw_parts_mut(outline.points, n);
        for p in pts.iter_mut() {
            p.x = ft::ffi::FT_MulFix(p.x, x_scale);
            p.y = ft::ffi::FT_MulFix(p.y, y_scale);
        }
        let tags = std::slice::from_raw_parts(outline.tags, n);
        let contours = std::slice::from_raw_parts(outline.contours, outline.n_contours as usize);
        add_glyph_to_path(&mut path, pts, tags, contours);
    }
    path
}

fn add_glyph_to_path(path: &mut Path, pts: &[ft::ffi::FT_Vector], tags: &[std::os::raw::c_char], contours: &[std::os::raw::c_short]) {
    let factor = 1. / 64.;
    let pt = |i: usize| (pts[i].x as f64 * factor, -(pts[i].y as f64) * factor);
    let tag = |i: usize| tags[i] as u8;
    let mut i: isize = 0;
    for &last in contours {
        let last_point = last as isize;
        let mut start = pt(i as usize);
        if tag(i as usize) & 1 == 0 {
            if tag(last_point as usize) & 1 == 0 {
                let e = pt(last_point as usize);
                start = ((e.0 + start.0) / 2.0, (e.1 + start.1) / 2.0);
            } else {
                start = pt(last_point as usize);
            }
            i -= 1;
        }
        path.move_to(start.0, start.1);
        let mut c = [(0f64, 0f64); 4];
        c[0] = start;
        let mut n = 1usize;
        while i < last_point {
            i += 1;
            c[n] = pt(i as usize);
            n += 1;
            match tag(i as usize) & 3 {
                2 => {
                    if n < 4 {
                        continue;
                    }
                    c[3] = ((c[3].0 + c[2].0) / 2., (c[3].1 + c[2].1) / 2.);
                    i -= 1;
                }
                0 => {
                    if n < 3 {
                        continue;
                    }
                    c[3] = ((c[1].0 + c[2].0) / 2., (c[1].1 + c[2].1) / 2.);
                    c[2] = ((2. * c[1].0 + c[3].0) / 3., (2. * c[1].1 + c[3].1) / 3.);
                    c[1] = ((2. * c[1].0 + c[0].0) / 3., (2. * c[1].1 + c[0].1) / 3.);
                    i -= 1;
                }
                _ => {
                    if n == 2 {
                        path.line_to(c[1].0, c[1].1);
                        c[0] = c[1];
                        n = 1;
                        continue;
                    } else if n == 3 {
                        c[3] = c[2];
                        c[2] = ((2. * c[1].0 + c[3].0) / 3., (2. * c[1].1 + c[3].1) / 3.);
                        c[1] = ((2. * c[1].0 + c[0].0) / 3., (2. * c[1].1 + c[0].1) / 3.);
                    }
                }
            }
            path.cubic_to(c[1], c[2], c[3]);
            c[0] = c[3];
            n = 1;
        }
        if n == 1 {
            path.close();
        } else {
            c[3] = start;
            if n == 2 {
                c[2] = ((2. * c[1].0 + c[3].0) / 3., (2. * c[1].1 + c[3].1) / 3.);
                c[1] = ((2. * c[1].0 + c[0].0) / 3., (2. * c[1].1 + c[0].1) / 3.);
            }
            path.cubic_to(c[1], c[2], c[3]);
        }
        i += 1;
    }
}

// ------------------------------------------------------------------ QRBTree

#[derive(Clone, Copy)]
struct RbNode {
    data: usize,
    parent: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
    red: bool,
}

#[derive(Default)]
struct RbTree {
    n: Vec<RbNode>,
    root: Option<usize>,
    free: Vec<usize>,
}

impl RbTree {
    fn new_node(&mut self, data: usize) -> usize {
        let node = RbNode { data, parent: None, left: None, right: None, red: true };
        if let Some(i) = self.free.pop() {
            self.n[i] = node;
            i
        } else {
            self.n.push(node);
            self.n.len() - 1
        }
    }
    fn set_ref(&mut self, node: usize, to: Option<usize>) {
        // Node *&ref = parent ? (node == parent->left ? parent->left : parent->right) : root
        match self.n[node].parent {
            Some(p) => {
                if self.n[p].left == Some(node) {
                    self.n[p].left = to;
                } else {
                    self.n[p].right = to;
                }
            }
            None => self.root = to,
        }
    }
    fn rotate_left(&mut self, node: usize) {
        let r = self.n[node].right.unwrap();
        self.set_ref(node, Some(r));
        self.n[r].parent = self.n[node].parent;
        self.n[node].right = self.n[r].left;
        if let Some(l) = self.n[r].left {
            self.n[l].parent = Some(node);
        }
        self.n[r].left = Some(node);
        self.n[node].parent = Some(r);
    }
    fn rotate_right(&mut self, node: usize) {
        let l = self.n[node].left.unwrap();
        self.set_ref(node, Some(l));
        self.n[l].parent = self.n[node].parent;
        self.n[node].left = self.n[l].right;
        if let Some(r) = self.n[l].right {
            self.n[r].parent = Some(node);
        }
        self.n[l].right = Some(node);
        self.n[node].parent = Some(l);
    }
    fn update(&mut self, mut node: usize) {
        loop {
            let Some(parent) = self.n[node].parent else {
                self.n[node].red = false;
                return;
            };
            if !self.n[parent].red {
                return;
            }
            let grandpa = self.n[parent].parent.unwrap();
            let uncle = if Some(parent) == self.n[grandpa].left { self.n[grandpa].right } else { self.n[grandpa].left };
            if let Some(u) = uncle.filter(|&u| self.n[u].red) {
                self.n[parent].red = false;
                self.n[u].red = false;
                self.n[grandpa].red = true;
                node = grandpa;
                continue;
            }
            if Some(node) == self.n[parent].right && Some(parent) == self.n[grandpa].left {
                node = parent;
                self.rotate_left(node);
            } else if Some(node) == self.n[parent].left && Some(parent) == self.n[grandpa].right {
                node = parent;
                self.rotate_right(node);
            }
            let parent = self.n[node].parent.unwrap();
            if Some(parent) == self.n[grandpa].left {
                self.rotate_right(grandpa);
            } else {
                self.rotate_left(grandpa);
            }
            self.n[parent].red = false;
            self.n[grandpa].red = true;
            return;
        }
    }
    fn front(&self, mut node: usize) -> usize {
        while let Some(l) = self.n[node].left {
            node = l;
        }
        node
    }
    fn back(&self, mut node: usize) -> usize {
        while let Some(r) = self.n[node].right {
            node = r;
        }
        node
    }
    fn next(&self, mut node: usize) -> Option<usize> {
        if let Some(r) = self.n[node].right {
            return Some(self.front(r));
        }
        while let Some(p) = self.n[node].parent {
            if self.n[p].right == Some(node) {
                node = p;
            } else {
                break;
            }
        }
        self.n[node].parent
    }
    fn previous(&self, mut node: usize) -> Option<usize> {
        if let Some(l) = self.n[node].left {
            return Some(self.back(l));
        }
        while let Some(p) = self.n[node].parent {
            if self.n[p].left == Some(node) {
                node = p;
            } else {
                break;
            }
        }
        self.n[node].parent
    }
    fn attach_left(&mut self, parent: usize, child: usize) {
        self.n[parent].left = Some(child);
        self.n[child].parent = Some(parent);
        self.update(child);
    }
    fn attach_right(&mut self, parent: usize, child: usize) {
        self.n[parent].right = Some(child);
        self.n[child].parent = Some(parent);
        self.update(child);
    }
    fn attach_after(&mut self, parent: Option<usize>, child: usize) {
        match (self.root, parent) {
            (None, _) => {
                self.root = Some(child);
                self.update(child);
            }
            (Some(root), None) => {
                let f = self.front(root);
                self.attach_left(f, child);
            }
            (Some(_), Some(p)) => {
                if let Some(r) = self.n[p].right {
                    let f = self.front(r);
                    self.attach_left(f, child);
                } else {
                    self.attach_right(p, child);
                }
            }
        }
    }
    fn swap_nodes(&mut self, n1: usize, n2: usize) {
        if self.n[n1].parent == Some(n2) {
            self.n[n1].parent = self.n[n2].parent;
            self.n[n2].parent = Some(n1);
        } else if self.n[n2].parent == Some(n1) {
            self.n[n2].parent = self.n[n1].parent;
            self.n[n1].parent = Some(n2);
        } else {
            let t = self.n[n1].parent;
            self.n[n1].parent = self.n[n2].parent;
            self.n[n2].parent = t;
        }
        let (l1, r1, red1) = (self.n[n1].left, self.n[n1].right, self.n[n1].red);
        self.n[n1].left = self.n[n2].left;
        self.n[n1].right = self.n[n2].right;
        self.n[n1].red = self.n[n2].red;
        self.n[n2].left = l1;
        self.n[n2].right = r1;
        self.n[n2].red = red1;
        match self.n[n1].parent {
            Some(p) => {
                if self.n[p].left == Some(n2) {
                    self.n[p].left = Some(n1);
                } else {
                    self.n[p].right = Some(n1);
                }
            }
            None => self.root = Some(n1),
        }
        match self.n[n2].parent {
            Some(p) => {
                if self.n[p].left == Some(n1) {
                    self.n[p].left = Some(n2);
                } else {
                    self.n[p].right = Some(n2);
                }
            }
            None => self.root = Some(n2),
        }
        for n in [n1, n2] {
            if let Some(l) = self.n[n].left {
                self.n[l].parent = Some(n);
            }
            if let Some(r) = self.n[n].right {
                self.n[r].parent = Some(n);
            }
        }
    }
    fn rebalance(&mut self, mut node: usize) {
        loop {
            let Some(parent) = self.n[node].parent else { return };
            let mut sibling = if self.n[parent].left == Some(node) { self.n[parent].right.unwrap() } else { self.n[parent].left.unwrap() };
            if self.n[sibling].red {
                self.n[sibling].red = false;
                self.n[parent].red = true;
                if self.n[parent].left == Some(node) {
                    self.rotate_left(parent);
                } else {
                    self.rotate_right(parent);
                }
                let parent = self.n[node].parent.unwrap();
                sibling = if self.n[parent].left == Some(node) { self.n[parent].right.unwrap() } else { self.n[parent].left.unwrap() };
            }
            let parent = self.n[node].parent.unwrap();
            let red = |t: &Self, x: Option<usize>| x.is_some_and(|x| t.n[x].red);
            if !red(self, self.n[sibling].left) && !red(self, self.n[sibling].right) {
                let was_red = self.n[parent].red;
                self.n[sibling].red = true;
                self.n[parent].red = false;
                if was_red {
                    return;
                }
                node = parent;
                continue;
            }
            if self.n[parent].left == Some(node) {
                if !red(self, self.n[sibling].right) {
                    self.n[sibling].red = true;
                    let sl = self.n[sibling].left.unwrap();
                    self.n[sl].red = false;
                    self.rotate_right(sibling);
                    sibling = self.n[sibling].parent.unwrap();
                }
                self.n[sibling].red = self.n[parent].red;
                self.n[parent].red = false;
                let sr = self.n[sibling].right.unwrap();
                self.n[sr].red = false;
                self.rotate_left(parent);
            } else {
                if !red(self, self.n[sibling].left) {
                    self.n[sibling].red = true;
                    let sr = self.n[sibling].right.unwrap();
                    self.n[sr].red = false;
                    self.rotate_left(sibling);
                    sibling = self.n[sibling].parent.unwrap();
                }
                self.n[sibling].red = self.n[parent].red;
                self.n[parent].red = false;
                let sl = self.n[sibling].left.unwrap();
                self.n[sl].red = false;
                self.rotate_right(parent);
            }
            return;
        }
    }
    fn detach(&mut self, node: usize) {
        if let Some(r) = self.n[node].right {
            let f = self.front(r);
            self.swap_nodes(node, f);
        }
        let child = self.n[node].left.or(self.n[node].right);
        if !self.n[node].red {
            if let Some(c) = child.filter(|&c| self.n[c].red) {
                self.n[c].red = false;
            } else {
                self.rebalance(node);
            }
        }
        self.set_ref(node, child);
        if let Some(c) = child {
            self.n[c].parent = self.n[node].parent;
        }
        self.n[node].left = None;
        self.n[node].right = None;
        self.n[node].parent = None;
    }
    fn delete_node(&mut self, node: usize) {
        self.detach(node);
        self.free.push(node);
    }
}

// ------------------------------------------------------------------ PathSimplifier

type P = (i32, i32);
const FP: i32 = 256; // Q_FIXED_POINT_SCALE
const END: u32 = u32::MAX;

fn lt(a: P, b: P) -> bool {
    a.1 < b.1 || (a.1 == b.1 && a.0 < b.0)
}
fn cross(u: P, v: P) -> i32 {
    u.0.wrapping_mul(v.1).wrapping_sub(u.1.wrapping_mul(v.0))
}
fn dot(u: P, v: P) -> i32 {
    u.0.wrapping_mul(v.0).wrapping_add(u.1.wrapping_mul(v.1))
}
fn sub(a: P, b: P) -> P {
    (a.0.wrapping_sub(b.0), a.1.wrapping_sub(b.1))
}
fn add(a: P, b: P) -> P {
    (a.0.wrapping_add(b.0), a.1.wrapping_add(b.1))
}

#[derive(Clone, Copy, Default)]
struct Frac {
    num: u32,
    den: u32,
}
fn gcd(mut x: u32, mut y: u32) -> u32 {
    while y != 0 {
        let z = y;
        y = x % y;
        x = z;
    }
    x
}
fn fraction(n: u32, d: u32) -> Frac {
    if n == 0 {
        Frac { num: 0, den: 1 }
    } else {
        let g = gcd(n, d);
        Frac { num: n / g, den: d / g }
    }
}
#[derive(Clone, Copy, Default)]
struct Rational {
    int: i32,
    frac: Frac,
}
#[derive(Clone, Copy, Default)]
struct Isect {
    x: Rational,
    y: Rational,
}
impl Isect {
    fn valid(&self) -> bool {
        self.x.frac.den != 0 && self.y.frac.den != 0
    }
    fn accurate(&self) -> bool {
        self.x.frac.num == 0 && self.y.frac.num == 0
    }
    fn round(&self) -> P {
        let mut r = (self.x.int, self.y.int);
        if 2 * self.x.frac.num >= self.x.frac.den {
            r.0 += 1;
        }
        if 2 * self.y.frac.num >= self.y.frac.den {
            r.1 += 1;
        }
        r
    }
}

fn point_side_of_line(p: P, v1: P, v2: P) -> i32 {
    let ux = v2.0 as i64 - v1.0 as i64;
    let uy = v2.1 as i64 - v1.1 as i64;
    let vx = p.0 as i64 - v1.0 as i64;
    let vy = p.1 as i64 - v1.1 as i64;
    let c = ux * vy - uy * vx;
    c.signum() as i32
}

fn intersection_point(u1: P, u2: P, v1: P, v2: P) -> Isect {
    let mut r = Isect::default();
    let u = sub(u2, u1);
    let v = sub(v2, v1);
    let mut d1 = cross(u, sub(v1, u1));
    let mut d2 = cross(u, sub(v2, u1));
    let mut det = d2.wrapping_sub(d1);
    let mut d3 = cross(v, sub(u1, v1));
    let mut d4 = d3.wrapping_sub(det);
    if det == 0 {
        return r;
    }
    if det < 0 {
        det = -det;
        d1 = -d1;
        d2 = -d2;
        d3 = -d3;
        d4 = -d4;
    }
    if d1 >= 0 || d2 <= 0 || d3 <= 0 || d4 >= 0 {
        return r;
    }
    let det64 = det as i64;
    if v.0 >= 0 {
        r.x.int = v1.0.wrapping_add(((-(v.0 as i64)) * d1 as i64 / det64) as i32);
        r.x.frac = fraction((((-(v.0 as i64)) * d1 as i64) % det64) as u32, det as u32);
    } else {
        r.x.int = v2.0.wrapping_add(((-(v.0 as i64)) * d2 as i64 / det64) as i32);
        r.x.frac = fraction((((-(v.0 as i64)) * d2 as i64) % det64) as u32, det as u32);
    }
    if v.1 >= 0 {
        r.y.int = v1.1.wrapping_add(((-(v.1 as i64)) * d1 as i64 / det64) as i32);
        r.y.frac = fraction((((-(v.1 as i64)) * d1 as i64) % det64) as u32, det as u32);
    } else {
        r.y.int = v2.1.wrapping_add(((-(v.1 as i64)) * d2 as i64 / det64) as i32);
        r.y.frac = fraction((((-(v.1 as i64)) * d2 as i64) % det64) as u32, det as u32);
    }
    r
}

#[derive(Clone, Copy, Default)]
struct Element {
    middle: P,
    indices: [u32; 4],
    next: Option<usize>,
    previous: Option<usize>,
    winding: i32,
    edge_node: Option<usize>,
    bvh_node: usize,
    degree: u8,
    processed: bool,
    pointing_up: bool,
    originally_pointing_up: bool,
}
impl Element {
    fn upper(&self) -> u32 {
        self.indices[if self.pointing_up { self.degree as usize } else { 0 }]
    }
    fn lower(&self) -> u32 {
        self.indices[if self.pointing_up { 0 } else { self.degree as usize }]
    }
    fn upper_mut(&mut self) -> &mut u32 {
        let i = if self.pointing_up { self.degree as usize } else { 0 };
        &mut self.indices[i]
    }
    fn lower_mut(&mut self) -> &mut u32 {
        let i = if self.pointing_up { 0 } else { self.degree as usize };
        &mut self.indices[i]
    }
    fn flip(&mut self) {
        let d = self.degree as usize;
        for i in 0..(d + 1) >> 1 {
            self.indices.swap(i, d - i);
        }
        self.pointing_up = !self.pointing_up;
    }
}

#[derive(Clone, Copy, Default)]
struct Bvh {
    leaf: bool,
    min: P,
    max: P,
    element: usize, // leaf
    left: usize,    // split
    right: usize,
}

#[derive(Clone, Copy)]
struct Event {
    point: P,
    ty: u8, // 0 = Upper, 1 = Lower
    element: usize,
}
fn event_lt(a: &Event, b: &Event) -> bool {
    if a.point == b.point {
        return a.ty < b.ty;
    }
    lt(b.point, a.point)
}

struct Simplifier {
    el: Vec<Element>,
    elements: Vec<usize>, // m_elements
    pts: Vec<P>,
    idx: Vec<u32>,
    bvh: Vec<Bvh>,
    tree: RbTree,
}

impl Simplifier {
    fn new_element(&mut self) -> usize {
        self.el.push(Element::default());
        self.el.len() - 1
    }
    fn line_element(&mut self, a: u32, b: u32) -> usize {
        let e = self.new_element();
        let (from, to) = (self.pts[a as usize], self.pts[b as usize]);
        self.el[e].degree = 1;
        self.el[e].indices[0] = a;
        self.el[e].indices[1] = b;
        self.el[e].middle = ((from.0 + to.0) >> 1, (from.1 + to.1) >> 1);
        e
    }

    fn init(&mut self, path: &Path, offs: f64, scale: f64) {
        let map = |x: f64, y: f64| -> P {
            // QTransform translate(offs) · scale(1/dfScale), then qRound(· 256)
            let tx = x * scale + offs;
            let ty = y * scale + offs;
            (qround(tx * FP as f64), qround(ty * FP as f64))
        };
        let els = &path.els;
        let mut move_to_index = 0u32;
        let mut previous_index = 0u32;
        let mut i = 0usize;
        while i < els.len() {
            let e = els[i];
            match e.t {
                Ty::MoveTo => {
                    if !self.pts.is_empty() {
                        let (from, to) = (self.pts[previous_index as usize], self.pts[move_to_index as usize]);
                        if from != to {
                            let el = self.line_element(previous_index, move_to_index);
                            self.elements.push(el);
                        }
                    }
                    previous_index = self.pts.len() as u32;
                    move_to_index = previous_index;
                    self.pts.push(map(e.x, e.y));
                }
                Ty::LineTo => {
                    let to = map(e.x, e.y);
                    let from = *self.pts.last().unwrap();
                    if to != from {
                        let n = self.pts.len() as u32;
                        self.pts.push(to);
                        let el = self.line_element(previous_index, n);
                        self.elements.push(el);
                        previous_index = n;
                    }
                }
                Ty::CurveTo => {
                    let start = previous_index;
                    let (c1, c2, en) = (els[i], els[i + 1], els[i + 2]);
                    let end = map(en.x, en.y);
                    previous_index = self.pts.len() as u32;
                    self.pts.push(end);
                    let pm = els[i - 1];
                    let x1 = pm.x + 1.5 * (c1.x - pm.x);
                    let y1 = pm.y + 1.5 * (c1.y - pm.y);
                    let x2 = en.x + 1.5 * (c2.x - en.x);
                    let y2 = en.y + 1.5 * (c2.y - en.y);
                    let el = self.new_element();
                    if (x1 - x2).abs() < 1e-3 && (y1 - y2).abs() < 1e-3 {
                        let ctrl = map(x1, y1);
                        self.set_quadratic(el, start, ctrl, previous_index);
                    } else {
                        let a = map(c1.x, c1.y);
                        let b = map(c2.x, c2.y);
                        self.set_cubic_simplify(el, start, a, b, previous_index);
                    }
                    self.elements.push(el);
                    i += 2;
                }
                Ty::CurveData => {}
            }
            i += 1;
        }
        if !self.pts.is_empty() {
            let (from, to) = (self.pts[previous_index as usize], self.pts[move_to_index as usize]);
            if from != to {
                let el = self.line_element(previous_index, move_to_index);
                self.elements.push(el);
            }
        }
        for &e in &self.elements {
            self.el[e].processed = false;
        }
    }

    fn set_quadratic(&mut self, e: usize, i1: u32, ctrl: P, i2: u32) -> bool {
        let (p1, p2) = (self.pts[i1 as usize], self.pts[i2 as usize]);
        if flatten_quadratic(p1, ctrl, p2) {
            let el = &mut self.el[e];
            el.degree = 1;
            el.indices[0] = i1;
            el.indices[1] = i2;
            el.middle = ((p1.0 + p2.0) >> 1, (p1.1 + p2.1) >> 1);
            false
        } else {
            let n = self.pts.len() as u32;
            let el = &mut self.el[e];
            el.degree = 2;
            el.indices[0] = i1;
            el.indices[1] = n;
            el.indices[2] = i2;
            el.middle = ((p1.0 + ctrl.0 + p2.0) / 3, (p1.1 + ctrl.1 + p2.1) / 3);
            self.pts.push(ctrl);
            true
        }
    }
    fn set_cubic(&mut self, e: usize, i1: u32, v: P, w: P, i2: u32) -> bool {
        let (u, q) = (self.pts[i1 as usize], self.pts[i2 as usize]);
        if flatten_cubic(u, v, w, q) {
            let el = &mut self.el[e];
            el.degree = 1;
            el.indices[0] = i1;
            el.indices[1] = i2;
            el.middle = ((u.0 + q.0) >> 1, (u.1 + q.1) >> 1);
            false
        } else {
            self.insert_cubic(e, i1, v, w, i2);
            true
        }
    }
    fn insert_cubic(&mut self, e: usize, i1: u32, v: P, w: P, i2: u32) {
        let (u, q) = (self.pts[i1 as usize], self.pts[i2 as usize]);
        let n = self.pts.len() as u32;
        let el = &mut self.el[e];
        el.degree = 3;
        el.indices = [i1, n, n + 1, i2];
        el.middle = ((u.0 + v.0 + w.0 + q.0) >> 2, (u.1 + v.1 + w.1 + q.1) >> 2);
        self.pts.push(v);
        self.pts.push(w);
    }
    fn set_cubic_simplify(&mut self, e: usize, i1: u32, v: P, w: P, i2: u32) {
        let (u, q) = (self.pts[i1 as usize], self.pts[i2 as usize]);
        if flatten_cubic(u, v, w, q) {
            let el = &mut self.el[e];
            el.degree = 1;
            el.indices[0] = i1;
            el.indices[1] = i2;
            el.middle = ((u.0 + q.0) >> 1, (u.1 + q.1) >> 1);
            return;
        }
        let intersecting = u == q || intersection_point(u, v, w, q).valid();
        if !intersecting {
            self.insert_cubic(e, i1, v, w, i2);
            return;
        }
        let pts = split_cubic(u, v, w, q).0;
        let pi = self.pts.len() as u32;
        self.pts.push(pts[2]);
        let e2 = self.new_element();
        self.elements.push(e2);
        self.set_cubic_simplify(e, i1, pts[0], pts[1], pi);
        self.set_cubic_simplify(e2, pi, pts[3], pts[4], i2);
    }

    fn build_tree(&mut self, list: &mut [usize]) -> usize {
        self.bvh.push(Bvh::default());
        let node = self.bvh.len() - 1;
        if list.len() == 1 {
            let e = list[0];
            self.el[e].bvh_node = node;
            let mut mn = self.pts[self.el[e].indices[0] as usize];
            let mut mx = mn;
            for i in 1..=self.el[e].degree as usize {
                let p = self.pts[self.el[e].indices[i] as usize];
                mn = (mn.0.min(p.0), mn.1.min(p.1));
                mx = (mx.0.max(p.0), mx.1.max(p.1));
            }
            self.bvh[node] = Bvh { leaf: true, min: mn, max: mx, element: e, left: 0, right: 0 };
            return node;
        }
        let mut mn = self.el[list[0]].middle;
        let mut mx = mn;
        for &e in list.iter().skip(1) {
            let p = self.el[e].middle;
            mn = (mn.0.min(p.0), mn.1.min(p.1));
            mx = (mx.0.max(p.0), mx.1.max(p.1));
        }
        let (comp, pivot) = if mx.0 - mn.0 > mx.1 - mn.1 { (0, (mx.0 + mn.0) >> 1) } else { (1, (mx.1 + mn.1) >> 1) };
        let key = |s: &Self, e: usize| if comp == 0 { s.el[e].middle.0 } else { s.el[e].middle.1 };
        let n = list.len();
        let (mut lo, mut hi) = (0usize, n - 1);
        while lo < hi {
            while lo < hi && key(self, list[lo]) <= pivot {
                lo += 1;
            }
            while lo < hi && key(self, list[hi]) > pivot {
                hi -= 1;
            }
            if lo < hi {
                list.swap(lo, hi);
            }
        }
        if lo == n {
            lo = n >> 1;
        }
        let (a, b) = list.split_at_mut(lo);
        let left = self.build_tree(a);
        let right = self.build_tree(b);
        let (l, r) = (self.bvh[left], self.bvh[right]);
        self.bvh[node] = Bvh {
            leaf: false,
            min: (l.min.0.min(r.min.0), l.min.1.min(r.min.1)),
            max: (l.max.0.max(r.max.0), l.max.1.max(r.max.1)),
            element: 0,
            left,
            right,
        };
        node
    }

    fn remove_intersections(&mut self) {
        let mut list: Vec<usize> = self.elements.clone();
        let root = self.build_tree(&mut list);
        let mut stack: Vec<usize> = self.elements.clone();
        while let Some(e) = stack.pop() {
            let node = self.el[e].bvh_node;
            if !self.el[e].processed && !self.intersect_nodes(&mut stack, node, root) {
                self.el[e].processed = true;
            }
        }
    }

    fn equal_elements(&self, e1: usize, e2: usize) -> bool {
        let (a, b) = (&self.el[e1], &self.el[e2]);
        if a.degree != b.degree {
            return false;
        }
        let d = a.degree as usize;
        let mut same = true;
        let mut opp = true;
        for i in 0..=d {
            same &= self.pts[a.indices[i] as usize] == self.pts[b.indices[i] as usize];
            opp &= self.pts[a.indices[d - i] as usize] == self.pts[b.indices[i] as usize];
        }
        same || opp
    }

    fn intersect_nodes(&mut self, stack: &mut Vec<usize>, en: usize, tn: usize) -> bool {
        let (eb, tb) = (self.bvh[en], self.bvh[tn]);
        if eb.min.0 >= tb.max.0 || eb.min.1 >= tb.max.1 || eb.max.0 <= tb.min.0 || eb.max.1 <= tb.min.1 {
            return false;
        }
        let element = eb.element;
        if tb.leaf {
            let ne = tb.element;
            if !self.el[ne].processed {
                return false;
            }
            if ne == element {
                return false;
            }
            if self.equal_elements(ne, element) {
                return false;
            }
            if self.el[element].degree == 1 && self.el[ne].degree == 1 {
                let u1 = self.pts[self.el[element].indices[0] as usize];
                let u2 = self.pts[self.el[element].indices[1] as usize];
                let v1 = self.pts[self.el[ne].indices[0] as usize];
                let v2 = self.pts[self.el[ne].indices[1] as usize];
                let is = intersection_point(u1, u2, v1, v2);
                if !is.valid() {
                    return false;
                }
                self.pts.push(is.round());
                let pi = self.pts.len() as u32 - 1;
                self.split_line_at(stack, tn, pi, !is.accurate());
                self.split_line_at(stack, en, pi, false)
            } else {
                let mut axes = Vec::new();
                self.append_axes(&mut axes, element);
                self.append_axes(&mut axes, ne);
                for &axis in &axes {
                    let r1 = self.axis_range(axis, element);
                    let r2 = self.axis_range(axis, ne);
                    if r1.0 >= r2.1 || r1.1 <= r2.0 {
                        return false;
                    }
                }
                if self.el[ne].degree > 1 {
                    self.split_curve(stack, tn);
                }
                if self.el[element].degree > 1 {
                    self.split_curve(stack, en);
                } else {
                    let (l, r) = (self.bvh[tn].left, self.bvh[tn].right);
                    if self.intersect_nodes(stack, en, l) {
                        return true;
                    }
                    if self.intersect_nodes(stack, en, r) {
                        return true;
                    }
                    return false;
                }
                true
            }
        } else {
            if self.intersect_nodes(stack, en, tb.left) {
                return true;
            }
            // tree may have changed; re-read right child
            let r = self.bvh[tn].right;
            if self.intersect_nodes(stack, en, r) {
                return true;
            }
            false
        }
    }

    fn split_line_at(&mut self, stack: &mut Vec<usize>, node: usize, pi: u32, process_again: bool) -> bool {
        let first = self.bvh[node].element;
        let u = self.pts[self.el[first].indices[0] as usize];
        let v = self.pts[self.el[first].indices[1] as usize];
        let p = self.pts[pi as usize];
        if u == p || v == p {
            return false;
        }
        if process_again {
            self.el[first].processed = false;
        }
        let second = self.new_element();
        self.el[second] = self.el[first];
        self.el[first].indices[1] = pi;
        self.el[second].indices[0] = pi;
        self.el[first].middle = ((u.0 + p.0) >> 1, (u.1 + p.1) >> 1);
        self.el[second].middle = ((v.0 + p.0) >> 1, (v.1 + p.1) >> 1);
        self.elements.push(second);
        let nb = self.bvh[node];
        let mut left = Bvh { leaf: true, min: nb.min, max: nb.max, element: first, left: 0, right: 0 };
        let mut right = Bvh { leaf: true, min: nb.min, max: nb.max, element: second, left: 0, right: 0 };
        if u.0 < v.0 {
            left.max.0 = p.0;
            right.min.0 = p.0;
        } else {
            left.min.0 = p.0;
            right.max.0 = p.0;
        }
        if u.1 < v.1 {
            left.max.1 = p.1;
            right.min.1 = p.1;
        } else {
            left.min.1 = p.1;
            right.max.1 = p.1;
        }
        self.bvh.push(left);
        let li = self.bvh.len() - 1;
        self.bvh.push(right);
        let ri = self.bvh.len() - 1;
        self.el[first].bvh_node = li;
        self.el[second].bvh_node = ri;
        self.bvh[node].leaf = false;
        self.bvh[node].left = li;
        self.bvh[node].right = ri;
        if !self.el[first].processed {
            stack.push(first);
            stack.push(second);
        }
        true
    }

    fn append_axes(&self, axes: &mut Vec<P>, e: usize) {
        let el = &self.el[e];
        let p = |i: usize| self.pts[el.indices[i] as usize];
        let n = |a: P, b: P| (a.1 - b.1, b.0 - a.0);
        let list: Vec<P> = match el.degree {
            3 => {
                let (u, v, w, q) = (p(0), p(1), p(2), p(3));
                vec![n(u, v), n(v, w), n(w, q), n(q, u), n(u, w), n(v, q)]
            }
            2 => {
                let (u, v, w) = (p(0), p(1), p(2));
                vec![n(u, v), n(v, w), n(w, u)]
            }
            _ => vec![n(p(0), p(1))],
        };
        for a in list {
            if a.0 != 0 || a.1 != 0 {
                axes.push(a);
            }
        }
    }
    fn axis_range(&self, axis: P, e: usize) -> (i32, i32) {
        let el = &self.el[e];
        let mut r = (0x7fffffff, -0x7fffffff);
        for i in 0..=el.degree as usize {
            let d = dot(axis, self.pts[el.indices[i] as usize]);
            r.0 = r.0.min(d);
            r.1 = r.1.max(d);
        }
        r
    }

    fn split_curve(&mut self, stack: &mut Vec<usize>, node: usize) {
        let first = self.bvh[node].element;
        let second = self.new_element();
        self.el[second] = self.el[first];
        self.elements.push(second);
        let fi = self.el[first].indices;
        let (u, v, w) = (self.pts[fi[0] as usize], self.pts[fi[1] as usize], self.pts[fi[2] as usize]);
        let mut accurate;
        if self.el[first].degree == 2 {
            let (pts, acc) = split_quadratic(u, v, w);
            accurate = acc;
            let pi = self.pts.len() as u32;
            self.pts.push(pts[1]);
            accurate &= self.set_quadratic(first, fi[0], pts[0], pi);
            let si2 = self.el[second].indices[2];
            accurate &= self.set_quadratic(second, pi, pts[2], si2);
        } else {
            let q = self.pts[fi[3] as usize];
            let (pts, acc) = split_cubic(u, v, w, q);
            accurate = acc;
            let pi = self.pts.len() as u32;
            self.pts.push(pts[2]);
            accurate &= self.set_cubic(first, fi[0], pts[0], pts[1], pi);
            let si3 = self.el[second].indices[3];
            accurate &= self.set_cubic(second, pi, pts[3], pts[4], si3);
        }
        if !accurate {
            self.el[first].processed = false;
            self.el[second].processed = false;
        }
        let bounds = |s: &Self, e: usize| {
            let mut mn = (i32::MAX, i32::MAX);
            let mut mx = (i32::MIN, i32::MIN);
            for i in 0..=s.el[e].degree as usize {
                let p = s.pts[s.el[e].indices[i] as usize];
                mn = (mn.0.min(p.0), mn.1.min(p.1));
                mx = (mx.0.max(p.0), mx.1.max(p.1));
            }
            (mn, mx)
        };
        let (lmn, lmx) = bounds(self, first);
        let (rmn, rmx) = bounds(self, second);
        self.bvh.push(Bvh { leaf: true, min: lmn, max: lmx, element: first, left: 0, right: 0 });
        let li = self.bvh.len() - 1;
        self.bvh.push(Bvh { leaf: true, min: rmn, max: rmx, element: second, left: 0, right: 0 });
        let ri = self.bvh.len() - 1;
        self.el[first].bvh_node = li;
        self.el[second].bvh_node = ri;
        self.bvh[node].leaf = false;
        self.bvh[node].left = li;
        self.bvh[node].right = ri;
        if !self.el[first].processed {
            stack.push(first);
            stack.push(second);
        }
    }

    fn p(&self, i: u32) -> P {
        self.pts[i as usize]
    }

    fn element_is_left_of(&self, left: usize, right: usize) -> bool {
        let (l, r) = (&self.el[left], &self.el[right]);
        let (lu, ll) = (self.p(l.upper()), self.p(l.lower()));
        let (ru, rl) = (self.p(r.upper()), self.p(r.lower()));
        if lu.0 < rl.0.min(ru.0) {
            return true;
        }
        if lu.0 > rl.0.max(ru.0) {
            return false;
        }
        let mut d = point_side_of_line(lu, rl, ru);
        if d == 0 {
            d = point_side_of_line(ll, rl, ru);
            if d == 0 {
                if r.degree > 1 {
                    d = point_side_of_line(ll, rl, self.p(r.indices[1]));
                    if d == 0 {
                        d = point_side_of_line(ll, rl, self.p(r.indices[2]));
                    }
                } else if l.degree > 1 {
                    d = point_side_of_line(self.p(l.indices[1]), rl, ru);
                    if d == 0 {
                        d = point_side_of_line(self.p(l.indices[2]), rl, ru);
                    }
                }
            }
        }
        d < 0
    }

    fn find_element_left_of(&self, e: usize, bounds: (Option<usize>, Option<usize>)) -> Option<usize> {
        let root = self.tree.root?;
        let mut current = bounds.0.unwrap_or_else(|| self.tree.front(root));
        let mut result = None;
        loop {
            if Some(current) == bounds.1 || self.element_is_left_of(e, self.tree.n[current].data) {
                break;
            }
            result = Some(current);
            match self.tree.next(current) {
                Some(n) => current = n,
                None => break,
            }
        }
        result
    }

    fn outer_bounds(&self, point: P) -> (Option<usize>, Option<usize>) {
        let mut current = self.tree.root;
        let mut result = (None, None);
        while let Some(c) = current {
            let el = &self.el[self.tree.n[c].data];
            let (v1, v2) = (self.p(el.lower()), self.p(el.upper()));
            if point == v1 || point == v2 {
                break;
            }
            let mut d = point_side_of_line(point, v1, v2);
            if d == 0 {
                if el.degree == 1 {
                    break;
                }
                d = point_side_of_line(point, v1, self.p(el.indices[1]));
                if d == 0 {
                    d = point_side_of_line(point, v1, self.p(el.indices[2]));
                }
            }
            if d < 0 {
                result.1 = Some(c);
                current = self.tree.n[c].left;
            } else {
                result.0 = Some(c);
                current = self.tree.n[c].right;
            }
        }
        let Some(mid) = current else { return result };
        let mut current = self.tree.n[mid].left;
        while let Some(c) = current {
            let el = &self.el[self.tree.n[c].data];
            let (v1, v2) = (self.p(el.lower()), self.p(el.upper()));
            let mut equal = point == v1 || point == v2;
            if !equal {
                let d = point_side_of_line(point, v1, v2);
                equal = d == 0 && el.degree == 1;
            }
            if equal {
                current = self.tree.n[c].left;
            } else {
                result.0 = Some(c);
                current = self.tree.n[c].right;
            }
        }
        let mut current = self.tree.n[mid].right;
        while let Some(c) = current {
            let el = &self.el[self.tree.n[c].data];
            let (v1, v2) = (self.p(el.lower()), self.p(el.upper()));
            let mut equal = point == v1 || point == v2;
            if !equal {
                let d = point_side_of_line(point, v1, v2);
                equal = d == 0 && el.degree == 1;
            }
            if equal {
                current = self.tree.n[c].right;
            } else {
                result.1 = Some(c);
                current = self.tree.n[c].left;
            }
        }
        result
    }

    fn after_first(&self, bounds_first: Option<usize>) -> Option<usize> {
        match bounds_first {
            Some(f) => self.tree.next(f),
            None => self.tree.root.map(|r| self.tree.front(r)),
        }
    }

    fn connect_elements(&mut self) -> bool {
        let mut events: Vec<Event> = Vec::with_capacity(self.elements.len() * 2);
        for k in 0..self.elements.len() {
            let e = self.elements[k];
            let el = &mut self.el[e];
            el.next = None;
            el.previous = None;
            el.winding = 0;
            el.edge_node = None;
            let u = self.pts[el.indices[0] as usize];
            let v = self.pts[el.indices[el.degree as usize] as usize];
            if u != v {
                el.pointing_up = lt(v, u);
                el.originally_pointing_up = el.pointing_up;
                let up = el.pointing_up;
                events.push(Event { point: u, ty: if up { 1 } else { 0 }, element: e });
                events.push(Event { point: v, ty: if up { 0 } else { 1 }, element: e });
            }
        }
        if !events.is_empty() {
            sort_events(&mut events);
        }
        let mut ordered: Vec<usize> = Vec::new();
        while let Some(&event) = events.last() {
            let ep = event.point;
            let bounds = self.outer_bounds(ep);
            let n = events.len();
            if event.ty == 1 && n > 2 {
                let rf = self.after_first(bounds.0);
                let rs = match bounds.1 {
                    Some(s) => self.tree.previous(s),
                    None => self.tree.root.map(|r| self.tree.back(r)),
                };
                let e2 = events[n - 2];
                let e3 = events[n - 3];
                if rf == rs && e2.ty == 0 && e3.point != ep {
                    let element = event.element;
                    let element2 = e2.element;
                    let edge = self.el[element].edge_node.unwrap();
                    self.tree.n[edge].data = element2;
                    self.el[element2].edge_node = Some(edge);
                    self.el[element].edge_node = None;
                    events.pop();
                    events.pop();
                    if self.el[element2].pointing_up != self.el[element].pointing_up {
                        self.el[element2].flip();
                    }
                    self.el[element2].winding = self.el[element].winding;
                    let mut winding = self.el[element].winding;
                    if self.el[element].originally_pointing_up {
                        winding += 1;
                    }
                    if winding == 0 || winding == 1 {
                        if self.el[element].pointing_up {
                            self.el[element].previous = Some(element2);
                            self.el[element2].next = Some(element);
                        } else {
                            self.el[element].next = Some(element2);
                            self.el[element2].previous = Some(element);
                        }
                    }
                    continue;
                }
            }
            ordered.clear();
            if self.tree.root.is_some() {
                let mut current = self.after_first(bounds.0);
                while current != bounds.1 {
                    let c = current.unwrap();
                    let e = self.tree.n[c].data;
                    let mut winding = self.el[e].winding;
                    if self.el[e].originally_pointing_up {
                        winding += 1;
                    }
                    let lower = self.p(self.el[e].lower());
                    if lower == ep {
                        if winding == 0 || winding == 1 {
                            ordered.push(e);
                        }
                    } else {
                        // split the line passing through the event point
                        let ee = &self.el[event.element];
                        let index_index = if (event.ty == 0) == ee.pointing_up { ee.degree as usize } else { 0 };
                        let pi = ee.indices[index_index];
                        let ue = self.new_element();
                        self.el[ue] = self.el[e];
                        let up = self.el[e].upper();
                        let _ = up;
                        *self.el[ue].lower_mut() = pi;
                        *self.el[e].upper_mut() = pi;
                        self.el[ue].edge_node = None;
                        self.el[e].next = None;
                        self.el[e].previous = None;
                        if let Some(nx) = self.el[ue].next {
                            self.el[nx].previous = Some(ue);
                        } else if let Some(pv) = self.el[ue].previous {
                            self.el[pv].next = Some(ue);
                        }
                        if self.el[e].pointing_up != self.el[e].originally_pointing_up {
                            self.el[e].flip();
                        }
                        if winding == 0 || winding == 1 {
                            ordered.push(ue);
                        }
                        self.elements.push(ue);
                    }
                    current = self.tree.next(c);
                }
            }
            while let Some(&ev) = events.last() {
                if ev.point != ep {
                    break;
                }
                if ev.ty == 0 {
                    let left = self.find_element_left_of(ev.element, bounds);
                    let node = self.tree.new_node(ev.element);
                    self.el[ev.element].edge_node = Some(node);
                    self.tree.attach_after(left, node);
                } else {
                    let node = self.el[ev.element].edge_node.unwrap();
                    self.tree.delete_node(node);
                    self.el[ev.element].edge_node = None;
                }
                events.pop();
            }
            if self.tree.root.is_some() {
                let mut current = self.after_first(bounds.0);
                let mut winding = bounds.0.map(|f| self.el[self.tree.n[f].data].winding).unwrap_or(0);
                while current != bounds.1 {
                    let c = current.unwrap();
                    let e = self.tree.n[c].data;
                    let mut ccw = winding & 1;
                    if self.el[e].originally_pointing_up {
                        winding -= 1;
                    } else {
                        winding += 1;
                        ccw ^= 1;
                    }
                    self.el[e].winding = winding;
                    if ccw == 0 {
                        self.el[e].flip();
                    }
                    current = self.tree.next(c);
                }
                let mut current = match bounds.1 {
                    Some(s) => self.tree.previous(s),
                    None => self.tree.root.map(|r| self.tree.back(r)),
                };
                while current != bounds.0 {
                    let c = current.unwrap();
                    let e = self.tree.n[c].data;
                    let mut winding = self.el[e].winding;
                    if self.el[e].originally_pointing_up {
                        winding += 1;
                    }
                    if winding == 0 || winding == 1 {
                        ordered.push(e);
                    }
                    current = self.tree.previous(c);
                }
            }
            if !ordered.is_empty() {
                if ordered.len() & 1 == 1 {
                    return false;
                }
                let mut i = 0;
                let first = ordered[0];
                if self.p(self.el[first].indices[0]) != ep {
                    ordered.push(first);
                    i = 1;
                }
                while i < ordered.len() {
                    let next = ordered[i];
                    let previous = ordered[i + 1];
                    self.el[next].previous = Some(previous);
                    self.el[previous].next = Some(next);
                    i += 2;
                }
            }
        }
        true
    }

    fn fill_indices(&mut self) {
        for &e in &self.elements {
            self.el[e].processed = false;
        }
        for k in 0..self.elements.len() {
            let start = self.elements[k];
            if self.el[start].processed || self.el[start].next.is_none() {
                continue;
            }
            let mut e = start;
            loop {
                self.idx.push(self.el[e].indices[0]);
                let ind = self.el[e].indices;
                match self.el[e].degree {
                    2 => {
                        let (a, b, c) = (self.p(ind[0]), self.p(ind[1]), self.p(ind[2]));
                        self.sub_div_quadratic(a, b, c);
                    }
                    3 => {
                        let (a, b, c, d) = (self.p(ind[0]), self.p(ind[1]), self.p(ind[2]), self.p(ind[3]));
                        self.sub_div_cubic(a, b, c, d);
                    }
                    _ => {}
                }
                self.el[e].processed = true;
                e = self.el[e].next.unwrap();
                if e == start {
                    break;
                }
            }
            self.idx.push(END);
        }
    }

    fn sub_div_quadratic(&mut self, u: P, v: P, w: P) {
        if flatten_quadratic(u, v, w) {
            return;
        }
        let pts = split_quadratic(u, v, w).0;
        self.sub_div_quadratic(u, pts[0], pts[1]);
        self.idx.push(self.pts.len() as u32);
        self.pts.push(pts[1]);
        self.sub_div_quadratic(pts[1], pts[2], w);
    }
    fn sub_div_cubic(&mut self, u: P, v: P, w: P, q: P) {
        if flatten_cubic(u, v, w, q) {
            return;
        }
        let pts = split_cubic(u, v, w, q).0;
        self.sub_div_cubic(u, pts[0], pts[1], pts[2]);
        self.idx.push(self.pts.len() as u32);
        self.pts.push(pts[2]);
        self.sub_div_cubic(pts[2], pts[3], pts[4], q);
    }
}

fn qround(d: f64) -> i32 {
    (d + 0.5).floor() as i32
}

fn flatten_quadratic(u: P, v: P, w: P) -> bool {
    let (d0, d1) = (sub(v, u), sub(w, v));
    let d = cross(d0, d1).wrapping_abs();
    let l = d0.0.abs() + d0.1.abs() + d1.0.abs() + d1.1.abs();
    d < (FP * FP * 3 / 2) || l <= FP * 2
}
fn flatten_cubic(u: P, v: P, w: P, q: P) -> bool {
    let ds = [sub(v, u), sub(w, v), sub(q, w), sub(q, u)];
    let d = cross(ds[0], ds[1]).wrapping_abs().wrapping_add(cross(ds[1], ds[2]).wrapping_abs()).wrapping_add(cross(ds[0], ds[3]).wrapping_abs()).wrapping_add(cross(ds[3], ds[2]).wrapping_abs());
    let l = ds[0].0.abs() + ds[0].1.abs() + ds[1].0.abs() + ds[1].1.abs() + ds[2].0.abs() + ds[2].1.abs();
    d < (FP * FP * 3) || l <= FP * 2
}
fn split_quadratic(u: P, v: P, w: P) -> ([P; 3], bool) {
    let r0 = add(u, v);
    let r2 = add(v, w);
    let r1 = add(r0, r2);
    let accurate = ((r0.0 | r0.1 | r2.0 | r2.1) & 1) == 0 && ((r1.0 | r1.1) & 3) == 0;
    ([(r0.0 >> 1, r0.1 >> 1), (r1.0 >> 2, r1.1 >> 2), (r2.0 >> 1, r2.1 >> 1)], accurate)
}
fn split_cubic(u: P, v: P, w: P, q: P) -> ([P; 5], bool) {
    let r0 = add(u, v);
    let mut r2 = add(v, w);
    let r4 = add(w, q);
    let r1 = add(r0, r2);
    let r3 = add(r2, r4);
    r2 = add(r1, r3);
    let accurate = ((r0.0 | r0.1 | r4.0 | r4.1) & 1) == 0 && ((r1.0 | r1.1 | r3.0 | r3.1) & 3) == 0 && ((r2.0 | r2.1) & 7) == 0;
    ([(r0.0 >> 1, r0.1 >> 1), (r1.0 >> 2, r1.1 >> 2), (r2.0 >> 3, r2.1 >> 3), (r3.0 >> 2, r3.1 >> 2), (r4.0 >> 1, r4.1 >> 1)], accurate)
}

/// PathSimplifier::sortEvents (bucket sort + insertion sort, order-exact).
fn sort_events(events: &mut [Event]) {
    let count = events.len();
    let mut buffer = events.to_vec();
    let mut bins = vec![0usize; count];
    let mut counts = [0usize; 0x101];
    let (mut mn, mut mx) = (events[0].point.1, events[0].point.1);
    for e in events.iter().skip(1) {
        mn = mn.min(e.point.1);
        mx = mx.max(e.point.1);
    }
    for i in 0..count {
        bins[i] = (((mx - events[i].point.1) << 8) / (mx - mn + 1)) as usize;
        counts[bins[i]] += 1;
    }
    for i in 1..0x100 {
        counts[i] += counts[i - 1];
    }
    counts[0x100] = counts[0xff];
    for i in 0..count {
        counts[bins[i]] -= 1;
        buffer[counts[bins[i]]] = events[i];
    }
    let mut j = 0usize;
    for i in 0..0x100 {
        while j < counts[i + 1] {
            let mut k = j;
            while k > 0 && event_lt(&buffer[j], &events[k - 1]) {
                events[k] = events[k - 1];
                k -= 1;
            }
            events[k] = buffer[j];
            j += 1;
        }
    }
}

/// qSimplifyPath with the distance-field transform (translate(offs), scale(1/dfScale)).
fn simplify(path: &Path, offs: i32, df_scale: i32) -> (Vec<P>, Vec<u32>) {
    let mut s = Simplifier { el: Vec::new(), elements: Vec::new(), pts: Vec::new(), idx: Vec::new(), bvh: Vec::new(), tree: RbTree::default() };
    s.init(path, offs as f64, 1. / df_scale as f64);
    let mut ok = true;
    if !s.elements.is_empty() {
        s.remove_intersections();
        ok = s.connect_elements();
        if ok {
            s.fill_indices();
        }
    }
    if !ok {
        return (Vec::new(), Vec::new());
    }
    (s.pts, s.idx)
}

// ------------------------------------------------------------------ makeDistanceField

#[derive(Clone, Copy)]
enum HDir {
    L2R,
    R2L,
}

fn fill_line(line: &mut [i32], width: i32, lx: i32, rx: i32, d: i32, dd: i32, clip: bool, dir: HDir) {
    let (from_x, to_x) = if clip { ((lx >> 8).max(0), (rx >> 8).min(width)) } else { (lx >> 8, rx >> 8) };
    let mut x = to_x - from_x;
    if x <= 0 {
        return;
    }
    match dir {
        HDir::L2R => {
            let mut val = if clip { d.wrapping_add((((from_x << 8) + 0xff - lx).wrapping_mul(dd)) >> 8) } else { d.wrapping_add(((!lx & 0xff).wrapping_mul(dd)) >> 8) };
            let mut i = from_x as usize;
            loop {
                let cur = line[i];
                line[i] = if val.wrapping_abs() < cur.wrapping_abs() { val } else { cur };
                val = val.wrapping_add(dd);
                i += 1;
                x -= 1;
                if x == 0 {
                    break;
                }
            }
        }
        HDir::R2L => {
            let mut val = if clip { d.wrapping_add((((to_x << 8) + 0xff - rx).wrapping_mul(dd)) >> 8) } else { d.wrapping_add(((!rx & 0xff).wrapping_mul(dd)) >> 8) };
            let mut i = to_x as usize;
            loop {
                val = val.wrapping_sub(dd);
                i -= 1;
                let cur = line[i];
                line[i] = if val.wrapping_abs() < cur.wrapping_abs() { val } else { cur };
                x -= 1;
                if x == 0 {
                    break;
                }
            }
        }
    }
}

fn fill_lines(bits: &mut [i32], width: i32, upper_y: i32, lower_y: i32, lx: &mut i32, ldx: i32, rx: &mut i32, rdx: i32, d: &mut i32, ddy: i32, ddx: i32, clip: bool, top_down: bool, dir: HDir) {
    let mut y = lower_y - upper_y;
    if top_down {
        let mut row = upper_y;
        loop {
            let s = (row * width) as usize;
            fill_line(&mut bits[s..s + width as usize], width, *lx, *rx, *d, ddx, clip, dir);
            *lx = lx.wrapping_add(ldx);
            *d = d.wrapping_add(ddy);
            *rx = rx.wrapping_add(rdx);
            row += 1;
            y -= 1;
            if y == 0 {
                break;
            }
        }
    } else {
        let mut row = lower_y;
        loop {
            *lx = lx.wrapping_sub(ldx);
            *d = d.wrapping_sub(ddy);
            *rx = rx.wrapping_sub(rdx);
            row -= 1;
            let s = (row * width) as usize;
            fill_line(&mut bits[s..s + width as usize], width, *lx, *rx, *d, ddx, clip, dir);
            y -= 1;
            if y == 0 {
                break;
            }
        }
    }
}

struct Img<'a> {
    bits: &'a mut [i32],
    w: i32,
    h: i32,
}

impl Img<'_> {
    #[allow(clippy::too_many_arguments)]
    fn lines(&mut self, clip: bool, top_down: bool, dir: HDir, upper: i32, lower: i32, lx: &mut i32, ldx: i32, rx: &mut i32, rdx: i32, d: &mut i32, ddy: i32, ddx: i32) {
        // fillLines asserts upperY < lowerY; callers guarantee it
        fill_lines(self.bits, self.w, upper, lower, lx, ldx, rx, rdx, d, ddy, ddx, clip, top_down, dir);
    }

    fn draw_triangle(&mut self, clip: bool, center: P, v1: P, v2: P, value: i32) {
        let height = self.h;
        let yb = |v: i32| if clip { (v >> 8).clamp(0, height) } else { v >> 8 };
        let frac = |yi: i32, v: i32| if clip { (yi << 8) + 0xff - v } else { !v & 0xff };
        let (y1, y2, yc) = (yb(v1.1), yb(v2.1), yb(center.1));
        let v1f = frac(y1, v1.1);
        let v2f = frac(y2, v2.1);
        let cf = frac(yc, center.1);
        let (mut dx1, mut x1, mut dx2, mut x2) = (0, 0, 0, 0);
        if v1.1 != center.1 {
            dx1 = ((v1.0 - center.0) << 8) / (v1.1 - center.1);
            x1 = center.0 + cf * (v1.0 - center.0) / (v1.1 - center.1);
        }
        if v2.1 != center.1 {
            dx2 = ((v2.0 - center.0) << 8) / (v2.1 - center.1);
            x2 = center.0 + cf * (v2.0 - center.0) / (v2.1 - center.1);
        }
        let div = (v2.0 - center.0).wrapping_mul(v1.1 - center.1).wrapping_sub((v2.1 - center.1).wrapping_mul(v1.0 - center.0));
        let dd = if div != 0 { (((value.wrapping_mul(v1.1 - v2.1)) as i64) << 8) / div as i64 } else { 0 } as i32;
        use HDir::*;
        if y2 < yc {
            if y1 < yc {
                if y2 < y1 {
                    let mut d1 = cf * value / (v1.1 - center.1);
                    let dd1 = (value << 8) / (v1.1 - center.1);
                    self.lines(clip, false, L2R, y1, yc, &mut x1, dx1, &mut x2, dx2, &mut d1, dd1, dd);
                    dx1 = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
                    x1 = v1.0 + v1f * (v1.0 - v2.0) / (v1.1 - v2.1);
                    let mut vv = value;
                    self.lines(clip, false, L2R, y2, y1, &mut x1, dx1, &mut x2, dx2, &mut vv, 0, dd);
                } else {
                    let mut d2 = cf * value / (v2.1 - center.1);
                    let dd2 = (value << 8) / (v2.1 - center.1);
                    self.lines(clip, false, R2L, y2, yc, &mut x1, dx1, &mut x2, dx2, &mut d2, dd2, dd);
                    if y1 != y2 {
                        dx2 = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
                        x2 = v2.0 + v2f * (v1.0 - v2.0) / (v1.1 - v2.1);
                        let mut vv = value;
                        self.lines(clip, false, R2L, y1, y2, &mut x1, dx1, &mut x2, dx2, &mut vv, 0, dd);
                    }
                }
            } else {
                let dx = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
                let base = if clip { (yc << 8) + 0xff - v2.1 } else { (center.1 | 0xff) - v2.1 };
                let mut x_up = v2.0 + base * (v1.0 - v2.0) / (v1.1 - v2.1);
                let mut x_dn = x_up;
                let mut vv = value;
                self.lines(clip, false, L2R, y2, yc, &mut x_up, dx, &mut x2, dx2, &mut vv, 0, dd);
                if yc != y1 {
                    let mut vv = value;
                    self.lines(clip, true, L2R, yc, y1, &mut x_dn, dx, &mut x1, dx1, &mut vv, 0, dd);
                }
            }
        } else if y1 < yc {
            let dx = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
            let base = if clip { (yc << 8) + 0xff - v1.1 } else { (center.1 | 0xff) - v1.1 };
            let mut x_up = v1.0 + base * (v1.0 - v2.0) / (v1.1 - v2.1);
            let mut x_dn = x_up;
            let mut vv = value;
            self.lines(clip, false, R2L, y1, yc, &mut x1, dx1, &mut x_up, dx, &mut vv, 0, dd);
            if yc != y2 {
                let mut vv = value;
                self.lines(clip, true, R2L, yc, y2, &mut x2, dx2, &mut x_dn, dx, &mut vv, 0, dd);
            }
        } else if y2 < y1 {
            if yc != y2 {
                let mut d2 = cf * value / (v2.1 - center.1);
                let dd2 = (value << 8) / (v2.1 - center.1);
                self.lines(clip, true, L2R, yc, y2, &mut x2, dx2, &mut x1, dx1, &mut d2, dd2, dd);
            }
            dx2 = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
            x2 = v2.0 + v2f * (v1.0 - v2.0) / (v1.1 - v2.1);
            let mut vv = value;
            self.lines(clip, true, L2R, y2, y1, &mut x2, dx2, &mut x1, dx1, &mut vv, 0, dd);
        } else {
            if yc != y1 {
                let mut d1 = cf * value / (v1.1 - center.1);
                let dd1 = (value << 8) / (v1.1 - center.1);
                self.lines(clip, true, R2L, yc, y1, &mut x2, dx2, &mut x1, dx1, &mut d1, dd1, dd);
            }
            if y1 != y2 {
                dx1 = ((v1.0 - v2.0) << 8) / (v1.1 - v2.1);
                x1 = v1.0 + v1f * (v1.0 - v2.0) / (v1.1 - v2.1);
                let mut vv = value;
                self.lines(clip, true, R2L, y1, y2, &mut x2, dx2, &mut x1, dx1, &mut vv, 0, dd);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_rectangle(&mut self, clip: bool, mut int1: P, mut center1: P, mut ext1: P, mut int2: P, mut center2: P, mut ext2: P, mut ext_value: i32) {
        if center1.1 > center2.1 {
            std::mem::swap(&mut center1, &mut center2);
            std::mem::swap(&mut int1, &mut ext2);
            std::mem::swap(&mut ext1, &mut int2);
            ext_value = -ext_value;
        }
        let height = self.h;
        let yb = |v: i32| if clip { (v >> 8).clamp(0, height) } else { v >> 8 };
        let frac = |yi: i32, v: i32| if clip { (yi << 8) + 0xff - v } else { !v & 0xff };
        let (yc1, yc2) = (yb(center1.1), yb(center2.1));
        let (yi1, yi2) = (yb(int1.1), yb(int2.1));
        let (ye1, ye2) = (yb(ext1.1), yb(ext2.1));
        let c1f = frac(yc1, center1.1);
        let c2f = frac(yc2, center2.1);
        let i1f = frac(yi1, int1.1);
        let e1f = frac(ye1, ext1.1);
        let (mut dx_c, mut dx_e, mut dd_c) = (0, 0, 0);
        if ext1.1 != int1.1 {
            dx_c = ((ext1.0 - int1.0) << 8) / (ext1.1 - int1.1);
            dd_c = (ext_value << 9) / (ext1.1 - int1.1);
        }
        if ext1.1 != ext2.1 {
            dx_e = ((ext1.0 - ext2.0) << 8) / (ext1.1 - ext2.1);
        }
        let div = (ext1.0 - int1.0).wrapping_mul(ext2.1 - int1.1).wrapping_sub((ext1.1 - int1.1).wrapping_mul(ext2.0 - int1.0));
        let dd = if div != 0 { (((ext_value.wrapping_mul(ext2.1 - ext1.1)) as i64) << 9) / div as i64 } else { 0 } as i32;
        let int_value = -ext_value;
        use HDir::*;
        if center2.0 < center1.0 {
            if int1.1 < ext2.1 {
                let mut xe1 = ext1.0 + e1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                let mut xe2 = int1.0 + i1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                if ye1 != yi1 {
                    let mut xc2 = center1.0 + c1f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                    xc2 += (ye1 - yc1) * dx_c;
                    let mut v = ext_value;
                    self.lines(clip, true, L2R, ye1, yi1, &mut xe1, dx_e, &mut xc2, dx_c, &mut v, 0, dd);
                }
                if yi1 != ye2 {
                    let mut v = ext_value;
                    self.lines(clip, true, L2R, yi1, ye2, &mut xe1, dx_e, &mut xe2, dx_e, &mut v, 0, dd);
                }
                if ye2 != yi2 {
                    let mut xc1 = center2.0 + c2f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                    xc1 += (ye2 - yc2) * dx_c;
                    let mut v = int_value;
                    self.lines(clip, true, R2L, ye2, yi2, &mut xc1, dx_c, &mut xe2, dx_e, &mut v, 0, dd);
                }
            } else {
                let mut xc1 = center2.0 + c2f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                let mut xc2 = center1.0 + c1f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                xc1 += (ye2 - yc2) * dx_c;
                xc2 += (ye1 - yc1) * dx_c;
                if ye1 != ye2 {
                    let mut xe1 = ext1.0 + e1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                    let mut v = ext_value;
                    self.lines(clip, true, L2R, ye1, ye2, &mut xe1, dx_e, &mut xc2, dx_c, &mut v, 0, dd);
                }
                if ye2 != yi1 {
                    let base = if clip { (ye2 << 8) + 0xff - center2.1 } else { (ext2.1 | 0xff) - center2.1 };
                    let mut d = base * 2 * ext_value / (ext1.1 - int1.1);
                    self.lines(clip, true, L2R, ye2, yi1, &mut xc1, dx_c, &mut xc2, dx_c, &mut d, dd_c, dd);
                }
                if yi1 != yi2 {
                    let mut xe2 = int1.0 + i1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                    let mut v = int_value;
                    self.lines(clip, true, R2L, yi1, yi2, &mut xc1, dx_c, &mut xe2, dx_e, &mut v, 0, dd);
                }
            }
        } else if ext1.1 < int2.1 {
            let mut xe1 = ext1.0 + e1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
            let mut xe2 = int1.0 + i1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
            if yi1 != ye1 {
                let mut xc1 = center1.0 + c1f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                xc1 += (yi1 - yc1) * dx_c;
                let mut v = int_value;
                self.lines(clip, true, R2L, yi1, ye1, &mut xc1, dx_c, &mut xe2, dx_e, &mut v, 0, dd);
            }
            if ye1 != yi2 {
                let mut v = int_value;
                self.lines(clip, true, R2L, ye1, yi2, &mut xe1, dx_e, &mut xe2, dx_e, &mut v, 0, dd);
            }
            if yi2 != ye2 {
                let mut xc2 = center2.0 + c2f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
                xc2 += (yi2 - yc2) * dx_c;
                let mut v = ext_value;
                self.lines(clip, true, L2R, yi2, ye2, &mut xe1, dx_e, &mut xc2, dx_c, &mut v, 0, dd);
            }
        } else {
            let mut xc1 = center1.0 + c1f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
            let mut xc2 = center2.0 + c2f * (ext1.0 - int1.0) / (ext1.1 - int1.1);
            xc1 += (yi1 - yc1) * dx_c;
            xc2 += (yi2 - yc2) * dx_c;
            if yi1 != yi2 {
                let mut xe2 = int1.0 + i1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                let mut v = int_value;
                self.lines(clip, true, R2L, yi1, yi2, &mut xc1, dx_c, &mut xe2, dx_e, &mut v, 0, dd);
            }
            if yi2 != ye1 {
                let base = if clip { (yi2 << 8) + 0xff - center2.1 } else { (int2.1 | 0xff) - center2.1 };
                let mut d = base * 2 * ext_value / (ext1.1 - int1.1);
                self.lines(clip, true, R2L, yi2, ye1, &mut xc1, dx_c, &mut xc2, dx_c, &mut d, dd_c, dd);
            }
            if ye1 != ye2 {
                let mut xe1 = ext1.0 + e1f * (ext1.0 - ext2.0) / (ext1.1 - ext2.1);
                let mut v = ext_value;
                self.lines(clip, true, L2R, ye1, ye2, &mut xe1, dx_e, &mut xc2, dx_c, &mut v, 0, dd);
            }
        }
    }

    fn draw_polygons(&mut self, vertices: &[P], indices: &[u32], value: i32) {
        let (width, height) = (self.w, self.h);
        let mut scans: Vec<Vec<u16>> = vec![Vec::new(); height as usize];
        let mut first = 0usize;
        let mut i = 1usize;
        while i < indices.len() {
            let idx1 = indices[i - 1];
            let mut idx2 = indices[i];
            if idx2 == END {
                idx2 = indices[first];
                i += 1;
                first = i;
            }
            let mut v1 = vertices[idx1 as usize];
            let mut v2 = vertices[idx2 as usize];
            if v2.1 < v1.1 {
                std::mem::swap(&mut v1, &mut v2);
            }
            let from_y = (v1.1 >> 8).max(0);
            let to_y = (v2.1 >> 8).min(height);
            if from_y < to_y {
                let dx = ((v2.0 - v1.0) << 8) / (v2.1 - v1.1);
                let mut x = v1.0 + ((from_y << 8) + 0xff - v1.1) * (v2.0 - v1.0) / (v2.1 - v1.1);
                for y in from_y..to_y {
                    let c = (x >> 8) as u32;
                    if c < width as u32 {
                        scans[y as usize].push(c as u16);
                    }
                    x += dx;
                }
            }
            i += 1;
        }
        for (row, scan) in scans.iter_mut().enumerate() {
            // insertion sort as in Qt (stable)
            for j in 1..scan.len() {
                let mut k = j;
                let v = scan[k];
                while k != 0 && v < scan[k - 1] {
                    scan[k] = scan[k - 1];
                    k -= 1;
                }
                scan[k] = v;
            }
            let line = &mut self.bits[row * width as usize..(row + 1) * width as usize];
            let mut j = 0;
            while j + 1 < scan.len() {
                for x in scan[j]..scan[j + 1] {
                    line[x as usize] = value;
                }
                j += 2;
            }
            if j < scan.len() {
                for pixel in line.iter_mut().take(width as usize).skip(scan[j] as usize) {
                    *pixel = value;
                }
            }
        }
    }
}

fn make_distance_field(width: usize, height: usize, path: &Path, df_scale: i32, offs: i32) -> Vec<u8> {
    let mut out = vec![0u8; width * height];
    if path.is_empty() {
        return out;
    }
    let (vertices, indices) = simplify(path, offs, df_scale);
    if vertices.is_empty() {
        return out;
    }
    let interior = -0x7f80;
    let exterior = 0x7f80;
    let mut bits = vec![exterior; width * height];
    let (iw, ih) = (width as i32, height as i32);
    let mut img = Img { bits: &mut bits, w: iw, h: ih };
    let angle = 15f64.to_radians();
    let rotation = (qround_signed(angle.cos() * 0x4000 as f64), qround_signed(angle.sin() * 0x4000 as f64));
    img.draw_polygons(&vertices, &indices, interior);
    let mut index = 0usize;
    while index < indices.len() {
        let mut end = index;
        while indices[end] != END {
            end += 1;
        }
        let mut normals: Vec<P> = Vec::new();
        let mut verts: Vec<P> = Vec::new();
        let mut needs_clip: Vec<bool> = Vec::new();
        let mut next = index;
        let mut prev = end - 1;
        while next < end {
            let from = vertices[indices[prev] as usize];
            let to = vertices[indices[next] as usize];
            let mut n = (to.1 - from.1, from.0 - to.0);
            if !(n.0 == 0 && n.1 == 0) {
                let len = ((n.0 as f64) * n.0 as f64 + (n.1 as f64) * n.1 as f64).sqrt();
                let scale = qround_signed(((offs << 16) as f64) / len);
                n.0 = n.0.wrapping_mul(scale) >> 8;
                n.1 = n.1.wrapping_mul(scale) >> 8;
                normals.push(n);
                verts.push((to.0 + 0x7f, to.1 + 0x7f));
                needs_clip.push(to.0 < offs << 8 || to.0 >= (iw - offs) << 8 || to.1 < offs << 8 || to.1 >= (ih - offs) << 8);
            }
            prev = next;
            next += 1;
        }
        let m = normals.len();
        let mut cps = vec![0i32; m];
        if m > 0 {
            let mut prev = m - 1;
            for next in 0..m {
                cps[prev] = normals[prev].0.wrapping_mul(normals[next].1).wrapping_sub(normals[prev].1.wrapping_mul(normals[next].0));
                prev = next;
            }
            let mut prev = m - 1;
            for next in 0..m {
                let mut n = normals[next];
                let mut int_prev = verts[prev];
                let mut ext_prev = verts[prev];
                let mut int_next = verts[next];
                let mut ext_next = verts[next];
                ext_prev = (ext_prev.0 - n.0, ext_prev.1 - n.1);
                int_prev = (int_prev.0 + n.0, int_prev.1 + n.1);
                ext_next = (ext_next.0 - n.0, ext_next.1 - n.1);
                int_next = (int_next.0 + n.0, int_next.1 + n.1);
                let clip = needs_clip[prev] || needs_clip[next];
                img.draw_rectangle(clip, int_prev, verts[prev], ext_prev, int_next, verts[next], ext_next, exterior);
                let cp = cps[prev];
                let clip_prev = needs_clip[prev];
                if cp < 0 {
                    loop {
                        let rn = ((n.0.wrapping_mul(rotation.0).wrapping_sub(n.1.wrapping_mul(rotation.1))) >> 14, (n.1.wrapping_mul(rotation.0).wrapping_add(n.0.wrapping_mul(rotation.1))) >> 14);
                        n = rn;
                        let np = normals[prev];
                        if n.0.wrapping_mul(np.1).wrapping_sub(n.1.wrapping_mul(np.0)) <= 0 {
                            let p = (verts[prev].0 - np.0, verts[prev].1 - np.1);
                            img.draw_triangle(clip_prev, verts[prev], ext_prev, p, exterior);
                            break;
                        }
                        let p = (verts[prev].0 - n.0, verts[prev].1 - n.1);
                        img.draw_triangle(clip_prev, verts[prev], ext_prev, p, exterior);
                        ext_prev = p;
                    }
                } else if cp > 0 {
                    loop {
                        let rn = ((n.0.wrapping_mul(rotation.0).wrapping_add(n.1.wrapping_mul(rotation.1))) >> 14, (n.1.wrapping_mul(rotation.0).wrapping_sub(n.0.wrapping_mul(rotation.1))) >> 14);
                        n = rn;
                        let np = normals[prev];
                        if n.0.wrapping_mul(np.1).wrapping_sub(n.1.wrapping_mul(np.0)) >= 0 {
                            let p = (verts[prev].0 + np.0, verts[prev].1 + np.1);
                            img.draw_triangle(clip_prev, verts[prev], p, int_prev, interior);
                            break;
                        }
                        let p = (verts[prev].0 + n.0, verts[prev].1 + n.1);
                        img.draw_triangle(clip_prev, verts[prev], p, int_prev, interior);
                        int_prev = p;
                    }
                }
                prev = next;
            }
        }
        index = end + 1;
    }
    for (o, &v) in out.iter_mut().zip(bits.iter()) {
        *o = ((0x7f80 - v) >> 8) as u8;
    }
    out
}

/// Qt 6 qRound(double) for either sign: round half away from zero.
fn qround_signed(d: f64) -> i32 {
    if d >= 0. {
        (d + 0.5) as i32
    } else {
        (d - 0.5) as i32
    }
}

/// One glyph's field as QSGDistanceFieldGlyphCache builds it.
pub struct Field {
    pub w: usize,
    pub h: usize,
    /// Glyph path bounding rect in base-font (54 px) units.
    pub bx: f64,
    pub by: f64,
    pub bw: f64,
    pub bh: f64,
    pub data: Vec<u8>,
}

pub fn field(face: &ft::Face<&'static [u8]>, glyph: u32, embolden: bool) -> Option<Field> {
    let mut path = glyph_path(face, glyph, BASE * SCALE, embolden);
    if path.is_empty() {
        return None;
    }
    // glyphData: boundingRect scaled down by 1/16 (exact power of two)
    let (x, y, w, h) = path.bounding_rect();
    let s = 1. / SCALE as f64;
    let (bx, by, bw, bh) = (x * s, y * s, w * s, h * s);
    if bw <= 0. || bh <= 0. {
        return None;
    }
    let fw = (bw + MARGIN as f64 * 2.).ceil() as usize;
    let fh = (bh + MARGIN as f64 * 2.).ceil() as usize;
    // QDistanceField(size, path, …): translate to the bbox origin, winding fill
    path.translate(-x, -y);
    let data = make_distance_field(fw, fh, &path, SCALE, MARGIN);
    Some(Field { w: fw, h: fh, bx, by, bw, bh, data })
}
