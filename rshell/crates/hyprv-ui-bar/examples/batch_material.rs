//! Independent Qt golden companion; test transport only.
#[path="../../hyprv-ui/tests/common/batch_material.rs"]
mod batch_material;
fn main()->Result<(),Box<dyn std::error::Error>>{
 let out=std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);std::fs::create_dir_all(&out)?;
 for scale in [1.,2.]{for faded in [false,true]{let scene=batch_material::scene(faded,scale);let png=hyprv_ui::offscreen::render_png_if_enabled(&mut hyprv_ui::render::Renderer::new(),&scene,3072.,1728.,scale.into())?.ok_or("set HYPRV_PARITY_BACKEND=gl")?;std::fs::write(out.join(format!("effect-{faded}--dark@{scale}.png")),png)?;}}
 Ok(())
}
