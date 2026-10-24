use crate::{
    format,
    layout::{self, Layout, Module},
    model::*,
};
use hyprv_ui::{
    animation::Color,
    geometry::{qt_center_half as hcenter, Rect},
    scene::{Border, Content, Node, NodeId, RenderType, Scene},
    skia,
};
use hyprv_widgets::{
    containers::group_pill,
    foundations::{centered, rgb, with_alpha, Palette},
    primitives::{rounded_rect, FontSpec, Label},
};
use std::{cell::RefCell, time::Duration};
pub(crate) fn group(frame: Rect) -> Node {
    Node::new(frame, Content::Group)
}
pub(crate) fn text(
    text: &str,
    frame: Rect,
    px: f32,
    weight: i32,
    color: Color,
    elide: bool,
) -> Node {
    let mut label = Label::new(
        text,
        FontSpec {
            pixel_size: px,
            weight,
            ..Default::default()
        },
        color,
    );
    label.elide = elide;
    let mut node=label.node(frame);
    node.native_text_mode=hyprv_ui::scene::NativeTextMode::QtGl;
    node
}
pub(crate) fn centered_text(label: &str, frame: Rect, px: f32, weight: i32, color: Color) -> Node {
    let (w, h) = layout::measure(label, px, weight);
    text(
        label,
        Rect::new(
            frame.x + hcenter(frame.width) - hcenter(w),
            frame.y + hcenter(frame.height) - hcenter(h),
            w,
            h,
        ),
        px,
        weight,
        color,
        false,
    )
}
/// Decode an in-memory dynamic SVG at the preferred physical output scale.
/// Fixed shell icons use `baked::image` instead. The host caches dynamic assets.
pub fn svg_image(bytes: &[u8], width: f32, height: f32, scale: f32) -> Option<skia::Image> {
    if ![width, height, scale]
        .iter()
        .all(|v| v.is_finite() && *v > 0.)
    {
        return None;
    }
    crate::svg::decode(
        bytes,
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    )
    .map(|d| d.image)
}
/// Compatibility picture helper at scale 1; prefer `svg_image` for native DPR.
pub fn svg_picture(bytes: &str, width: f32, height: f32) -> Option<skia::Picture> {
    let image = svg_image(bytes.as_bytes(), width, height, 1.)?;
    let mut recorder = skia::PictureRecorder::new();
    let canvas = recorder.begin_recording(skia::Rect::from_wh(width, height), false);
    canvas.draw_image_rect(
        &image,
        None,
        skia::Rect::from_wh(width, height),
        &skia::Paint::default(),
    );
    recorder.finish_recording_as_picture(None)
}
/// Image leaf; sampling is resolved once the bar's full layout is known.
pub fn image_node(frame: Rect, image: skia::Image) -> Node {
    let mut node = Node::new(frame, Content::Image(image));
    node.image_sampling = hyprv_ui::image::ImageSampling::Mipmap;
    node
}
thread_local! {static ICONS:RefCell<[Option<skia::Picture>;2]>=const{RefCell::new([None,None])};}
/// DynamicIsland's root has `z: expanded ? 40 : 1` (DynamicIsland.qml) while the calendar
/// Loader keeps z 0, so during the morph cross-fade the fading island paints over the
/// fading-in calendar although it is declared first (full-screen calendar-morph A2).
pub const QUIRK_ISLAND_ABOVE_CALENDAR: bool = true;

fn control_icon(dark: bool) -> skia::Picture {
    ICONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache[usize::from(dark)]
            .get_or_insert_with(|| {
                let bytes = if dark {
                    include_str!("../assets/control-panel-dark.svg")
                } else {
                    include_str!("../assets/control-panel-light.svg")
                };
                svg_picture(bytes, 16., 16.).expect("bundled SVG")
            })
            .clone()
    })
}
pub(crate) fn module_node(module: &Module, m: &Model, theme: &Theme) -> Node {
    let p = Palette::new(theme.dark);
    let f = module.frame;
    let mut node = group(f);
    node.interactive = module.interactive;
    let hovered = m.hovered.as_ref() == Some(&module.target);
    if module.highlighted || (module.hoverable && hovered) {
        let color = if module.highlighted {
            module.highlight
        } else if module.target == Target::Wifi {
            with_alpha(p.active_workspace, if theme.dark { 0.18 } else { 0.22 })
        } else {
            p.workspace_hover
        };
        node.add(rounded_rect(
            Rect::new(0., 0., f.width, f.height),
            if module.target == Target::Wifi {
                10.
            } else {
                19.
            },
            color,
            None,
        ));
    }
    let color = if module.highlighted {
        module.highlighted_text
    } else {
        module.color
    };
    match &module.target {
        Target::ControlPanel => {
            let rect = centered(Rect::new(0., 0., f.width, f.height), (16., 16.));
            let key = if theme.dark { "control-panel-dark" } else { "control-panel-light" };
            if let Some(image) = m.assets.images.get(key).cloned().or_else(||
                crate::baked::image(key, 16, m.output.scale, "source")
            ) {
                node.add(image_node(rect, image));
            } else {
                node.add(Node::new(rect, Content::Picture(control_icon(theme.dark))));
            }
        }
        Target::Wifi => {
            let key = format::wifi_asset_key(m, theme.dark);
            let size = if theme.dark { 16. } else { 24. };
            let mut rect = centered(Rect::new(0., 0., f.width, f.height), (size, size));
            if theme.dark {
                rect.y -= 2.;
            }
            if let Some(image) = m.assets.images.get(&key).cloned().or_else(|| {
                crate::baked::image(
                    &key,
                    if theme.dark { 16 } else { 24 },
                    m.output.scale,
                    "source",
                )
            }) {
                node.add(image_node(rect, image.clone()));
            } else {
                let mut r = Rect::new(0., 0., f.width, f.height);
                if theme.dark {
                    r.y -= 2.;
                }
                node.add(centered_text(&module.label, r, 24., 700, color));
            }
        }
        Target::Tray(key) => {
            if let Some(image) = m.assets.images.get(key) {
                node.add(image_node(Rect::new(0., 10., 18., 18.), image.clone()));
            } else {
                let mut fallback = centered_text(
                    &module.label,
                    Rect::new(0., 0., f.width, f.height),
                    10.,
                    400,
                    color,
                );
                if let Content::Text(text) = &mut fallback.content {
                    text.render_type = RenderType::QtRendering;
                }
                node.add(fallback);
            }
        }
        Target::Overflow => {
            node.add(centered_text(
                &module.label,
                Rect::new(0., 0., f.width, f.height),
                18.,
                700,
                color,
            ));
        }
        Target::Notifications => {
            node.add(centered_text(
                &module.label,
                Rect::new(0., 0., f.width, f.height),
                14.,
                700,
                color,
            ));
            if m.notifications.alt.contains("notification") {
                let (w, h) = layout::measure("\u{f444}", 10., 700);
                node.add(text(
                    "\u{f444}",
                    Rect::new(f.width - 2. - w, 4., w, h),
                    10.,
                    700,
                    rgb("#ff0000"),
                    false,
                ));
            }
        }
        _ => {
            let (_, h) = layout::measure(&module.label, module.px, 700);
            let available = f.width - module.padding.0 - module.padding.1;
            let mut label = text(
                &module.label,
                Rect::new(
                    module.padding.0,
                    hcenter(f.height) - hcenter(h),
                    available,
                    h,
                ),
                module.px,
                700,
                color,
                false,
            );
            if let Content::Text(t) = &mut label.content {
                t.align = skia::textlayout::TextAlign::Center;
            }
            node.add(label);
        }
    }
    node
}
fn target_node(l: &Layout, m: &Model, theme: &Theme, target: &Target, module: &mut impl FnMut(&Module, &Model, &Theme) -> Node) -> Node {
    l.modules
        .iter()
        .find(|n| &n.target == target)
        .map(|n| module(n, m, theme))
        .unwrap_or_else(|| {
            let mut n = group(Rect::default());
            n.visible = false;
            n
        })
}
pub fn build(m: &Model, theme: &Theme) -> Node {
    let layout=layout::compute(m,theme);
    build_with(m,theme,&layout,right_batch_eligible(&layout),&mut module_node)
}
fn build_with(m: &Model, theme: &Theme, l: &Layout, right_batch_root: bool, module: &mut impl FnMut(&Module, &Model, &Theme) -> Node) -> Node {
    let p = Palette::new(theme.dark);
    let full = Rect::new(0., 0., m.output.width, m.surface_height());
    let mut root = group(full);
    root.native_materials=Some(true);
    let mut content = group(Rect::new(0., l.content_y, full.width, full.height));
    content.visible = m.reveal > 0.;
    let mut chrome = group(full);
    for rect in &l.pills {
        chrome.add(group_pill(*rect, p.module_background, vec![]));
    }
    content.add(chrome);
    let mut left = group(full);
    left.add(target_node(l, m, theme, &Target::Launcher, module));
    left.add(target_node(l, m, theme, &Target::Viewflow, module));
    let mut workspaces = group(full);
    for item in l
        .modules
        .iter()
        .filter(|n| matches!(n.target, Target::Workspace(_)))
    {
        workspaces.add(module(item, m, theme));
    }
    left.add(workspaces);
    let mut performance = group(full);
    for target in [Target::Cpu, Target::Memory, Target::Network] {
        performance.add(target_node(l, m, theme, &target, module));
    }
    left.add(performance);
    content.add(left);
    let mut title = rounded_rect(l.title, 19., p.module_background, None);
    title.visible = l.title.width > 0.;
    let height = layout::measure(m.title(), 16., 700).1;
    title.add(text(
        m.title(),
        Rect::new(
            12.,
            hcenter(38.) - hcenter(height),
            (l.title.width - 24.).max(0.),
            height,
        ),
        16.,
        700,
        p.primary_text,
        true,
    ));
    content.add(title);
    let mut right = group(full);
    for target in [
        Target::Temperature,
        Target::CpuPower,
        Target::Battery,
        Target::Previous,
        Target::PlayPause,
        Target::Next,
        Target::Volume,
        Target::Wifi,
    ] {
        right.add(target_node(l, m, theme, &target, module));
    }
    let mut tray = group(full);
    for item in l
        .modules
        .iter()
        .filter(|n| matches!(n.target, Target::Tray(_)))
    {
        tray.add(module(item, m, theme));
    }
    tray.add(target_node(l, m, theme, &Target::Overflow, module));
    right.add(tray);
    right.add(target_node(l, m, theme, &Target::ControlPanel, module));
    right.add(target_node(l, m, theme, &Target::Notifications, module));
    // QSG promotes the transformed right Row when its drawable subtree exceeds
    // the default 64-node threshold. Keep that local vertex basis; it changes
    // smooth-color AA rounding even though world geometry is identical.
    if right_batch_root {
        let origin = l.pills[2];
        let promote = |node: &mut Node| {
            node.frame.x = origin.x;
            node.frame.y = origin.y;
            node.native_batch_root = true;
            for child in &mut node.children {
                child.frame.x -= origin.x;
                child.frame.y -= origin.y;
            }
        };
        let mut chrome = group(full);
        chrome.children = content.children[0].children.split_off(2);
        promote(&mut chrome);
        content.children[0].add(chrome);
        promote(&mut right);
    }
    content.add(right);
    let mut dismiss = group(full);
    dismiss.visible = m.calendar_open;
    dismiss.interactive = m.calendar_open;
    content.add(dismiss);
    let mut center = rounded_rect(
        l.center,
        if m.calendar_presented {
            m.calendar_radius
        } else {
            24.
        },
        if m.calendar_progress > 0.001 {
            p.module_background
        } else {
            Color::default()
        },
        if m.calendar_progress > 0.001 {
            Some(Border {
                width: 1.,
                color: with_alpha(p.primary_text, if theme.dark { 0.11 } else { 0.09 }),
            })
        } else {
            None
        },
    );
    center.clip = Some([0.; 4]);
    let mut island = group(Rect::new(
        hcenter(l.center.width) - hcenter(m.island.width),
        0.,
        m.island.width,
        m.island.height,
    ));
    island.opacity = (1. - m.calendar_progress) as f32;
    island.visible = island.opacity > 0.01;
    if let Some(build) = &m.slots.island {
        island.add(build());
    }
    let cw = (m.output.width - 48.).clamp(0., 520.);
    let ch = (m.surface_height() - 78.).clamp(0., 500.);
    let mut calendar = group(Rect::new(hcenter(l.center.width) - hcenter(cw), 0., cw, ch));
    calendar.opacity = m.calendar_progress as f32;
    calendar.visible = m.calendar_presented;
    if let Some(build) = &m.slots.calendar {
        if m.calendar_presented {
            calendar.add(build());
        }
    }
    if QUIRK_ISLAND_ABOVE_CALENDAR {
        center.add(calendar);
        center.add(island);
    } else {
        center.add(island);
        center.add(calendar);
    }
    content.add(center);
    root.add(content);
    root
}
fn right_batch_eligible(l: &Layout) -> bool {
    // TrayButton.qml owns both IconImage and fallback Text scene-graph nodes;
    // visibility suppresses drawing, not their allocated subtree node count.
    let modules=l.modules.iter().filter(|m| matches!(m.target,
        Target::Temperature | Target::CpuPower | Target::Battery | Target::Previous | Target::PlayPause |
        Target::Next | Target::Volume | Target::Wifi | Target::Overflow |
        Target::ControlPanel | Target::Notifications)).count();
    l.tray.visible * 2 + modules + l.pills.len().saturating_sub(2) > 64
}
fn content_equal(a: &Content, b: &Content) -> bool {
    match (a, b) {
        (Content::Group, Content::Group) => true,
        (Content::Shared(a), Content::Shared(b)) => std::rc::Rc::ptr_eq(a, b),
        (
            Content::StrokedPath {
                path: a,
                fill: af,
                stroke: as_,
            },
            Content::StrokedPath {
                path: b,
                fill: bf,
                stroke: bs,
            },
        ) => {
            a == b
                && af == bf
                && as_.width == bs.width
                && as_.color == bs.color
                && as_.cap == bs.cap
                && as_.join == bs.join
        }
        (Content::Rect(a), Content::Rect(b)) => a == b,
        (Content::Text(a), Content::Text(b)) => a == b,
        (Content::Image(a), Content::Image(b)) => a.unique_id() == b.unique_id(),
        (Content::Picture(a), Content::Picture(b)) => a.unique_id() == b.unique_id(),
        (
            Content::Path {
                path: a,
                fill: af,
                stroke: as_,
            },
            Content::Path {
                path: b,
                fill: bf,
                stroke: bs,
            },
        ) => a == b && af == bf && as_ == bs,
        _ => false,
    }
}
fn effects_equal(a: &hyprv_ui::scene::Effects, b: &hyprv_ui::scene::Effects) -> bool {
    a.tint == b.tint
        && a.blur_sigma == b.blur_sigma
        && a.multi_effect == b.multi_effect
        && match (&a.shadow, &b.shadow) {
            (None, None) => true,
            (Some(a), Some(b)) => a.offset == b.offset && a.sigma == b.sigma && a.color == b.color,
            _ => false,
        }
}
/// Reconcile by stable component slots. No edit/damage on an equal tree; only
/// changed node properties/children are edited, preserving renderer text caches.
pub fn reconcile(scene: &mut Scene, old_id: NodeId, new: Node) -> usize {
    reconcile_skipping(scene, old_id, new, &Default::default())
}
fn reconcile_skipping(scene: &mut Scene, old_id: NodeId, mut new: Node, retained: &std::collections::HashMap<NodeId, NodeId>) -> usize {
    if retained.get(&new.id()) == Some(&old_id) { return 0; }
    let Some(old) = scene.root().find(old_id) else {
        return 0;
    };
    let old_children: Vec<_> = old.children.iter().map(Node::id).collect();
    let child_count = old_children.len();
    let geometry = old.frame != new.frame
        || old.opacity != new.opacity
        || old.opacity_mode != new.opacity_mode
        || old.rect_mode != new.rect_mode
        || old.native_text_mode != new.native_text_mode
        || old.native_materials != new.native_materials
        || old.native_batch_root != new.native_batch_root
        || old.rect_antialias != new.rect_antialias
        || old.image_sampling != new.image_sampling
        || old.visible != new.visible
        || old.clip != new.clip
        || old.interactive != new.interactive
        || old.transform != new.transform
        || !effects_equal(&old.effects, &new.effects)
        || !content_equal(&old.content, &new.content);
    let mut edits = 0;
    if geometry {
        scene.edit(old_id, |node| {
            node.frame = new.frame;
            node.opacity = new.opacity;
            node.opacity_mode = new.opacity_mode;
            node.rect_mode = new.rect_mode;
            node.native_text_mode = new.native_text_mode;
            node.native_materials = new.native_materials;
            node.native_batch_root = new.native_batch_root;
            node.rect_antialias = new.rect_antialias;
            node.image_sampling = new.image_sampling;
            node.visible = new.visible;
            node.clip = new.clip;
            node.interactive = new.interactive;
            node.transform = new.transform;
            node.effects = new.effects.clone();
            node.content = new.content.clone();
        });
        edits += 1;
    }
    let paired = child_count.min(new.children.len());
    let mut children = std::mem::take(&mut new.children).into_iter();
    for id in old_children.into_iter().take(paired) {
        edits += reconcile_skipping(scene, id, children.next().unwrap(), retained);
    }
    let added: Vec<_> = children.collect();
    if child_count != paired || !added.is_empty() {
        scene.edit(old_id, |node| {
            node.children.truncate(paired);
            node.children.extend(added);
        });
        edits += 1;
    }
    edits
}
#[derive(Clone, PartialEq)]
struct ModuleSignature {
    module: Module,
    dark: bool,
    scale: f32,
    hovered: bool,
    image_key: Option<String>,
    image: Option<u32>,
    notification: bool,
}
impl ModuleSignature {
    fn new(module: &Module, m: &Model, theme: &Theme) -> Self {
        let key = match &module.target {
            Target::ControlPanel => Some(if theme.dark { "control-panel-dark".into() } else { "control-panel-light".into() }),
            Target::Wifi => Some(format::wifi_asset_key(m, theme.dark)),
            Target::Tray(key) => Some(key.clone()),
            _ => None,
        };
        Self {
            module: module.clone(), dark: theme.dark, scale: m.output.scale,
            hovered: m.hovered.as_ref() == Some(&module.target),
            image: key.as_ref().and_then(|key| m.assets.images.get(key).map(skia::Image::unique_id)),
            image_key: key,
            notification: module.target == Target::Notifications && m.notifications.alt.contains("notification"),
        }
    }
}
/// Per-mounted-bar rendering state. Unchanged modules are neither constructed
/// nor visited by reconciliation. The small scaffold and hosted slots still
/// update independently (including their animation frames).
#[derive(Default)]
pub struct RetainedView {
    right_batch_root: bool,
    modules: std::collections::BTreeMap<Target, (ModuleSignature, NodeId)>,
    order: Vec<Target>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub built_modules: usize,
    pub retained_modules: usize,
    pub edited_nodes: usize,
}
impl RetainedView {
    /// `normalize` applies host clip/position canonicalization before comparing.
    /// Keep one instance per Scene; replacing the Scene safely rebuilds modules.
    pub fn update(&mut self, scene: &mut Scene, m: &Model, normalize: impl FnOnce(&mut Node)) -> UpdateStats {
        let layout = layout::compute(m, &m.theme);
        let order: Vec<_> = layout.modules.iter().map(|module| module.target.clone()).collect();
        if order != self.order { self.modules.clear(); }
        self.order = order;
        let mut retained = std::collections::HashMap::new();
        let mut pending = std::collections::HashMap::new();
        let mut stats = UpdateStats::default();
        self.right_batch_root |= right_batch_eligible(&layout);
        let mut tree = build_with(m, &m.theme, &layout, self.right_batch_root, &mut |module, model, theme| {
            let signature = ModuleSignature::new(module, model, theme);
            let node = if let Some((_, id)) = self.modules.get(&module.target)
                .filter(|(old, id)| *old == signature && scene.root().find(*id).is_some()) {
                let node = group(module.frame);
                retained.insert(node.id(), *id);
                stats.retained_modules += 1;
                node
            } else {
                stats.built_modules += 1;
                module_node(module, model, theme)
            };
            pending.insert(node.id(), (module.target.clone(), signature));
            node
        });
        normalize(&mut tree);
        // Pair structural slots before reconciliation; newly inserted nodes keep
        // their incoming ID, existing nodes keep their retained ID.
        let mut modules = std::collections::BTreeMap::new();
        fn collect(new: &Node, old: Option<&Node>, pending: &std::collections::HashMap<NodeId, (Target, ModuleSignature)>, out: &mut std::collections::BTreeMap<Target, (ModuleSignature, NodeId)>) {
            if let Some((target, signature)) = pending.get(&new.id()) {
                out.insert(target.clone(), (signature.clone(), old.map_or(new.id(), Node::id)));
                return;
            }
            for (index, child) in new.children.iter().enumerate() {
                collect(child, old.and_then(|old| old.children.get(index)), pending, out);
            }
        }
        collect(&tree, Some(scene.root()), &pending, &mut modules);
        stats.edited_nodes = reconcile_skipping(scene, scene.root().id(), tree, &retained);
        self.modules = modules;
        stats
    }
}

pub fn update(scene: &mut Scene, m: &Model) -> usize {
    let id = scene.root().id();
    reconcile(scene, id, build(m, &m.theme))
}
/// Helper for host-supplied SNI ARGB pixmaps; byte conversion is pure and bounded.
pub fn tray_pixmap(item: &TrayItem, scale: f32) -> Option<skia::Image> {
    let target = (18. * scale).ceil() as u32;
    let pixmap = item
        .icon_pixmaps
        .iter()
        .filter(|p| p.width > 0 && p.height > 0 && p.width <= 4096 && p.height <= 4096)
        .min_by_key(|p| (p.width.abs_diff(target), p.height.abs_diff(target)))?;
    let expected = pixmap.width as usize * pixmap.height as usize * 4;
    if pixmap.argb.len() != expected {
        return None;
    }
    let mut rgba = Vec::with_capacity(expected);
    for p in pixmap.argb.chunks_exact(4) {
        rgba.extend_from_slice(&[p[1], p[2], p[3], p[0]]);
    }
    let info = skia::ImageInfo::new(
        (pixmap.width as i32, pixmap.height as i32),
        skia::ColorType::RGBA8888,
        skia::AlphaType::Unpremul,
        None,
    );
    skia::images::raster_from_data(
        &info,
        skia::Data::new_copy(&rgba),
        pixmap.width as usize * 4,
    )
}
pub fn snapshot(m: &Model) -> Scene {
    Scene::new(build(m, &m.theme))
}
pub fn at_time(mut m: Model, now: Duration) -> Model {
    m.now = now;
    m
}

/// Adapter for hyprv-shell's Slot::Bar: the shell host owns content motion,
/// calendar/island composition and its dismiss layer. Layout still uses the
/// supplied island/calendar geometry, but this tree draws only the bar strip.
pub fn build_strip(m: &Model, theme: &Theme) -> Node {
    let mut local = m.clone();
    local.fullscreen_hide = 0.;
    local.reveal = 1.;
    local.slots = Slots::default();
    let mut node = build(&local, theme);
    node.frame.height = 58.;
    if let Some(content) = node.children.first_mut() {
        content.frame.height = 58.;
        content
            .children
            .truncate(content.children.len().saturating_sub(2));
    }
    node
}
pub fn update_strip(scene: &mut Scene, m: &Model) -> usize {
    let id = scene.root().id();
    reconcile(scene, id, build_strip(m, &m.theme))
}
