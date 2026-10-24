//! Qt Quick scene-graph details the engine does not cover yet, applied to built trees:
//!
//! - `clip: true` on an axis-aligned item is a GL scissor whose edges are qRound(device
//!   coordinate) (qsgbatchrenderer.cpp updateStencilClip); skia's anti-aliased clip
//!   would leave partial-coverage edge columns. Clip frames are moved onto that
//!   scissor; children and the item's own painting keep their positions.
//! - native text under a scaled item is re-placed as device-snapped glyph quads with
//!   Nearest sampling ([`crate::qtext::transformed_picture`], textmask.vert).
//!
//! Rectangles, images and MultiEffects are drawn by the engine (`hyprv_ui::qt_rect`
//! QtQuick rect mode, `image` sampling, `effects::MultiEffect`).
use hyprv_ui::{
    geometry::Rect,
    scene::{Content, Node, NodeId, Scene},
    skia as sk,
};

/// Qt clips axis-aligned `clip: true` items with a GL scissor rounded to whole device
/// pixels; skia's anti-aliased clip would leave partial-coverage edge columns.
pub const QUIRK_CLIP_IS_DEVICE_SCISSOR: bool = true;

/// Apply the scene-graph details to a freshly built tree; `parent_abs` = absolute
/// logical position of `node`'s parent.
pub fn convert(node: &mut Node, parent_abs: (f32, f32), dpr: f32) {
    convert_x(node, parent_abs, dpr, dpr);
}

/// Uniform scale + translation of a node transform, as (scale, offset in node units).
fn scale_translate(m: &sk::Matrix) -> Option<(f32, (f32, f32))> {
    if m.is_identity() {
        return Some((1., (0., 0.)));
    }
    let (sx, sy) = (m.scale_x(), m.scale_y());
    if m.skew_x() != 0. || m.skew_y() != 0. || (sx - sy).abs() > 1e-6 || sx <= 0. || m.has_perspective() {
        return None;
    }
    Some((sx, (m.translate_x(), m.translate_y())))
}

/// `abs` is expressed in units of the accumulated scale s (= window origin / s) and
/// `dpr` is device px per local unit (= base_dpr · s), so scissors are computed by the
/// unscaled code; native text uses `base_dpr` (textmask.vert snapping).
fn convert_x(node: &mut Node, parent_abs: (f32, f32), dpr: f32, base_dpr: f32) {
    let Some((sn, off)) = scale_translate(&node.transform) else { return };
    snap_clip(node, parent_abs, dpr);
    let s = dpr / base_dpr;
    let (abs, dpr_n) = if sn == 1. {
        ((parent_abs.0 + node.frame.x + off.0, parent_abs.1 + node.frame.y + off.1), dpr)
    } else {
        // window origin of the node's transformed coordinate system
        let o = (s * (parent_abs.0 + node.frame.x + off.0), s * (parent_abs.1 + node.frame.y + off.1));
        let s2 = s * sn;
        ((o.0 / s2, o.1 / s2), base_dpr * s2)
    };
    if (dpr_n - base_dpr).abs() > 1e-6 {
        if let Content::Picture(p) = &node.content {
            let s2 = dpr_n / base_dpr;
            if let Some(t) = crate::qtext::transformed_picture(p, (abs.0 * s2, abs.1 * s2), s2, base_dpr) {
                node.content = Content::Picture(t);
            }
        }
    }
    for c in &mut node.children {
        convert_x(c, abs, dpr_n, base_dpr);
    }
}

/// Device-space scissor of an axis-aligned `clip: true` item (qsgbatchrenderer.cpp
/// updateStencilClip): edges at qRound(device coordinate).
pub fn scissor(abs: f32, len: f32, dpr: f32) -> (f32, f32) {
    let a = (abs * dpr).round();
    let b = ((abs + len) * dpr).round();
    (a / dpr, (b - a).max(0.) / dpr)
}

/// Move a rectangular clip node's edges onto Qt's scissor rectangle; children and the
/// item's own painting keep their absolute positions. Returns true if anything moved.
fn snap_clip(node: &mut Node, parent_abs: (f32, f32), dpr: f32) -> bool {
    if !QUIRK_CLIP_IS_DEVICE_SCISSOR || node.clip != Some([0.; 4]) {
        return false;
    }
    let f = node.frame;
    let (x, w) = scissor(parent_abs.0 + f.x, f.width, dpr);
    let (y, h) = scissor(parent_abs.1 + f.y, f.height, dpr);
    let (dx, dy) = (x - parent_abs.0 - f.x, y - parent_abs.1 - f.y);
    if dx.abs() < 1e-5 && dy.abs() < 1e-5 && (w - f.width).abs() < 1e-5 && (h - f.height).abs() < 1e-5 {
        return false;
    }
    node.frame = Rect::new(f.x + dx, f.y + dy, w, h);
    for c in &mut node.children {
        c.frame.x -= dx;
        c.frame.y -= dy;
    }
    // The item's own painting stays where it was (the scissor only clips it).
    match std::mem::replace(&mut node.content, Content::Group) {
        Content::Group => {}
        Content::Picture(p) => {
            let b = p.cull_rect();
            let mut rec = sk::PictureRecorder::new();
            let canvas = rec.begin_recording(b.with_offset((-dx, -dy)), false);
            canvas.translate((-dx, -dy));
            canvas.draw_picture(&p, None, None);
            node.content = rec.finish_recording_as_picture(None).map_or(Content::Picture(p), Content::Picture);
        }
        other => {
            // geometry content (engine rectangles): keep it at its true frame as the
            // first child, inside the clip
            let mut own = Node::new(Rect::new(-dx, -dy, f.width, f.height), other);
            own.rect_mode = node.rect_mode;
            own.rect_antialias = node.rect_antialias;
            own.image_sampling = node.image_sampling;
            node.children.insert(0, own);
        }
    }
    true
}

/// Apply [`convert`]'s clip snapping to clip nodes inserted by updates, editing only
/// those nodes (idempotent: snapped clips are left alone).
pub fn finalize(scene: &mut Scene, dpr: f32) {
    fn walk(n: &Node, parent: (f32, f32), dpr: f32, out: &mut Vec<(NodeId, (f32, f32))>) {
        if !n.transform.is_identity() {
            return;
        }
        let abs = (parent.0 + n.frame.x, parent.1 + n.frame.y);
        let mut probe = Node::new(n.frame, Content::Group);
        probe.clip = n.clip;
        if snap_clip(&mut probe, parent, dpr) {
            out.push((n.id(), parent));
        }
        for c in &n.children {
            walk(c, abs, dpr, out);
        }
    }
    let mut todo = Vec::new();
    walk(scene.root(), (0., 0.), dpr, &mut todo);
    for (id, parent) in todo {
        scene.edit(id, |n| {
            snap_clip(n, parent, dpr);
        });
    }
}
