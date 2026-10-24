//! Logical-pixel geometry, explicit layout, and conservative damage accumulation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
    pub fn union(self, b: Self) -> Self {
        let x = self.x.min(b.x);
        let y = self.y.min(b.y);
        Self::new(
            x,
            y,
            (self.x + self.width).max(b.x + b.width) - x,
            (self.y + self.height).max(b.y + b.height) - y,
        )
    }
    pub fn outset(self, n: f32) -> Self {
        Self::new(
            self.x - n,
            self.y - n,
            self.width + n * 2.,
            self.height + n * 2.,
        )
    }
    pub fn physical(self, scale: f64) -> (i32, i32, i32, i32) {
        let x = (self.x as f64 * scale).floor() as i32;
        let y = (self.y as f64 * scale).floor() as i32;
        let r = ((self.x + self.width) as f64 * scale).ceil() as i32;
        let b = ((self.y + self.height) as f64 * scale).ceil() as i32;
        (x, y, r - x, b - y)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
impl Margins {
    pub const fn all(n: f32) -> Self {
        Self {
            top: n,
            right: n,
            bottom: n,
            left: n,
        }
    }
    pub fn inset(self, r: Rect) -> Rect {
        Rect::new(
            r.x + self.left,
            r.y + self.top,
            (r.width - self.left - self.right).max(0.),
            (r.height - self.top - self.bottom).max(0.),
        )
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Anchors {
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub center_x: Option<f32>,
    pub center_y: Option<f32>,
}
impl Anchors {
    pub fn resolve(self, parent: Rect, mut child: Rect) -> Rect {
        if let (Some(l), Some(r)) = (self.left, self.right) {
            child.width = (parent.width - l - r).max(0.);
        }
        if let (Some(t), Some(b)) = (self.top, self.bottom) {
            child.height = (parent.height - t - b).max(0.);
        }
        child.x = parent.x
            + if let Some(l) = self.left {
                l
            } else if let Some(r) = self.right {
                parent.width - r - child.width
            } else if let Some(c) = self.center_x {
                (parent.width - child.width) * 0.5 + c
            } else {
                child.x
            };
        child.y = parent.y
            + if let Some(t) = self.top {
                t
            } else if let Some(b) = self.bottom {
                parent.height - b - child.height
            } else if let Some(c) = self.center_y {
                (parent.height - child.height) * 0.5 + c
            } else {
                child.y
            };
        child
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Orientation {
    Row,
    Column,
}
/// Positions known sizes; does not measure text or mutate the scene implicitly.
pub fn linear_layout(
    sizes: &[(f32, f32)],
    origin: (f32, f32),
    spacing: f32,
    direction: Orientation,
) -> Vec<Rect> {
    let (mut x, mut y) = origin;
    sizes
        .iter()
        .map(|&(w, h)| {
            let r = Rect::new(x, y, w, h);
            match direction {
                Orientation::Row => x += w + spacing,
                Orientation::Column => y += h + spacing,
            };
            r
        })
        .collect()
}
#[derive(Clone, Debug, Default)]
pub struct Damage {
    full: bool,
    bounds: Option<Rect>,
    regions: Vec<Rect>,
}
impl Damage {
    pub fn full(&mut self) {
        self.full = true;
        self.bounds = None;
        self.regions.clear();
    }
    pub fn add(&mut self, r: Rect) {
        if r.width > 0. && r.height > 0. && !self.full {
            self.bounds = Some(self.bounds.map_or(r, |b| b.union(r)));
            let mut region=r;
            let mut index=0;
            while index<self.regions.len() {
                let other=self.regions[index];
                if region.x<=other.x+other.width && other.x<=region.x+region.width
                    && region.y<=other.y+other.height && other.y<=region.y+region.height {
                    region=region.union(self.regions.swap_remove(index));index=0;
                }else{index+=1;}
            }
            self.regions.push(region);
            if self.regions.len()>16 {self.regions.clear();self.regions.push(self.bounds.unwrap());}
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.full || self.bounds.is_some()
    }
    pub fn take_regions(&mut self,extent:Rect)->Vec<Rect> {
        let regions=if self.full {vec![extent]} else {std::mem::take(&mut self.regions)};
        self.full=false;self.bounds=None;
        regions
    }
    pub fn take(&mut self, extent: Rect) -> Option<Rect> {
        let b = if self.full { Some(extent) } else { self.bounds };
        *self = Self::default();
        b
    }
}
/// Qt 6 anchors.alignWhenCentered: qRound(size / 2), in logical pixels.
/// Do this separately for parent and child; rounding their difference changes
/// placement for fractional and odd dimensions. Negative halves round away from zero.
pub fn qt_center_half(size: f32) -> f32 {
    let half = size as f64 / 2.;
    if half >= 0. {
        (half + 0.5).floor() as f32
    } else {
        (half - 0.5).ceil() as f32
    }
}
pub fn centered_position(parent: f32, child: f32, align_when_centered: bool) -> f32 {
    if align_when_centered {
        qt_center_half(parent) - qt_center_half(child)
    } else {
        (parent - child) / 2.
    }
}
/// Qt centreIn with alignWhenCentered set explicitly; dimensions stay fractional.
pub fn qt_centered(parent: Rect, size: (f32, f32), align_when_centered: bool) -> Rect {
    Rect::new(
        parent.x + centered_position(parent.width, size.0, align_when_centered),
        parent.y + centered_position(parent.height, size.1, align_when_centered),
        size.0,
        size.1,
    )
}
impl Anchors {
    pub fn resolve_qt(self, parent: Rect, child: Rect, align_when_centered: bool) -> Rect {
        let mut resolved = self.resolve(parent, child);
        if self.left.is_none() && self.right.is_none() {
            if let Some(offset) = self.center_x {
                resolved.x = parent.x
                    + centered_position(parent.width, resolved.width, align_when_centered)
                    + offset;
            }
        }
        if self.top.is_none() && self.bottom.is_none() {
            if let Some(offset) = self.center_y {
                resolved.y = parent.y
                    + centered_position(parent.height, resolved.height, align_when_centered)
                    + offset;
            }
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damage_covers_fractional_edges() {
        assert_eq!(Rect::new(0.3, 0.3, 10., 10.).physical(1.25), (0, 0, 13, 13));
    }
    #[test]
    fn damage_idle_and_union() {
        let mut d = Damage::default();
        assert!(!d.is_dirty());
        d.add(Rect::new(5., 5., 2., 2.));
        d.add(Rect::new(0., 0., 1., 1.));
        assert_eq!(d.take(Rect::default()), Some(Rect::new(0., 0., 7., 7.)));
        assert!(!d.is_dirty());
    }
}

#[cfg(test)]
mod damage_region_tests {
    use super::*;
    #[test]
    fn independent_changes_stay_disjoint_and_overlapping_old_new_ink_merge(){
        let extent=Rect::new(0.,0.,1000.,60.);let a=Rect::new(10.,10.,30.,20.);let b=Rect::new(900.,10.,30.,20.);
        let mut damage=Damage::default();damage.add(a);damage.add(b);damage.add(Rect::new(15.,10.,30.,20.));
        let regions=damage.take_regions(extent);assert_eq!(regions.len(),2);assert!(!damage.is_dirty());
        assert_eq!(regions.iter().map(|r|r.width*r.height).sum::<f32>(),1300.);
        damage.full();damage.add(a);assert_eq!(damage.take_regions(extent),vec![extent]);
        damage.add(a);damage.add(b);assert_eq!(damage.take(extent),Some(a.union(b)));assert!(!damage.is_dirty());
    }
}
