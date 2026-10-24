use crate::{layout, model::{Model, QUIRK_CORNER_MASK_FOLLOWS_STRIP}, view};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Fill, Node},
    skia,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Top,
    Overlay,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    None,
    OnDemand,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchors {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}
#[derive(Clone, Debug)]
pub struct Requirement {
    pub namespace: String,
    pub output: String,
    pub size: (u32, u32),
    pub layer: Layer,
    pub anchors: Anchors,
    pub exclusive_zone: i32,
    pub keyboard: Keyboard,
    pub input_regions: Vec<Rect>,
    pub input_passthrough: bool,
    pub mapped: bool,
}
pub fn bar(m: &Model) -> Requirement {
    let l = layout::compute(m, &m.theme);
    Requirement {
        namespace: "hyprv-quickshell".into(),
        output: m.output.name.clone(),
        size: (
            m.output.width.ceil() as u32,
            m.surface_height().ceil() as u32,
        ),
        layer: if m.reveal < 1. || m.fullscreen || m.fullscreen_hide != 0. {
            Layer::Overlay
        } else {
            Layer::Top
        },
        anchors: Anchors {
            top: true,
            bottom: false,
            left: true,
            right: true,
        },
        exclusive_zone: (25. + 33. * m.reveal).round() as i32,
        keyboard: Keyboard::None,
        input_regions: l.input_regions,
        input_passthrough: false,
        mapped: true,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
/// One full-output decorative overlay, matching shell.qml:3470-3546.
/// The vector stays compatible with the host's surface mounting API.
pub fn corners(m: &Model) -> Vec<(Requirement, Node)> {
    let mut root = Node::new(
        Rect::new(0., 0., m.output.width, m.output.height),
        Content::Group,
    );
    root.native_materials=Some(true);
    // BarScreenState.qml:22: top shades follow the compact/fullscreen reveal.
    let top = (15. * (1. - m.reveal) * (1. - m.fullscreen_hide)).clamp(0., 15.) as f32;
    for (corner, x, y) in [
        (Corner::TopLeft, 0., top),
        (Corner::TopRight, m.output.width - 27., top),
        (Corner::BottomLeft, 0., m.output.height - 27.),
        (
            Corner::BottomRight,
            m.output.width - 27.,
            m.output.height - 27.,
        ),
    ] {
        let mut child = corner_node_at_scale(corner, m.output.scale);
        child.frame.x = x;
        child.frame.y = y;
        root.add(child);
    }
    // Laptop strip (z 100, above the shades). Desktop never builds it.
    if m.laptop() {
        root.add(compact_strip(m).node);
    }
    vec![(
        Requirement {
            namespace: format!("hyprv-screen-corner-overlay-{}", m.output.name),
            output: m.output.name.clone(),
            // All-edge anchors request compositor-assigned full output dimensions.
            size: (0, 0),
            layer: Layer::Overlay,
            anchors: Anchors {
                top: true,
                bottom: true,
                left: true,
                right: true,
            },
            exclusive_zone: -1,
            keyboard: Keyboard::None,
            input_regions: corner_input(m),
            input_passthrough: !(m.laptop() && QUIRK_CORNER_MASK_FOLLOWS_STRIP),
            mapped: true,
        },
        root,
    )]
}
/// Laptop corner-window mask `{0, 0, width, ceil(topCornerOffset)}` (§2.1); desktop empty.
pub fn corner_input(m: &Model) -> Vec<Rect> {
    let h = m.top_corner_offset().ceil();
    if m.laptop() && QUIRK_CORNER_MASK_FOLLOWS_STRIP && h > 0. {
        vec![Rect::new(0., 0., m.output.width, h)]
    } else {
        vec![]
    }
}
pub fn corner_node(corner: Corner) -> Node {
    corner_node_at_scale(corner, 1.)
}
/// Production QSG-window Canvas sampling (the component capture uses a different
/// RenderControl atlas). One A8 atlas is shared by all four decorative corners.
pub fn corner_window(width:f32,height:f32,scale:f32,top:f32,bottom:f32)->Node {
    use std::{cell::RefCell,collections::BTreeMap};
    thread_local! {static PACKED:RefCell<BTreeMap<u32,skia::Image>>=Default::default();}
    let image=PACKED.with(|cache| {
        let mut cache=cache.borrow_mut();let key=(scale*1000.).round()as u32;
        if let Some(image)=cache.get(&key){return Some(image.clone());}
        let keys=["corner-top-left","corner-top-right","corner-bottom-left","corner-bottom-right"];
        let first=crate::baked::image(keys[0],27,scale,"000000ff")?;
        let side=first.width();
        let mut pixels=vec![0;side as usize*side as usize*4];
        let info=skia::ImageInfo::new((side,side),skia::ColorType::Alpha8,skia::AlphaType::Premul,None);
        for (i,key) in keys.into_iter().enumerate(){
            let image=crate::baked::image(key,27,scale,"000000ff")?;
            if !image.read_pixels(&info,&mut pixels[i*side as usize*side as usize..],side as usize,(0,0),skia::image::CachingHint::Allow){return None;}
        }
        let info=skia::ImageInfo::new((side,side*4),skia::ColorType::Alpha8,skia::AlphaType::Premul,None);
        let image=skia::images::raster_from_data(&info,skia::Data::new_copy(&pixels),side as usize)?;
        cache.insert(key,image.clone());Some(image)
    });
    let mut root=Node::new(Rect::new(0.,0.,width,height),Content::Group);
    root.native_materials=Some(true);
    let atlas=((width*scale).ceil().max(1.)as u32).next_power_of_two().max(512);
    let atlas_h=((height*scale).ceil().max(1.)as u32).next_power_of_two().max(512);
    for (index,(corner,x,y)) in [
        (Corner::TopLeft,0.,top),(Corner::TopRight,width-27.,top),
        (Corner::BottomLeft,0.,height-27.-bottom),(Corner::BottomRight,width-27.,height-27.-bottom)
    ].into_iter().enumerate(){
        let mut node=if let Some(image)=&image {
            let mut node=Node::new(Rect::new(0.,0.,27.,27.),Content::Image(image.clone()));
            node.image_sampling=hyprv_ui::image::ImageSampling::QtCanvasAtlas {size:(atlas,atlas_h),corner:index as u8};node
        }else{corner_node_at_scale(corner,scale)};
        node.frame.x=x;node.frame.y=y;root.add(node);
    }
    root
}
pub fn corner_node_at_scale(corner: Corner, scale: f32) -> Node {
    let key = match corner {
        Corner::TopLeft => "corner-top-left",
        Corner::TopRight => "corner-top-right",
        Corner::BottomLeft => "corner-bottom-left",
        Corner::BottomRight => "corner-bottom-right",
    };
    if let Some(image) = crate::baked::image(key, 27, scale, "000000ff") {
        return Node::new(Rect::new(0., 0., 27., 27.), Content::Image(image));
    }
    let mut path = skia::Path::new();
    path.move_to((0., 0.))
        .line_to((27., 0.))
        .arc_to(skia::Rect::from_xywh(0., 0., 54., 54.), 270., -90., false)
        .line_to((0., 0.))
        .close();
    let content = Content::Path {
        path,
        fill: Fill::Solid(Color::rgba(0., 0., 0., 1.)),
        stroke: None,
    };
    let mut n = Node::new(Rect::new(0., 0., 27., 27.), content);
    let mirror_x = matches!(corner, Corner::TopRight | Corner::BottomRight);
    let mirror_y = matches!(corner, Corner::BottomLeft | Corner::BottomRight);
    let mut matrix = skia::Matrix::translate((
        if mirror_x { 27. } else { 0. },
        if mirror_y { 27. } else { 0. },
    ));
    matrix.pre_scale(
        (
            if mirror_x { -1. } else { 1. },
            if mirror_y { -1. } else { 1. },
        ),
        None,
    );
    n.transform = matrix;
    n
}
pub(crate) fn popup(m: &Model, namespace: &str, mapped: bool) -> Requirement {
    Requirement {
        namespace: namespace.into(),
        output: m.output.name.clone(),
        size: (m.output.width.ceil() as u32, m.output.height.ceil() as u32),
        layer: Layer::Overlay,
        anchors: Anchors {
            top: true,
            bottom: true,
            left: true,
            right: true,
        },
        exclusive_zone: -1,
        keyboard: if mapped {
            Keyboard::OnDemand
        } else {
            Keyboard::None
        },
        input_regions: if mapped {
            vec![Rect::new(0., 0., m.output.width, m.output.height)]
        } else {
            vec![]
        },
        input_passthrough: false,
        mapped,
    }
}
pub fn corner_atlas() -> Node {
    let mut root = view::group(Rect::new(0., 0., 54., 54.));
    for (corner, x, y) in [
        (Corner::TopLeft, 0., 0.),
        (Corner::TopRight, 27., 0.),
        (Corner::BottomLeft, 0., 27.),
        (Corner::BottomRight, 27., 27.),
    ] {
        let mut wrapper = view::group(Rect::new(x, y, 27., 27.));
        wrapper.add(corner_node(corner));
        root.add(wrapper);
    }
    root
}

impl Requirement {
    /// Host must bind `output` to its wl_output and apply `input_regions` with
    /// UiContext::set_input_region after creating/updating this configuration.
    pub fn layer_config(&self) -> hyprv_ui::wayland::LayerConfig {
        use hyprv_ui::wayland::{Anchor, KeyboardInteractivity, Layer as EngineLayer, LayerConfig};
        let mut anchor = Anchor::empty();
        for (enabled, edge) in [
            (self.anchors.top, Anchor::TOP),
            (self.anchors.bottom, Anchor::BOTTOM),
            (self.anchors.left, Anchor::LEFT),
            (self.anchors.right, Anchor::RIGHT),
        ] {
            if enabled {
                anchor |= edge;
            }
        }
        LayerConfig {
            namespace: self.namespace.clone(),
            layer: match self.layer {
                Layer::Top => EngineLayer::Top,
                Layer::Overlay => EngineLayer::Overlay,
            },
            anchor,
            size: self.size,
            exclusive_zone: self.exclusive_zone,
            keyboard: match self.keyboard {
                Keyboard::None => KeyboardInteractivity::None,
                Keyboard::OnDemand => KeyboardInteractivity::OnDemand,
            },
            input_passthrough: self.input_passthrough,
            ..Default::default()
        }
    }
}

/// Laptop compact strip (laptop-bar.md §2, L:3578-3657), in corner-window coordinates.
/// Items are `(target, rect, node)`; rects are the strip MouseAreas for hit testing.
pub struct Strip {
    pub frame: Rect,
    pub items: Vec<(crate::model::Target, Rect)>,
    pub node: Node,
}
const STRIP_WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
fn strip_text(label: &str, x: f32, px: f32, weight: i32, color: Color) -> (Node, f32) {
    let (w, h) = layout::measure(label, px, weight);
    let mut node = view::text(label, Rect::new(x, (15. - h) / 2., w, h), px, weight, color, false);
    if crate::model::QUIRK_COMPACT_STRIP_DF_TEXT {
        if let Content::Text(t) = &mut node.content {
            t.render_type = hyprv_ui::scene::RenderType::QtRendering;
        }
        node.native_text_mode = Default::default();
    }
    (node, w)
}
/// `assets/bar/control-panel-dark.svg` rasterised at its 200 px intrinsic size times the
/// window DPR (QUIRK_STRIP_ICON_FROM_200PX_SVG), then scaled smoothly without mipmaps.
fn strip_icon(dpr: f32) -> Option<skia::Image> {
    use std::cell::RefCell;
    thread_local! {static ICON:RefCell<Option<(f32, Option<skia::Image>)>>=const{RefCell::new(None)};}
    ICON.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().map(|(d, _)| *d) != Some(dpr) {
            let size = if crate::model::QUIRK_STRIP_ICON_FROM_200PX_SVG { 200. } else { 11. };
            *c = Some((dpr, view::svg_image(include_bytes!("../assets/control-panel-dark.svg"), size, size, dpr)));
        }
        c.as_ref().and_then(|(_, i)| i.clone())
    })
}
pub fn compact_strip(m: &Model) -> Strip {
    use crate::{format, model::Target};
    let white = Color::rgba(1., 1., 1., 1.);
    let w = m.output.width;
    let frame = Rect::new(0., m.top_corner_offset() - 15., w, 15.);
    let mut node = Node::new(frame, Content::Group);
    node.add(hyprv_widgets::primitives::rounded_rect(
        Rect::new(0., 0., w, 15.),
        0.,
        Color::rgba(0., 0., 0., 1.),
        None,
    ));
    let mut items = Vec::new();
    let push = |node: &mut Node, items: &mut Vec<(Target, Rect)>, target, n: Node| {
        items.push((target, Rect::new(n.frame.x, frame.y, n.frame.width, 15.)));
        node.add(n);
    };
    // Left Row: x 10, spacing 12; workspace Row spacing 10.
    let mut x = 10.;
    let active = m.focused_workspace();
    let l = layout::compute(m, &m.theme);
    let workspaces: Vec<(i32, String)> = l
        .modules
        .iter()
        .filter_map(|n| match n.target {
            Target::Workspace(id) => Some((id, n.label.clone())),
            _ => None,
        })
        .collect();
    for (i, (id, label)) in workspaces.iter().enumerate() {
        if i > 0 {
            x += 10.;
        }
        let on = *id == active;
        let (n, tw) = strip_text(
            label,
            x,
            11.,
            if on { 700 } else { 400 },
            if on { white } else { hyprv_widgets::foundations::rgb("#858585") },
        );
        push(&mut node, &mut items, Target::Workspace(*id), n);
        x += tw;
    }
    x += 12.;
    let stats = format!(
        "\u{f2db} {}%  \u{f0c9} {}%",
        format::js_round(m.stats.cpu_percent.unwrap_or(0.)),
        format::js_round(m.stats.memory_percent.unwrap_or(0.))
    );
    let (n, _) = strip_text(&stats, x, 10., 400, white);
    push(&mut node, &mut items, Target::Cpu, n);
    // Centre: anchors.centerIn (alignWhenCentered).
    let clock = format!(
        "{}  {:02}:{:02}",
        STRIP_WEEKDAYS[m.clock.weekday.min(6) as usize],
        m.clock.hour,
        m.clock.minute
    );
    let (mut n, tw) = strip_text(&clock, 0., 11., 400, white);
    n.frame.x = view_center(w) - view_center(tw);
    n.frame.y = view_center(15.) - view_center(n.frame.height);
    push(&mut node, &mut items, Target::StripClock, n);
    // Right Row: anchors.right + rightMargin 10, spacing 12.
    let power = format::power_label(m);
    let connected = m.network.connected
        || (!crate::model::QUIRK_STRIP_WIFI_IGNORES_ETHERNET && m.network.wired_connected);
    let wifi = if connected {
        "\u{f1eb}"
    } else {
        "\u{f05aa}"
    };
    let bell = format!(
        "{}{}",
        if m.dnd() { "\u{f009b}" } else { "\u{f0a2}" },
        if m.notifications.alt.contains("notification") { "·" } else { "" }
    );
    let widths = [
        layout::measure(&power, 10., 400).0,
        layout::measure(wifi, 11., 400).0,
        13.,
        layout::measure(&bell, 11., 400).0,
    ];
    let mut x = w - 10. - widths.iter().sum::<f32>() - 12. * 3.;
    let (n, _) = strip_text(&power, x, 10., 400, white);
    push(&mut node, &mut items, Target::Battery, n);
    x += widths[0] + 12.;
    let (n, _) = strip_text(wifi, x, 11., 400, white);
    push(&mut node, &mut items, Target::Wifi, n);
    x += widths[1] + 12.;
    let mut icon = Node::new(Rect::new(x, 0., 13., 15.), Content::Group);
    if let Some(image) = strip_icon(m.output.scale) {
        let mut leaf = Node::new(Rect::new(1., 2., 11., 11.), Content::Image(image));
        leaf.image_sampling = hyprv_ui::image::ImageSampling::qt(true, false);
        icon.add(leaf);
    }
    push(&mut node, &mut items, Target::ControlPanel, icon);
    x += 13. + 12.;
    let (n, _) = strip_text(&bell, x, 11., 400, white);
    push(&mut node, &mut items, Target::Notifications, n);
    node.visible = m.compact_strip_visible();
    Strip { frame, items, node }
}
fn view_center(v: f32) -> f32 {
    hyprv_ui::geometry::qt_center_half(v)
}
