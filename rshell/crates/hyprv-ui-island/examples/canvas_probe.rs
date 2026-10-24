#[path="../src/spectrum.rs"] mod spectrum;
use hyprv_ui::{geometry::Rect,scene::{Content,Node,Scene},render::Renderer};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let out=std::path::PathBuf::from(std::env::args().nth(1).unwrap());std::fs::create_dir_all(&out)?;
 let values=[12.,38.,74.,92.,66.,41.,58.,80.,95.,61.,33.,47.,70.,52.,28.,15.];
 for dark in [true,false]{for scale in [1.,2.] {
  let tok=hyprv_widgets::controls::Tok::new(dark);let mut root=Node::new(Rect::new(0.,0.,180.,120.),Content::Group);root.native_materials=Some(true);
  for i in 0..6 {let expanded=i>=4;let(x,y,w,h)=if expanded{(10.+(i-4)as f32*90.,50.,70.,56.)}else{(10.+i as f32*40.,10.,22.,22.)};let node=spectrum::build(Rect::new(x,y,w,h),if i==3{&[]}else{&values},i==0||i==3||i==4,if i==1{0.}else{6.44},&tok,expanded);
  if let Content::Canvas(batch)=&node.content {let image=batch.raster((w*scale as f32)as u32,(h*scale as f32)as u32,scale).unwrap();let data=image.encode(None,hyprv_ui::skia::EncodedImageFormat::PNG,None).unwrap();std::fs::write(out.join(format!("source-{i}--{}@{scale}.png",if dark{"dark"}else{"light"})),data.as_bytes())?;}root.add(node);}
  let png=hyprv_ui::offscreen::render_png_if_enabled(&mut Renderer::new(),&Scene::new(root),180.,120.,scale)?.unwrap();std::fs::write(out.join(format!("canvas-probe--{}@{scale}.png",if dark{"dark"}else{"light"})),png)?;
 }}Ok(())
}
