//! Retained scene. Modify nodes through `Scene::edit` to preserve damage accounting.
use crate::{
    animation::Color,
    geometry::{Damage, Rect},
};
use skia_safe::{svg, Image, Matrix, Path};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u64);
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    LinearGradient {
        start: (f32, f32),
        end: (f32, f32),
        stops: Vec<(f32, Color)>,
    },
}
impl Default for Fill {
    fn default() -> Self {
        Self::Solid(Color::default())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rectangle {
    pub fill: Fill,
    /// TL, TR, BR, BL in logical pixels.
    pub radii: [f32; 4],
    pub border: Option<Border>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderType {
    NativeRendering,
    #[default]
    QtRendering,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub render_type: RenderType,
    pub text: String,
    pub families: Vec<String>,
    pub size: f32,
    pub weight: i32,
    pub color: Color,
    pub letter_spacing: f32,
    pub max_lines: Option<usize>,
    pub elide: bool,
    pub align: skia_safe::textlayout::TextAlign,
    pub hinting: skia_safe::FontHinting,
    pub subpixel: bool,
}
impl Default for Text {
    fn default() -> Self {
        Self {
            render_type: RenderType::QtRendering,
            text: String::new(),
            families: vec!["Noto Sans".into()],
            size: 14.,
            weight: 400,
            color: Color::rgba(1., 1., 1., 1.),
            letter_spacing: 0.,
            max_lines: Some(1),
            elide: true,
            align: skia_safe::textlayout::TextAlign::Left,
            hinting: skia_safe::FontHinting::Slight,
            subpixel: false,
        }
    }
}
#[derive(Clone)]
pub enum Content {
    Group,
    Canvas(std::rc::Rc<crate::canvas::CanvasBatch>),
    /// Immutable retained subtree. Replace its Rc to update; descendant edits are refused.
    Shared(std::rc::Rc<Node>),
    Picture(skia_safe::Picture),
    StrokedPath {
        path: Path,
        fill: Fill,
        stroke: StrokeStyle,
    },
    Rect(Rectangle),
    Text(Text),
    Image(Image),
    Svg(svg::Dom),
    Path {
        path: Path,
        fill: Fill,
        stroke: Option<Border>,
    },
}
#[derive(Clone, Debug)]
pub struct StrokeStyle {
    pub width: f32,
    pub color: Color,
    pub cap: skia_safe::paint::Cap,
    pub join: skia_safe::paint::Join,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    pub offset: (f32, f32),
    pub sigma: f32,
    pub color: Color,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub tint: Option<Color>,
    pub blur_sigma: f32,
    pub shadow: Option<Shadow>,
    /// Qt normalized blur/shadow pyramid, applied before optional Gaussian effects.
    pub multi_effect: Option<crate::effects::MultiEffect>,
}
impl Effects {
    fn outset(&self) -> f32 {
        self.multi_effect.as_ref().map_or(0., |e| e.padding())
            + self.blur_sigma.max(0.) * 3.
            + self.shadow.as_ref().map_or(0., |s| {
                s.sigma.max(0.) * 3. + s.offset.0.abs().max(s.offset.1.abs())
            })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpacityMode {
    #[default]
    PerNode,
    Group,
}
/// Backend of NativeRendering text leaves. QtGl requires EglWindow's registered
/// target; unsupported transforms/clips/layers retain Skia rendering.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum NativeTextMode {
    #[default]
    Skia,
    QtGl,
}
pub struct Node {
    id: NodeId,
    revision: u64,
    pub frame: Rect,
    pub transform: Matrix,
    pub opacity: f32,
    pub opacity_mode: OpacityMode,
    pub native_text_mode: NativeTextMode,
    /// Qt GL rectangle/image/effect materials. None inherits (base false).
    /// Shared content starts a new scope; its root must explicitly opt in.
    pub native_materials: Option<bool>,
    /// Preserve a promoted QSG transform-root coordinate basis without clipping.
    pub native_batch_root: bool,
    /// None inherits the renderer mode (QtQuick by default).
    pub rect_mode: Option<crate::qt_rect::RectMode>,
    /// QQuickRectangle defaults to radius > 0; override for explicit antialiasing.
    pub rect_antialias: Option<bool>,
    /// Original QML qreal rectangle size, before scene geometry narrows to f32.
    /// Its f32 projection must match frame size. Used only by the Qt mesh mode.
    pub qt_rect_size: Option<(f64, f64)>,
    /// Qt Image smooth/mipmap settings; no source-size downsampling is implicit.
    pub image_sampling: crate::image::ImageSampling,
    pub visible: bool,
    /// Clip the entire subtree to this node's per-corner rounded bounds.
    pub clip: Option<[f32; 4]>,
    pub effects: Effects,
    pub content: Content,
    pub interactive: bool,
    pub children: Vec<Node>,
}
impl Node {
    pub fn new(frame: Rect, content: Content) -> Self {
        Self {
            id: NodeId(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            revision: 1,
            frame,
            transform: Matrix::new_identity(),
            opacity: 1.,
            opacity_mode: OpacityMode::PerNode,
            native_text_mode: NativeTextMode::default(),
            native_materials: None,
            native_batch_root: false,
            rect_mode: None,
            rect_antialias: None,
            qt_rect_size: None,
            image_sampling: Default::default(),
            visible: true,
            clip: None,
            effects: Effects::default(),
            content,
            interactive: false,
            children: Vec::new(),
        }
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    fn touch_all(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        for child in &mut self.children {
            child.touch_all();
        }
    }
    fn touch_path(&mut self, id: NodeId, include_target: bool) -> bool {
        if self.id == id {
            if include_target {
                self.touch_all();
            }
            return true;
        }
        if self
            .children
            .iter_mut()
            .any(|c| c.touch_path(id, include_target))
        {
            self.revision = self.revision.wrapping_add(1);
            true
        } else {
            false
        }
    }
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn add(&mut self, child: Node) -> NodeId {
        let id = child.id;
        self.children.push(child);
        id
    }
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        if self.id == id {
            Some(self)
        } else {
            self.children
                .iter()
                .find_map(|n| n.find(id))
                .or_else(|| match &self.content {
                    Content::Shared(n) => n.find(id),
                    _ => None,
                })
        }
    }
    fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if self.id == id {
            Some(self)
        } else {
            self.children.iter_mut().find_map(|n| n.find_mut(id))
        }
    }
    pub(crate) fn local_matrix(&self) -> Matrix {
        let mut m = Matrix::translate((self.frame.x, self.frame.y));
        m.pre_concat(&self.transform);
        m
    }
    pub(crate) fn local_bounds(&self) -> Rect {
        Rect::new(0., 0., self.frame.width, self.frame.height)
    }
    pub(crate) fn effects_outset(&self) -> f32 {
        self.effects.outset()
    }
    pub(crate) fn paint_bounds(&self) -> Rect {
        let mut bounds = self.local_bounds();
        if matches!(self.content, Content::Rect(_))
            && self.rect_mode != Some(crate::qt_rect::RectMode::Skia)
        {
            bounds = bounds.outset(1.);
        }
        let (path, stroke) = match &self.content {
            Content::Path { path, stroke, .. } => (
                Some(path),
                stroke.as_ref().map(|s| {
                    let mut p = skia_safe::Paint::default();
                    p.set_style(skia_safe::paint::Style::Stroke)
                        .set_stroke_width(s.width);
                    p
                }),
            ),
            Content::StrokedPath { path, stroke, .. } => {
                let mut p = skia_safe::Paint::default();
                p.set_style(skia_safe::paint::Style::Stroke)
                    .set_stroke_width(stroke.width)
                    .set_stroke_cap(stroke.cap)
                    .set_stroke_join(stroke.join);
                (Some(path), Some(p))
            }
            Content::Picture(picture) => {
                let r = picture.cull_rect();
                bounds = bounds.union(Rect::new(r.x(), r.y(), r.width(), r.height()));
                (None, None)
            }
            _ => (None, None),
        };
        if let Some(path) = path {
            let r = path.bounds();
            bounds = bounds.union(Rect::new(r.x(), r.y(), r.width(), r.height()));
            if let Some(stroke) = stroke {
                let mut outline = Path::new();
                if skia_safe::path_utils::fill_path_with_paint(
                    path,
                    &stroke,
                    &mut outline,
                    None,
                    None,
                ) {
                    let r = outline.bounds();
                    bounds = bounds.union(Rect::new(r.x(), r.y(), r.width(), r.height()));
                }
            }
        }
        bounds
    }
    fn bounds(&self, parent: &Matrix) -> Rect {
        let mut m = *parent;
        m.pre_concat(&self.local_matrix());
        let mut local = self.paint_bounds();
        if let Content::Shared(n) = &self.content {
            local = local.union(n.bounds(&Matrix::new_identity()));
        }
        for c in &self.children {
            local = local.union(c.bounds(&Matrix::new_identity()));
        }
        let b = local.outset(self.effects.outset() + 1.);
        let (r, _) = m.map_rect(skia_safe::Rect::from_xywh(b.x, b.y, b.width, b.height));
        Rect::new(r.x(), r.y(), r.width(), r.height())
    }
    fn damage_bounds(&self, id: NodeId) -> Option<Rect> {
        let bounds = if self.id == id {
            self.bounds(&Matrix::new_identity())
        } else {
            let local = self
                .children
                .iter()
                .find_map(|c| c.damage_bounds(id))?
                .outset(self.effects.outset() + 1.);
            let (r, _) = self.local_matrix().map_rect(crate::render::skrect(local));
            Rect::new(r.x(), r.y(), r.width(), r.height())
        };
        Some(bounds)
    }
    fn hit(&self, x: f32, y: f32) -> Option<NodeId> {
        if !self.visible || self.opacity <= 0. {
            return None;
        }
        let inverse = self.local_matrix().invert()?;
        let p = inverse.map_point((x, y));
        if let Some(radii) = self.clip {
            let rr = crate::render::rounded_rect(self.local_bounds(), radii);
            if !rr.contains(skia_safe::Rect::from_xywh(p.x, p.y, 0.001, 0.001)) {
                return None;
            }
        }
        self.children
            .iter()
            .rev()
            .find_map(|c| c.hit(p.x, p.y))
            .or_else(|| match &self.content {
                Content::Shared(n) => n.hit(p.x, p.y),
                _ => None,
            })
            .or_else(|| {
                if self.interactive && self.local_bounds().contains(p.x, p.y) {
                    Some(self.id)
                } else {
                    None
                }
            })
    }
}
pub struct Scene {
    root: Node,
    damage: Damage,
    revision: u64,
}
impl Scene {
    pub fn new(root: Node) -> Self {
        let mut damage = Damage::default();
        damage.full();
        Self {
            root,
            damage,
            revision: 1,
        }
    }
    pub fn root(&self) -> &Node {
        &self.root
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn edit(&mut self, id: NodeId, edit: impl FnOnce(&mut Node)) -> bool {
        if self.root.find_mut(id).is_none() {
            return false;
        }
        // Include ancestor effect extents and all transformed descendants conservatively.
        let old = self.root.damage_bounds(id).unwrap();
        let node = self.root.find_mut(id).unwrap();
        let revision = node.revision;
        edit(node);
        // The editor may replace the whole value; retained identity belongs to the slot.
        node.id = id;
        node.revision = revision;
        self.root.touch_path(id, true);
        self.damage.add(old);
        self.damage.add(self.root.damage_bounds(id).unwrap());
        self.revision += 1;
        true
    }
    /// Position-only change keeps this node's offscreen effect texture reusable.
    pub fn set_position(&mut self, id: NodeId, x: f32, y: f32) -> bool {
        let Some(n) = self.root.find(id) else {
            return false;
        };
        if n.frame.x == x && n.frame.y == y {
            return false;
        }
        if self.root.find_mut(id).is_none() {
            return false;
        }
        let old = self.root.damage_bounds(id).unwrap();
        let node = self.root.find_mut(id).unwrap();
        node.frame.x = x;
        node.frame.y = y;
        self.root.touch_path(id, false);
        self.damage.add(old);
        self.damage.add(self.root.damage_bounds(id).unwrap());
        self.revision += 1;
        true
    }
    pub fn invalidate(&mut self) {
        self.damage.full();
    }
    pub fn is_dirty(&self) -> bool {
        self.damage.is_dirty()
    }
    pub fn take_damage_regions(&mut self, extent:Rect)->Vec<Rect> {self.damage.take_regions(extent)}
    pub fn take_damage(&mut self, extent: Rect) -> Option<Rect> {
        self.damage.take(extent)
    }
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.root.hit(x, y)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_order_and_transforms() {
        let mut root = Node::new(Rect::new(0., 0., 100., 100.), Content::Group);
        let mut child = Node::new(Rect::new(20., 30., 20., 20.), Content::Group);
        child.interactive = true;
        let id = root.add(child);
        let s = Scene::new(root);
        assert_eq!(s.hit_test(25., 35.), Some(id));
        assert_eq!(s.hit_test(5., 5.), None);
    }
}

#[cfg(test)]
mod damage_tests {
    use super::*;
    #[test]
    fn edits_damage_only_old_and_new_subtree() {
        let mut root = Node::new(Rect::new(0., 0., 1000., 1000.), Content::Group);
        let id = root.add(Node::new(Rect::new(10., 10., 20., 20.), Content::Group));
        let mut scene = Scene::new(root);
        let extent = Rect::new(0., 0., 1000., 1000.);
        scene.take_damage(extent);
        scene.edit(id, |n| n.frame.x = 40.);
        let d = scene.take_damage(extent).unwrap();
        assert!(d.x <= 10. && d.x + d.width >= 60.);
        assert!(d.width < 100. && d.height < 100.);
    }
}
