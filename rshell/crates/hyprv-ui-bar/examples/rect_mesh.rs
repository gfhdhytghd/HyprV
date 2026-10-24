//! Diagnostic geometry export; compare with tools/rect_mesh.cpp, never runtime Qt.
use hyprv_ui::{animation::Color,geometry::Rect,qt_rect,scene::{Border,Fill,Rectangle}};
fn main(){
 let a=std::env::args().skip(1).map(|s|s.parse::<f32>().unwrap()).collect::<Vec<_>>();
 let m=qt_rect::tessellate(Rect::new(0.,0.,a[0],a[2]),&Rectangle{fill:Fill::Solid(Color::rgba(30./255.,30./255.,46./255.,1.)),radii:[a[3];4],border:Some(Border{width:a[1],color:Color::rgba(205./255.,214./255.,244./255.,29./255.)})}).unwrap();
 let v=m.vertices.iter().map(|v|serde_json::json!([v.position[0],v.position[1],v.offset[0],v.offset[1],v.color[0],v.color[1],v.color[2],v.color[3]])).collect::<Vec<_>>();
 println!("{}",serde_json::json!({"vertices":v,"triangles":m.triangles}));
}
