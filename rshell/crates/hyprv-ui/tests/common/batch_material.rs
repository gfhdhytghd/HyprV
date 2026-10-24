use hyprv_ui::{animation::Color,effects::MultiEffect,geometry::Rect,scene::{Border,Content,Fill,Node,Rectangle,Scene},skia as sk};
fn rectangle(frame:Rect,radius:f32,color:Color,border:Option<Border>)->Node{Node::new(frame,Content::Rect(Rectangle{fill:Fill::Solid(color),radii:[radius;4],border}))}
pub fn scene(faded:bool,scale:f32)->Scene {
 let mut root=Node::new(Rect::new(0.,0.,3072.,1728.),Content::Group);root.native_materials=Some(true);
 root.add(rectangle(Rect::new(10.,10.,223.375,38.),19.,Color::rgba(30./255.,30./255.,46./255.,0.8),None));
 root.add(rectangle(Rect::new(2433.375,10.,400.875,38.),19.,Color::rgba(1.,1.,1.,0.28),Some(Border{width:1.,color:Color::rgba(0.15,0.15,0.16,0.10)})));
 let control=sk::Image::from_encoded(sk::Data::new_copy(include_bytes!("../fixtures/batch-materials/control.png"))).unwrap();
 let mut image=Node::new(Rect::new(3012.109375_f64 as f32,21.,16.,16.),Content::Image(control));image.image_sampling=hyprv_ui::image::ImageSampling::Mipmap;image.opacity=if faded {0.72}else{1.};root.add(image);
 let mut clip=Node::new(Rect::new(2755.,60.,100.,35.),Content::Group);clip.clip=Some([0.;4]);
 let cover=sk::Image::from_encoded(sk::Data::new_copy(include_bytes!("../fixtures/batch-materials/cover.png"))).unwrap();let mut image=Node::new(Rect::new(9.,9.,16.,16.),Content::Image(cover));image.opacity=if faded {0.72}else{1.};clip.add(image);root.add(clip);
 let mut layer=Node::new(Rect::new(2516.,58.,324.,190.),Content::Group);layer.effects.multi_effect=Some(MultiEffect{source_texture_size:Some(((324.*scale).round()as u32,(190.*scale).round()as u32)),shadow_enabled:true,shadow_blur:10./64.,shadow_color:Color::rgba(0.,0.,0.,0.45),..Default::default()});let mut fill=rectangle(Rect::new(0.,0.,324.,190.),19.,Color::rgba(16./255.,18./255.,20./255.,0.42),Some(Border{width:1.,color:Color::rgba(205./255.,214./255.,244./255.,0.14)}));fill.opacity=0.82;layer.add(fill);root.add(layer);
 Scene::new(root)
}
