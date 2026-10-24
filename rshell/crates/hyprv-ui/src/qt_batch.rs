// Copyright (C) 2016 The Qt Company Ltd.
// Copyright (C) 2026 HyprV contributors.
// SPDX-License-Identifier: LGPL-3.0-only
//! The smoothcolor subset of QSG's prepareAlphaBatches/uploadBatch decision.
//! Painting stays in scene order; only the vertex-coordinate basis is selected.
use crate::{scene::{Content, Fill, Node, NodeId, OpacityMode}, skia as sk};
#[derive(Clone, Copy)]
struct Element {
    id: Option<NodeId>,
    opacity: u32,
    scaled: bool,
    barrier: bool,
    bounds: sk::Rect,
}
fn bounds(n: &Node, matrix: &sk::Matrix) -> sk::Rect {
    let r=n.local_bounds();
    matrix.map_rect(sk::Rect::from_xywh(r.x,r.y,r.width,r.height)).0
}
fn collect(n: &Node, matrix: sk::Matrix, alpha: f32, root: bool, elements: &mut Vec<Element>) {
    if !n.visible || alpha <= 0. {return;}
    let boundary=!root && (n.clip.is_some() || n.native_batch_root);
    let foreign=n.native_materials==Some(false) || matches!(n.content,Content::Shared(_));
    let layer=n.effects.multi_effect.is_some() || n.effects.tint.is_some() || n.effects.shadow.is_some() || n.opacity_mode==OpacityMode::Group;
    if boundary || foreign || layer {
        elements.push(Element {id:None,opacity:0,scaled:false,barrier:boundary,bounds:bounds(n,&matrix)});
        return;
    }
    let id=match &n.content {
        Content::Rect(r) if n.rect_antialias.unwrap_or(r.radii[0]>0.) && matches!(r.fill,Fill::Solid(_)) => {
            let Fill::Solid(fill)=r.fill else {unreachable!()};
            (fill.a>0. || r.border.as_ref().is_some_and(|b|b.width>0. && b.color.a>0.)).then_some(n.id())
        },
        _=>None,
    };
    if !matches!(n.content,Content::Group) {
        elements.push(Element {id,opacity:alpha.to_bits(),scaled:matrix.has_perspective() || matrix.scale_x()!=1. || matrix.scale_y()!=1. || matrix.skew_x()!=0. || matrix.skew_y()!=0.,barrier:false,bounds:bounds(n,&matrix)});
    }
    for child in &n.children {
        collect(child,sk::Matrix::concat(&matrix,&child.local_matrix()),alpha*child.opacity.clamp(0.,1.),false,elements);
    }
}
fn intersects(a: sk::Rect,b: sk::Rect) -> bool { a.left<b.right && a.right>b.left && a.top<b.bottom && a.bottom>b.top }
fn union(a: sk::Rect,b: sk::Rect) -> sk::Rect {sk::Rect::new(a.left.min(b.left),a.top.min(b.top),a.right.max(b.right),a.bottom.max(b.bottom))}
/// Called after establishing a clip/batch root; alpha already includes root opacity.
pub(super) fn unmerged_rects(root: &Node,alpha: f32,preserve_root_precision: bool)->Vec<NodeId> {
    let mut elements=Vec::new();collect(root,sk::Matrix::new_identity(),alpha,true,&mut elements);
    let mut assigned=vec![false;elements.len()];let mut unmerged=Vec::new();
    for i in 0..elements.len() {
        if assigned[i] || elements[i].id.is_none() {continue;}
        assigned[i]=true;let mut batch=vec![i];let mut overlap:Option<sk::Rect>=None;
        for j in i+1..elements.len() {
            let e=elements[j];
            if e.barrier {break;}
            let compatible=!assigned[j] && e.id.is_some() && e.opacity==elements[i].opacity;
            if compatible {
                let obstructed=overlap.is_some_and(|r|intersects(r,e.bounds)) && elements[i+1..j].iter().any(|r|intersects(r.bounds,e.bounds));
                if obstructed {break;}
                assigned[j]=true;batch.push(j);
            } else {overlap=Some(overlap.map_or(e.bounds,|r|union(r,e.bounds)));}
        }
        // A scaled clip may have a device-snapped layout origin with the
        // opposite displacement on its children. Folding that displacement
        // into AA vertices rounds twice. Keep the complete matrix so the
        // original curve coordinates reach the shader unchanged.
        if preserve_root_precision || batch.iter().any(|i|elements[*i].scaled) {
            unmerged.extend(batch.into_iter().filter_map(|i|elements[i].id));
        }
    }
    unmerged
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{animation::Color,geometry::Rect,scene::Rectangle};
    fn root()->Node {Node::new(Rect::new(0.,0.,100.,60.),Content::Group)}
    fn circle(x:f32,scaled:bool)->Node {
        let mut n=Node::new(Rect::new(x,0.,20.,20.),Content::Rect(Rectangle {fill:Fill::Solid(Color::rgba(1.,1.,1.,1.)),radii:[10.;4],border:None}));
        if scaled {n.transform=sk::Matrix::scale((1.06,1.06));}n
    }
    #[test]
    fn scaled_clip_preserves_child_matrix_precision() {
        let mut n=root();let mut child=circle(0.17382812,false);
        child.frame.y= -0.03625;n.add(child);
        assert!(unmerged_rects(&n,1.,false).is_empty());
        assert_eq!(unmerged_rects(&n,1.,true),vec![n.children[0].id()]);
    }
    #[test]
    fn scaled_sibling_unmerges_the_compatible_unscaled_circle() {
        let mut n=root();n.add(circle(0.,false));n.add(circle(40.,true));
        assert_eq!(unmerged_rects(&n,1.,false),n.children.iter().map(Node::id).collect::<Vec<_>>());
    }
    #[test]
    fn different_opacity_does_not_unmerge_other_rectangles() {
        let mut n=root();n.add(circle(0.,false));let mut scaled=circle(40.,true);scaled.opacity=0.5;let id=scaled.id();n.add(scaled);
        assert_eq!(unmerged_rects(&n,0.8,false),vec![id]);
    }
    #[test]
    fn nested_clip_root_ends_the_batch() {
        let mut n=root();n.add(circle(0.,false));let mut clip=root();clip.clip=Some([0.;4]);n.add(clip);let scaled=circle(40.,true);let id=scaled.id();n.add(scaled);
        assert_eq!(unmerged_rects(&n,1.,false),vec![id]);
    }
    #[test]
    fn intervening_overlap_prevents_reordering_into_the_batch() {
        let mut n=root();n.add(circle(0.,false));let mut blocker=circle(40.,false);blocker.opacity=0.5;n.add(blocker);let scaled=circle(40.,true);let id=scaled.id();n.add(scaled);
        assert_eq!(unmerged_rects(&n,1.,false),vec![id]);
    }
    #[test]
    fn disjoint_intervening_content_allows_the_batch() {
        let mut n=root();n.add(circle(0.,false));let mut blocker=circle(70.,false);blocker.opacity=0.5;n.add(blocker);n.add(circle(40.,true));
        assert_eq!(unmerged_rects(&n,1.,false),vec![n.children[0].id(),n.children[2].id()]);
    }
}
