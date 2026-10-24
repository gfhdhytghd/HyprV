//! HyprV Dynamic Island (docs/spec/ui-island.md), ported from `quickshell/DynamicIsland.qml`.
//!
//! Pure UI: no I/O. The host feeds [`Inputs`] built from service topics, forwards
//! pointer events in bar-surface coordinates, drives [`Island::frame`] while
//! [`Island::animating`] or when [`Island::next_deadline`] fires, mounts the node from
//! [`Island::build`] (or reconciles it with [`update`]) and dispatches [`IslandAction`]s.
mod assets;
mod spectrum;
pub mod island;
pub mod model;
pub mod util;
mod view;

pub use island::{CursorShape, Derived, Geometry, Island, Pointer, PointerOutcome, Target};
pub use model::{Agent, Inputs, IslandAction, Media};

use hyprv_ui::scene::{Content, Node, NodeId, Scene};

fn content_equal(a: &Content, b: &Content) -> bool {
    match (a, b) {
        (Content::Group, Content::Group) => true,
        (Content::Rect(a), Content::Rect(b)) => a == b,
        (Content::Text(a), Content::Text(b)) => a == b,
        (Content::Image(a), Content::Image(b)) => a.unique_id() == b.unique_id(),
        // Recorded vector pictures (spectrum, rings) are rebuilt; treat as changed.
        _ => false,
    }
}

/// Reconcile `new` into the scene node `old_id`: only changed properties/children are
/// edited, so an unchanged island produces no damage (beyond rebuilt vector pictures).
pub fn reconcile(scene: &mut Scene, old_id: NodeId, mut new: Node) -> usize {
    let Some(old) = scene.root().find(old_id) else { return 0 };
    let old_children: Vec<_> = old.children.iter().map(Node::id).collect();
    let changed = old.frame != new.frame
        || old.opacity != new.opacity
        || old.visible != new.visible
        || old.clip != new.clip
        || old.qt_rect_size != new.qt_rect_size
        || old.native_materials != new.native_materials
        || old.native_text_mode != new.native_text_mode
        || old.transform != new.transform
        || !content_equal(&old.content, &new.content);
    let mut edits = 0;
    if changed {
        scene.edit(old_id, |node| {
            node.frame = new.frame;
            node.opacity = new.opacity;
            node.visible = new.visible;
            node.clip = new.clip;
            node.transform = new.transform;
            node.qt_rect_size = new.qt_rect_size;
            node.native_materials = new.native_materials;
            node.native_text_mode = new.native_text_mode;
            node.content = new.content.clone();
        });
        edits += 1;
    }
    let paired = old_children.len().min(new.children.len());
    let mut children = std::mem::take(&mut new.children).into_iter();
    for id in old_children.iter().take(paired) {
        edits += reconcile(scene, *id, children.next().unwrap());
    }
    let added: Vec<_> = children.collect();
    if old_children.len() != paired || !added.is_empty() {
        scene.edit(old_id, |node| {
            node.children.truncate(paired);
            node.children.extend(added);
        });
        edits += 1;
    }
    edits
}

/// Rebuild the island and reconcile it into `island_root` (a node previously created
/// from [`Island::build`]). Skips the rebuild while [`Island::visual_rev`] is unchanged
/// since the last update of the same root.
pub fn update(scene: &mut Scene, island_root: NodeId, island: &Island) -> usize {
    let key = island.visual_rev().map(|rev| (island_root, rev));
    if key.is_some() && island.reconciled.get() == key {
        return 0;
    }
    let edits = reconcile(scene, island_root, island.build());
    island.reconciled.set(key);
    edits
}

#[cfg(test)]
mod exact_material_tests {
    use super::*;
    #[test]
    fn reconcile_preserves_native_material_and_text_selection() {
        let island=Island::new(Inputs::default(),640.,std::time::Duration::ZERO);
        let mut scene=Scene::new(island.build());let id=scene.root().id();
        scene.edit(id,|n|n.native_materials=None);
        assert!(reconcile(&mut scene,id,island.build())>0);
        assert_eq!(scene.root().native_materials,Some(true));
        fn check(n:&Node) {if let Content::Text(t)=&n.content {if t.render_type==hyprv_ui::scene::RenderType::NativeRendering {assert_eq!(n.native_text_mode,hyprv_ui::scene::NativeTextMode::QtGl);}}for c in &n.children {check(c);}}
        check(scene.root());
    }
    #[test]
    fn reconcile_detects_qreal_size_changes_with_identical_float_bounds() {
        let make=|width:f64| {
            let mut node=Node::new(hyprv_ui::geometry::Rect::new(0.,0.,width as f32,38.),Content::Group);
            node.qt_rect_size=Some((width,38.));node
        };
        let old=262.99125364431484;
        let new=old as f32 as f64;
        let mut scene=Scene::new(make(old));let id=scene.root().id();
        assert_eq!(make(old).frame,make(new).frame);
        assert_eq!(reconcile(&mut scene,id,make(new)),1);
        assert_eq!(scene.root().qt_rect_size,Some((new,38.)));
        assert_eq!(reconcile(&mut scene,id,make(new)),0);
    }

    #[test]
    fn alignment_is_local_to_the_text_item() {
        let n=crate::util::text("\u{f023}",11.,8.,Some(12.59375),crate::util::HAlign::Center,16.,700,hyprv_ui::animation::Color::rgba(1.,1.,1.,1.));
        assert_eq!(n.frame.x,11.);assert_eq!(n.frame.width,12.59375);
        let Content::Text(t)=n.content else {panic!("text")};assert_eq!(t.align,hyprv_ui::skia::textlayout::TextAlign::Center);
    }
}
