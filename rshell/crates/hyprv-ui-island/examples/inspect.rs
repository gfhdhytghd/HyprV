//! Finite fixture geometry observation; no runtime instrumentation.
#[path="../tests/common/mod.rs"] mod common;
use hyprv_ui::scene::{Content,Node};
fn dump(n:&Node,x:f32,y:f32){let(x,y)=(x+n.frame.x,y+n.frame.y);if let Content::Text(t)=&n.content{println!("{:?} {x:.9} {y:.9} {} {}",t.text,n.frame.width,n.frame.height);}for c in &n.children{dump(c,x,y);}}
fn main(){let path=std::path::PathBuf::from(std::env::args().nth(1).unwrap());let f=serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();let mut r=common::Runner::new(&f,true,1.,path.file_stem().unwrap().to_str().unwrap());r.run_steps(&f["steps"]);dump(&r.scene(),0.,0.);}
