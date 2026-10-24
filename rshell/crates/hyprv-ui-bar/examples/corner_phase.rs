//! Private corner sampling experiment; Qt-free runtime render, no golden reads.
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, Scene},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::var("HYPRV_CORNER_PROBE_OUT")?);
    std::fs::create_dir_all(&out)?;
    for clip in [false, true] {
        let mut root = Node::new(Rect::new(0., 0., 3072., 1728.), Content::Group);
        root.native_materials = Some(true);
        let mut corner = hyprv_ui_bar::surfaces::corner_node_at_scale(
            hyprv_ui_bar::surfaces::Corner::BottomLeft,
            2.,
        );
        corner.frame.y = 1701.;
        if clip {
            corner.clip = Some([0.; 4]);
        }
        root.add(corner);
        let png = hyprv_ui::offscreen::render_png_if_enabled(
            &mut Renderer::new(),
            &Scene::new(root),
            3072.,
            1728.,
            2.,
        )?
        .ok_or("GL required")?;
        std::fs::write(out.join(format!("clip-{clip}.png")), png)?;
    }
    let root = hyprv_ui_bar::surfaces::corner_window(3072., 1728., 2., 0., 0.);
    let png = hyprv_ui::offscreen::render_png_if_enabled(
        &mut Renderer::new(),
        &Scene::new(root),
        3072.,
        1728.,
        2.,
    )?
    .ok_or("GL required")?;
    std::fs::write(out.join("atlas.png"), png)?;
    Ok(())
}
