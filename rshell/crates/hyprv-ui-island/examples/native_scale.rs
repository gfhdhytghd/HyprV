use hyprv_ui::{geometry::Rect,scene::{Content,Node,Scene,NativeTextMode},render::Renderer,animation::Color,skia as sk};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let out=std::path::PathBuf::from(std::env::args().nth(1).unwrap());std::fs::create_dir_all(&out)?;
 for dark in [true,false]{for dpr in [1.,2.] {
  let mut root=Node::new(Rect::new(0.,0.,480.,120.),Content::Group);root.native_materials=Some(true);
  for i in 0..12 {let scale=[1.,0.96,0.92,1.06,1.1,0.5][i%6];let mut cell=Node::new(Rect::new(10.+(i%6)as f32*75.,if i<6{10.}else{70.},60.,40.),Content::Group);let mut m=sk::Matrix::translate((30.,20.));m.pre_scale((scale,scale),None);m.pre_translate((-30.,-20.));cell.transform=m;
   let color=if dark{Color::rgba(205./255.,214./255.,244./255.,1.)}else{Color::rgba(43./255.,43./255.,44./255.,1.)};
   let mut text=if i<6{hyprv_ui_island::util::centered_text("\u{f023}",0.,0.,60.,40.,16.,700,color)}else{hyprv_ui_island::util::text("09:41",3.,9.,Some(54.),hyprv_ui_island::util::HAlign::Center,16.,700,color)};text.native_text_mode=NativeTextMode::QtGl;cell.add(text);root.add(cell);
  }
  let png=hyprv_ui::offscreen::render_png_if_enabled(&mut Renderer::new(),&Scene::new(root),480.,120.,dpr)?.unwrap();std::fs::write(out.join(format!("scale-probe--{}@{dpr}.png",if dark{"dark"}else{"light"})),png)?;
 }}Ok(())
}
