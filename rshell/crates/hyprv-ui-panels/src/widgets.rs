//! Panel controls, each a pure builder of QML pixels from props + interaction state:
//! WifiActionChip (ui-panels.md §2.4), WifiSecurityBadge (§2.5), ControlPanelToggle,
//! ControlPanelSplitTile, ControlPanelSlider, ControlPanelMediaCard, the Bluetooth power
//! switch (ui-panels-control.md §2–3), WifiIconWithFallback (ui-panels-wifi.md §5),
//! SystemTrendChart (ui-panels-stats.md §3). Images and their MultiEffects (toggle icon
//! colorization, album-art mask) are engine nodes (`Content::Image`, `MultiEffect`).
use crate::{
    qtext::{self, Align, Font, TextBox, BOLD, MEDIUM, NORMAL},
    ui::*,
};
use hyprv_ui::{
    animation::Color,
    canvas::{CanvasBatch, CanvasPaint, CanvasPath, VerticalGradient},
    effects::{Mask, MaskSource, MultiEffect},
    geometry::Rect,
    image::ImageSampling,
    scene::{Content, Node, StrokeStyle},
    skia::{self as sk, svg, Image},
};

fn tb(s: &str, px: f32, weight: i32, color: Color) -> TextBox {
    TextBox::new(s, Font::native(px, weight), color)
}

// ------------------------------------------------------------------ action chip

#[derive(Clone, Debug)]
pub struct Chip {
    pub label: String,
    pub icon: String,
    pub icon_px: f32,
    pub min_width: f32,
    pub radius: f32,
    pub fill: Color,
    pub fg: Color,
    pub stroke: Color,
    pub disabled: bool,
}

impl Chip {
    /// WifiActionChip defaults (`WifiActionChip.qml:6-15`).
    pub fn new(pal: &Palette, label: &str) -> Self {
        Self {
            label: label.into(),
            icon: String::new(),
            icon_px: 16.,
            min_width: 0.,
            radius: 10.,
            fill: with_alpha(pal.primary_text, 0.08),
            fg: pal.primary_text,
            stroke: with_alpha(pal.primary_text, 0.10),
            disabled: false,
        }
    }
    pub fn icon(pal: &Palette, glyph: &str) -> Self {
        let mut c = Self::new(pal, "");
        c.icon = glyph.into();
        c
    }
    pub fn min(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn r(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn colors(mut self, fill: Color, fg: Color, stroke: Color) -> Self {
        self.fill = fill;
        self.fg = fg;
        self.stroke = stroke;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// `implicitWidth = max(minimumWidth, max(label.contentWidth, icon.contentWidth) + 24)`
    pub fn width(&self) -> f32 {
        let lw = if self.label.is_empty() { 0. } else { qtext::measure(&self.label, &Font::native(13., BOLD)).width };
        let iw = if self.icon.is_empty() { 0. } else { qtext::measure(&self.icon, &Font::native(self.icon_px, BOLD)).width };
        self.min_width.max(lw.max(iw) + 24.)
    }
}

pub const CHIP_HEIGHT: f32 = 34.;

/// Build a chip at (x, y) in a parent at `pabs`. Returns the node and its width.
pub fn chip<Z: Clone + PartialEq>(c: &mut Ctx<Z>, x: f32, y: f32, pabs: (f32, f32), chip: &Chip, id: impl Into<Option<Z>>, inter: &Interaction<Z>) -> (Node, f32) {
    let id: Option<Z> = id.into();
    let w = chip.width();
    let frame = Rect::new(x, y, w, CHIP_HEIGHT);
    let hovered = !chip.disabled && id.as_ref().is_some_and(|i| inter.is_hovered(i));
    let pressed = !chip.disabled && id.as_ref().is_some_and(|i| inter.is_pressed(i));
    let fill = if chip.disabled {
        with_alpha(c.pal.primary_text, 0.06)
    } else if pressed {
        darker(chip.fill, 1.08)
    } else if hovered {
        lighter(chip.fill, 1.06)
    } else {
        chip.fill
    };
    let border = if chip.disabled {
        with_alpha(c.pal.primary_text, 0.06)
    } else if hovered {
        with_alpha(chip.fg, 0.22)
    } else {
        chip.stroke
    };
    let mut n = rect(frame, chip.radius, fill, Some((1., border)));
    let me = at(pabs, frame);
    let t = if chip.icon.is_empty() { tb(&chip.label, 13., BOLD, chip.fg) } else { tb(&chip.icon, chip.icon_px, BOLD, chip.fg) };
    n.children.push(text_center(&t, w, CHIP_HEIGHT, me, c.dpr));
    if chip.disabled {
        n.opacity = 0.56;
    }
    if let Some(id) = id {
        c.zone_if(!chip.disabled, id, abs(pabs, frame), Cursor::Pointer);
    }
    (n, w)
}

// ------------------------------------------------------------------ security badge

/// WifiSecurityBadge: 22×20, radius `r`, fill primaryText α.08 D / .06 L, 1 px α.08,
/// lock U+F023 11 px Bold, primaryText α.82.
pub fn badge(pal: &Palette, frame: Rect, pabs: (f32, f32), radius: f32, dpr: f32) -> Node {
    let mut n = rect(frame, radius, with_alpha(pal.primary_text, if pal.dark { 0.08 } else { 0.06 }), Some((1., with_alpha(pal.primary_text, 0.08))));
    n.children.push(text_center(&tb("\u{F023}", 11., BOLD, with_alpha(pal.primary_text, 0.82)), frame.width, frame.height, at(pabs, frame), dpr));
    n
}

// ------------------------------------------------------------------ toggle tile

#[derive(Clone, Debug)]
pub struct Toggle {
    pub icon: String,
    /// `iconSource` key in [`crate::model::Assets::svgs`].
    pub svg: Option<String>,
    pub icon_only: bool,
    pub label: String,
    pub active: bool,
    pub destructive: bool,
    pub enabled: bool,
}
impl Toggle {
    pub fn icon(glyph: &str, active: bool) -> Self {
        Self { icon: glyph.into(), svg: None, icon_only: true, label: String::new(), active, destructive: false, enabled: true }
    }
}

pub fn toggle<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), t: &Toggle, assets: &crate::model::Assets, id: Z, inter: &Interaction<Z>) -> Node {
    let pal = c.pal;
    let (w, h) = (frame.width, frame.height);
    let active_color = if t.destructive { pal.critical } else { pal.accent };
    let active_fill = with_alpha(active_color, if pal.dark { 0.28 } else { 0.22 });
    let active_stroke = with_alpha(active_color, if pal.dark { 0.42 } else { 0.28 });
    let hovered = t.enabled && inter.is_hovered(&id);
    let pressed = t.enabled && inter.is_pressed(&id);
    let base = if t.active { active_fill } else { pal.card_fill };
    let fill = if !t.enabled {
        with_alpha(base, 0.48)
    } else if pressed {
        darker(base, 1.06)
    } else if hovered {
        lighter(base, 1.04)
    } else {
        base
    };
    let border = if !t.enabled {
        pal.card_stroke
    } else if hovered {
        if t.active { lighter(active_stroke, 1.08) } else { lighter(pal.card_stroke, 1.04) }
    } else if t.active {
        active_stroke
    } else {
        pal.card_stroke
    };
    let mut n = rect(frame, 19., fill, Some((1., border)));
    let me = at(pabs, frame);
    let box_size = if t.icon_only { (w.min(h) - 12.).clamp(22., 30.) } else { (h - 16.).clamp(28., 34.) };
    let icon_radius = (box_size * 0.34).round();
    let icon_px = if t.icon_only { (box_size * 0.5).clamp(15., 18.) } else { (box_size * 0.46).clamp(15., 18.) };
    let image_size = if t.icon_only { (box_size * 0.74).clamp(18., 22.) } else { (box_size * 0.64).clamp(18., 20.) };
    let icon_color = if t.active { rgb("#ffffff") } else { pal.primary_text };
    let label_visible = !t.icon_only && !t.label.is_empty();
    let label = tb(&t.label, 10., BOLD, icon_color).width((w - 14.).max(40.)).wrap().align(Align::Center);
    let label_h = if label_visible { text_size(&label).1 } else { 0. };
    let col_h = box_size + if label_visible { 6. + label_h } else { 0. };
    let col_w = if label_visible { box_size.max((w - 14.).max(40.)) } else { box_size };
    let col_x = center(w, col_w);
    let col_y = center(h, col_h);
    let box_frame = Rect::new(col_x + center(col_w, box_size), col_y, box_size, box_size);
    let well = if t.icon_only {
        Color::default()
    } else if pressed {
        darker(pal.icon_well_fill(t.active), 1.05)
    } else if hovered {
        lighter(pal.icon_well_fill(t.active), 1.03)
    } else {
        pal.icon_well_fill(t.active)
    };
    let mut bx = rect(box_frame, icon_radius, well, None);
    let babs = at(me, box_frame);
    if let Some(dom) = t.svg.as_ref().and_then(|k| assets.svgs.get(k)) {
        let f = Rect::new(center(box_size, image_size), center(box_size, image_size), image_size, image_size);
        let src_px = (image_size * 2.).round().max(32.);
        let key = t.svg.as_deref().unwrap_or_default();
        // Image { sourceSize: src_px; smooth; mipmap; layer.effect: MultiEffect {
        // colorization: 1; colorizationColor: iconColor } }
        if let Some(raster) = svg_raster(key, dom, src_px, c.dpr) {
            let mut n = group(f);
            n.effects.multi_effect = Some(MultiEffect { colorization: 1., colorization_color: icon_color, ..Default::default() });
            n.children.push(image_node(Rect::new(0., 0., image_size, image_size), raster, ImageSampling::qt(true, true)));
            bx.children.push(n);
        }
    } else {
        bx.children.push(text_center(&tb(&t.icon, icon_px, BOLD, icon_color), box_size, box_size, babs, c.dpr));
    }
    n.children.push(bx);
    if label_visible {
        n.children.push(text(&label, Rect::new(col_x, col_y + box_size + 6., col_w, label_h), me, c.dpr));
    }
    if !t.enabled {
        n.opacity = 0.56;
    }
    c.zone_if(t.enabled, id, abs(pabs, frame), Cursor::Pointer);
    n
}

/// The texture QQuickImage loads for a shell SVG at `sourceSize` (logical, square):
/// the offline QtSvg bake at `source_size × dpr` ([`crate::baked`]), or, for scales
/// that were not baked, skia's SVG renderer at the same pixel size.
pub fn svg_raster(key: &str, dom: &svg::Dom, source_size: f32, dpr: f32) -> Option<Image> {
    if let Some(img) = crate::baked::raster(key, source_size, dpr) {
        return Some(img);
    }
    let px = (source_size * dpr).round().max(1.) as i32;
    let mut surface = sk::surfaces::raster_n32_premul((px, px))?;
    render_svg_into(dom, surface.canvas(), px as f32, px as f32);
    Some(surface.image_snapshot())
}

/// An image item drawn by the engine (`Node.image_sampling` from QML smooth/mipmap).
pub fn image_node(frame: Rect, img: Image, sampling: ImageSampling) -> Node {
    let mut n = Node::new(frame, Content::Image(img));
    n.image_sampling = sampling;
    n
}


// ------------------------------------------------------------------ split tile

#[derive(Clone, Debug)]
pub struct Split {
    pub icon: String,
    pub title: String,
    pub subtitle: String,
    pub active: bool,
    pub destructive: bool,
    pub left_enabled: bool,
    pub right_enabled: bool,
    pub chevron: bool,
    pub expanded: bool,
}

/// ControlPanelSplitTile. `left`/`right` = zone ids of the icon / content click areas.
pub fn split_tile<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), s: &Split, left: Z, right: Z, inter: &Interaction<Z>) -> Node {
    let pal = c.pal;
    let (w, h) = (frame.width, frame.height);
    let accent = if s.destructive { pal.critical } else { pal.accent };
    let active_fill = with_alpha(accent, if pal.dark { 0.28 } else { 0.22 });
    let active_stroke = with_alpha(accent, if pal.dark { 0.42 } else { 0.28 });
    let inset = 6f32.max((h * 0.14).round());
    let well_w = 28f32.max(h - 2. * inset);
    let well_h = 28f32.max(h - 2. * inset);
    let icon_px = (well_w.min(well_h) * 0.48).clamp(16., 20.);
    let content_x = inset + well_w + 9.;
    let mut n = rect(frame, 19., if s.active { active_fill } else { pal.card_fill }, Some((1., if s.active { active_stroke } else { pal.card_stroke })));
    let me = at(pabs, frame);
    let lp = s.left_enabled && inter.is_pressed(&left);
    let lh = s.left_enabled && inter.is_hovered(&left);
    let wf = pal.icon_well_fill(s.active);
    let well_color = if lp { darker(wf, 1.08) } else if lh { lighter(wf, 1.04) } else { wf };
    let wframe = Rect::new(inset, inset, well_w, well_h);
    let mut well = rect(wframe, 11., well_color, None);
    let icon_color = if s.active { rgb("#fff7ef") } else { pal.primary_text };
    well.children.push(text_center(&tb(&s.icon, icon_px, BOLD, icon_color), well_w, well_h, at(me, wframe), c.dpr));
    n.children.push(well);
    // text block
    let block_w = (w - 10. - content_x).max(0.);
    let chevron = tb(if s.expanded { "\u{F077}" } else { "\u{F078}" }, 11., BOLD, if s.active { rgb("#ffffff") } else { pal.muted_text });
    let (cw, ch) = text_size(&chevron);
    let col_w = if s.chevron { (block_w - cw - 6.).max(0.) } else { block_w };
    let title = tb(&s.title, 11., BOLD, if s.active { rgb("#ffffff") } else { pal.primary_text }).width(col_w).elide();
    let sub = TextBox::new(s.subtitle.clone(), Font::native(9., MEDIUM), if s.active { with_alpha(rgb("#ffffff"), 0.82) } else { pal.muted_text }).width(col_w).elide();
    let th = text_size(&title).1;
    let sh = if s.subtitle.is_empty() { 0. } else { text_size(&sub).1 };
    let col_h = th + if s.subtitle.is_empty() { 0. } else { 2. + sh };
    let bframe = Rect::new(content_x, center(h, col_h), block_w, col_h);
    let babs = at(me, bframe);
    let mut block = group(bframe);
    block.children.push(text(&title, Rect::new(0., 0., col_w, th), babs, c.dpr));
    if !s.subtitle.is_empty() {
        block.children.push(text(&sub, Rect::new(0., th + 2., col_w, sh), babs, c.dpr));
    }
    if s.chevron {
        block.children.push(text(&chevron, Rect::new(block_w - cw, center(col_h, ch), cw, ch), babs, c.dpr));
    }
    n.children.push(block);
    c.zone_if(s.left_enabled, left, abs(me, Rect::new(0., 0., content_x, h)), Cursor::Pointer);
    c.zone_if(s.right_enabled, right, abs(me, Rect::new(content_x, 0., w - content_x, h)), Cursor::Pointer);
    n
}

// ------------------------------------------------------------------ slider tile

#[derive(Clone, Debug)]
pub struct Slider {
    pub icon: String,
    /// 0..100 (displayValue: preview while adjusting)
    pub value: f32,
    pub icon_left_inset: f32,
}

pub fn slider_icon_px(h: f32) -> f32 {
    (h * 0.34).clamp(18., 22.)
}
pub fn slider_icon_center_x(w: f32, h: f32) -> f32 {
    (w - h / 2.).min((h / 2.).max(w * 0.24))
}
/// `computeValue(x) = round(clamp(x/w, 0, 1)·100)`
pub fn slider_value(x: f32, w: f32) -> f32 {
    ((x / w).clamp(0., 1.) * 100.).round()
}
/// Icon hit box test (iconClickable).
pub fn slider_icon_hit(s: &Slider, w: f32, h: f32, x: f32, y: f32) -> bool {
    let cx = if s.icon_left_inset >= 0. { s.icon_left_inset + slider_icon_px(h) / 2. } else { slider_icon_center_x(w, h) };
    let half = 18f32.max(h * 0.3);
    (x - cx).abs() <= half && (y - h / 2.).abs() <= half
}

pub fn slider<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), s: &Slider, id: Z, enabled: bool) -> Node {
    let pal = c.pal;
    let (w, h) = (frame.width, frame.height);
    let v = s.value.clamp(0., 100.);
    let fill_w = w * v / 100.;
    let progress = with_alpha(pal.accent, if pal.dark { 0.28 } else { 0.22 });
    let mut n = rect(frame, 19., pal.card_fill, None);
    let me = at(pabs, frame);
    let mut prog_full = rect(Rect::new(0., 0., w, h), 19., progress, None);
    prog_full.children.clear();
    n.children.push(clip(Rect::new(0., 0., fill_w, h), vec![prog_full]));
    let px = slider_icon_px(h);
    let glyph_w = qtext::measure(&s.icon, &Font::native(px, BOLD)).width;
    let gh = qtext::measure(&s.icon, &Font::native(px, BOLD)).height;
    let gx = if s.icon_left_inset >= 0. { s.icon_left_inset } else { slider_icon_center_x(w, h) - glyph_w / 2. };
    let gy = center(h, gh);
    let white = tb(&s.icon, px, BOLD, rgb("#ffffff"));
    let solid = tb(&s.icon, px, BOLD, progress);
    let fill_clip = Rect::new(0., 0., fill_w, h);
    n.children.push(clip(fill_clip, vec![text(&white, Rect::new(gx, gy, glyph_w, gh), me, c.dpr)]));
    let empty_clip = Rect::new(fill_w, 0., (w - fill_w).max(0.), h);
    n.children.push(clip(empty_clip, vec![text(&solid, Rect::new(gx - fill_w, gy, glyph_w, gh), at(me, empty_clip), c.dpr)]));
    n.children.push(rect(Rect::new(0., 0., w, h), 19., Color::default(), Some((1., pal.card_stroke))));
    c.zone_if(enabled, id, abs(pabs, frame), Cursor::Pointer);
    n
}

// ------------------------------------------------------------------ media card

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaButton {
    Previous,
    PlayPause,
    Next,
}

pub fn media_card<Z: Clone + PartialEq>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), m: &crate::model::Media, ids: [Z; 3]) -> Node {
    let pal = c.pal;
    let (w, h) = (frame.width, frame.height);
    let mut n = rect(frame, 19., pal.card_strong_fill, Some((1., pal.card_stroke)));
    n.clip = Some([0.; 4]);
    let me = at(pabs, frame);
    let art = (h * 0.42).clamp(42., 44.);
    let art_r = 11f32.min(art / 2.);
    let art_fill = with_alpha(rgb("#ffffff"), if pal.dark { 0.08 } else { 0.34 });
    let aframe = Rect::new(9., 9., art, art);
    let mut art_node = rect(aframe, art_r, art_fill, None);
    art_node.clip = Some([0.; 4]);
    let has_img = m.available && !m.art_url.is_empty() && m.art.is_some();
    if has_img {
        let img = m.art.as_ref().unwrap();
        art_node.children.push(masked_art(img, art, art_r));
    } else {
        let g = tb("\u{F001}", (art * 0.42).round(), BOLD, pal.muted_text);
        art_node.children.push(text_center(&g, art, art, at(me, aframe), c.dpr));
    }
    n.children.push(art_node);
    let title = if m.available { if m.title.is_empty() { "Unknown track".to_string() } else { m.title.clone() } } else { "Idle".to_string() };
    let tw = w - 18.;
    let t = tb(&title, 12., BOLD, pal.primary_text).width(tw).elide();
    let th = text_size(&t).1;
    n.children.push(text(&t, Rect::new(9. + 2., 9. + art + 13., tw, th), me, c.dpr));
    // controls row
    let sec = (15f32 * 1.7).round();
    let pri = (18f32 * 1.7).round();
    let row_h = pri + 3.;
    let sec_bottom = (3. + (pri - sec) / 2.).round().max(0.);
    let widths = [sec + 8., pri + 10., sec + 8.];
    let row_w = widths.iter().sum::<f32>() + 16.;
    let row_x = center(w, row_w);
    let row_y = h - row_h;
    let glyphs = ["\u{F04AE}", if m.playing { "\u{F03E4}" } else { "\u{F040A}" }, "\u{F04AD}"];
    let mut x = row_x;
    for (i, id) in ids.into_iter().enumerate() {
        let primary = i == 1;
        let bw = widths[i];
        let bframe = Rect::new(x, row_y, bw, row_h);
        let color = if primary { rgb("#000000") } else { pal.primary_text };
        let g = tb(glyphs[i], if primary { pri } else { sec }, BOLD, color);
        let (gw, gh) = text_size(&g);
        let bottom = if primary { 3. } else { sec_bottom };
        let mut b = group(bframe);
        b.children.push(text(&g, Rect::new(center(bw, gw), row_h - bottom - gh, gw, gh), at(me, bframe), c.dpr));
        if !m.available {
            b.opacity = 0.6;
        }
        n.children.push(b);
        c.zone_if(m.available, id, abs(me, bframe), Cursor::Pointer);
        x += bw + 8.;
    }
    n
}

/// ControlPanelMediaCard album art: `Image { PreserveAspectCrop; smooth; mipmap;
/// layer.effect: MultiEffect { maskEnabled; maskSource: <Rectangle radius r, layer>;
/// maskThresholdMin: 0.5; maskSpreadAtMin: 1 } }`. The effect layer is the item, so the
/// image child at its crop rect is clipped exactly like QQuickImage's source rect.
pub fn masked_art(img: &Image, side: f32, radius: f32) -> Node {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let scale = (side / iw).max(side / ih);
    let (dw, dh) = (iw * scale, ih * scale);
    let mut n = group(Rect::new(0., 0., side, side));
    n.effects.multi_effect = Some(MultiEffect {
        mask: Some(Mask { source: MaskSource::Rect { radius }, threshold_min: 0.5, spread_at_min: 1., ..Default::default() }),
        ..Default::default()
    });
    n.children.push(image_node(Rect::new((side - dw) / 2., (side - dh) / 2., dw, dh), img.clone(), ImageSampling::qt(true, true)));
    n
}

// ------------------------------------------------------------------ switch

/// The existing animated appearance and input state of a Bluetooth switch.
pub struct PowerSwitch {
    pub fill: Color,
    pub stroke: Color,
    pub knob_x: f32,
    pub enabled: bool,
    pub dim: bool,
}

/// Bluetooth power switch 48×28 (ui-panels-control.md §3.2 header). `fill` and `knob_x`
/// are the animated values (ColorAnimation 140 ms / knob x 160 ms InOutQuad).
pub fn power_switch<Z: Clone>(c: &mut Ctx<Z>, frame: Rect, pabs: (f32, f32), state: PowerSwitch, id: Z) -> Node {
    let PowerSwitch { fill, stroke, knob_x, enabled, dim } = state;
    let pal = c.pal;
    let mut n = rect(frame, 14., fill, Some((1., stroke)));
    let knob = if pal.dark { rgb("#f7f7fa") } else { rgb("#ffffff") };
    let knob_stroke = with_alpha(rgb("#000000"), if pal.dark { 0.18 } else { 0.10 });
    n.children.push(rect(Rect::new(knob_x, center(frame.height, 22.), 22., 22.), 11., knob, Some((1., knob_stroke))));
    if dim {
        n.opacity = 0.58;
    }
    c.zone_if(enabled, id, abs(pabs, frame), Cursor::Pointer);
    n
}

// ------------------------------------------------------------------ icon with fallback

/// WifiIconWithFallback (24 px box): SVG when the asset exists, else the Nerd glyph.
/// Symbolic (dark) sources draw at 16 px and both icon and glyph sit 2 px higher.
pub fn icon_with_fallback(pal: &Palette, frame: Rect, pabs: (f32, f32), source: &str, glyph: &str, assets: &crate::model::Assets, dpr: f32) -> Node {
    // QML tests the absolute path for "/symbolic/"; asset keys are theme-relative
    let symbolic = source.contains("/symbolic/") || source.starts_with("symbolic/");
    let size = 24f32;
    let eff = (size * if symbolic { 16. / 24. } else { 1. }).round().max(16.);
    let dy = if symbolic { -2. } else { 0. };
    let mut n = group(frame);
    let me = at(pabs, frame);
    if let Some(dom) = assets.svgs.get(source) {
        let f = Rect::new(center(size, eff), center(size, eff) + dy, eff, eff);
        // IconImage: Image { sourceSize: item size; smooth; mipmap: true }
        if let Some(img) = svg_raster(source, dom, eff, dpr) {
            n.children.push(image_node(f, img, ImageSampling::qt(true, true)));
        }
    } else {
        let px = 13f32.max(eff - 2.);
        let t = tb(glyph, px, BOLD, pal.primary_text);
        let (w, h) = text_size(&t);
        n.children.push(text(&t, Rect::new(center(size, w), center(size, h) + dy, w, h), me, dpr));
    }
    n
}

/// Rasterise an SVG to `w`×`h` pixels (Qt's QSvgRenderer at `sourceSize`): scale the
/// document's intrinsic size (or viewBox) to the target, PreserveAspectFit, centred.
pub fn render_svg_into(dom: &svg::Dom, canvas: &sk::Canvas, w: f32, h: f32) {
    let mut d = dom.clone();
    canvas.clear(sk::Color::TRANSPARENT);
    let root = d.root();
    let vb = root.view_box();
    let (iw, ih) = match vb {
        Some(r) if r.width() > 0. && r.height() > 0. => (r.width(), r.height()),
        _ => {
            let s = root.intrinsic_size();
            (s.width.max(1.), s.height.max(1.))
        }
    };
    let k = (w / iw).min(h / ih);
    canvas.save();
    canvas.translate(((w - iw * k) / 2., (h - ih * k) / 2.));
    canvas.scale((k, k));
    d.set_container_size((iw, ih));
    d.render(canvas);
    canvas.restore();
}


// ------------------------------------------------------------------ trend chart

/// SystemTrendChart (ui-panels-stats.md §3).
/// SystemTrendChart: frame rectangles plus the Canvas, painted by Qt's raster engine
/// port (engine `hyprv_ui::canvas` / `qt_painter`) at the output scale.
#[allow(clippy::too_many_arguments)]
pub fn trend_chart(pal: &Palette, frame: Rect, samples: &[f64], max_value: f64, auto_scale: bool, accent: Color) -> Node {
    let dark = pal.dark;
    let frame_color = with_alpha(pal.primary_text, if dark { 0.12 } else { 0.10 });
    let grid = with_alpha(pal.primary_text, if dark { 0.10 } else { 0.08 });
    let panel_fill = with_alpha(rgb("#ffffff"), if dark { 0.055 } else { 0.032 });
    let overlay = with_alpha(rgb("#ffffff"), if dark { 0.018 } else { 0. });
    let top = with_alpha(accent, if dark { 0.18 } else { 0.14 });
    let bottom = with_alpha(accent, 0.02);
    let mut n = group(frame);
    let mut fr = rect(Rect::new(0., 0., frame.width, frame.height), 10., panel_fill, Some((1., frame_color)));
    fr.opacity = 0.82;
    if dark {
        let mut o = rect(Rect::new(0., 0., frame.width, frame.height), 10., overlay, None);
        o.opacity = 1.;
        fr.children.push(o);
    }
    n.children.push(fr);
    // Canvas { anchors.fill: parent; anchors.margins: 2 } — onPaint replayed on the
    // engine Canvas (QQuickContext2D paths, QPainter stroker/gray raster, one image)
    let (cw, ch) = (frame.width - 4., frame.height - 4.);
    let chart = trend_canvas((cw as f64, ch as f64), samples, max_value, auto_scale, accent, grid, top, bottom);
    // QQuickContext2DImageTexture: the engine Canvas node rasterises round(size) × dpr
    // and draws it into the item rect with linear filtering
    n.children.push(chart.node(Rect::new(2., 2., cw, ch)));
    n
}

/// SystemTrendChart.qml `onPaint` (left/right/top/bottom insets 10): grid lines, area
/// with a vertical gradient, the polyline (2.25, round cap/join) and the end dot.
#[allow(clippy::too_many_arguments)]
pub fn trend_canvas(size: (f64, f64), samples: &[f64], max_value: f64, auto_scale: bool, accent: Color, grid: Color, area_top: Color, area_bottom: Color) -> CanvasBatch {
    let (width, height) = size;
    let (left, right, top, bottom) = (10., 10., 10., 10.);
    let chart_w = (width - left - right).max(1.);
    let chart_h = (height - top - bottom).max(1.);
    let stroke = |color: Color, width: f32, round: bool| StrokeStyle {
        width,
        color,
        cap: if round { sk::paint::Cap::Round } else { sk::paint::Cap::Butt },
        join: if round { sk::paint::Join::Round } else { sk::paint::Join::Miter },
    };
    let paint = |path: CanvasPath, fill: Option<Color>, gradient: Option<VerticalGradient>, stroke: Option<StrokeStyle>| CanvasPaint { path, fill, gradient, stroke };
    let mut ops = Vec::new();
    const GRID_LINES: usize = 4;
    for i in 0..GRID_LINES {
        let y = top + chart_h * (i as f64 / (GRID_LINES - 1) as f64);
        let mut path = CanvasPath::new();
        path.move_to(left, y).line_to(left + chart_w, y);
        ops.push(paint(path, None, None, Some(stroke(grid, 1., false))));
    }
    if !samples.is_empty() {
        let mut upper = if auto_scale { 0. } else { max_value.max(1.) };
        for v in samples {
            upper = upper.max(v.max(0.));
        }
        if auto_scale {
            upper *= 1.15;
        }
        let max_value = upper.max(1.);
        let step = if samples.len() > 1 { chart_w / (samples.len() - 1) as f64 } else { 0. };
        let mut pts: Vec<(f64, f64)> = samples.iter().enumerate().map(|(i, v)| (left + step * i as f64, top + chart_h - v.clamp(0., max_value) / max_value * chart_h)).collect();
        if pts.len() == 1 {
            pts.push((left + chart_w, pts[0].1));
        }
        let mut area = CanvasPath::new();
        area.move_to(pts[0].0, top + chart_h);
        for &(x, y) in &pts {
            area.line_to(x, y);
        }
        area.line_to(pts[pts.len() - 1].0, top + chart_h).close();
        ops.push(paint(area, None, Some(VerticalGradient { y0: top, y1: top + chart_h, top: area_top, bottom: area_bottom }), None));
        let mut line = CanvasPath::new();
        line.move_to(pts[0].0, pts[0].1);
        for &(x, y) in &pts[1..] {
            line.line_to(x, y);
        }
        ops.push(paint(line, None, None, Some(stroke(accent, 2.25, true))));
        let (lx, ly) = pts[pts.len() - 1];
        let mut dot = CanvasPath::new();
        dot.arc(lx, ly, 3., 0., std::f64::consts::TAU, false).close();
        ops.push(paint(dot, Some(accent), None, None));
    }
    CanvasBatch { operations: ops }
}

/// Text helper for panel labels: native, given px/weight.
pub fn label(s: &str, px: f32, weight: i32, color: Color) -> TextBox {
    tb(s, px, weight, color)
}
pub fn label_n(s: &str, px: f32, color: Color) -> TextBox {
    tb(s, px, NORMAL, color)
}
