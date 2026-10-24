//! Offline parity probe; production decoding remains memory-only.
use hyprv_ui::{
    geometry::Rect,
    render::Renderer,
    scene::{Content, Node, Scene},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let size: u32 = args[3].parse()?;
    let scale: f64 = args[4].parse()?;
    let decoded = hyprv_ui_bar::svg::decode_backend(
        &std::fs::read(&args[1])?,
        (size as f64 * scale).round() as u32,
        (size as f64 * scale).round() as u32,
        if args.len() > 5 {
            hyprv_ui_bar::svg::Backend::Resvg
        } else {
            hyprv_ui_bar::svg::Backend::GrayOutline
        },
    )
    .ok_or("invalid SVG")?;
    println!("{:?}", decoded.backend);
    Renderer::new().png(
        &Scene::new(Node::new(
            Rect::new(0., 0., size as f32, size as f32),
            Content::Image(decoded.image),
        )),
        size as f32,
        size as f32,
        scale,
        &args[2],
    )?;
    Ok(())
}
