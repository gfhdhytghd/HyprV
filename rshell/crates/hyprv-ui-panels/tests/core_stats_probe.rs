//! Finite private diagnostics. Ignored in ordinary CI; no runtime hooks.
mod shared_common;
use hyprv_ui::scene::{Content, Node};
use hyprv_ui_panels::{model::{Anchor, Assets}, stats::StatsPopup};
use std::{path::PathBuf, rc::Rc, time::Duration};

#[test]
fn fixture_stream_uses_qml_timer_delivery_clock() {
    let root=shared_common::rshell();
    let fixture:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("parity/fixtures/panels/stats-120.json")).unwrap()).unwrap();
    let mut t=shared_common::translate::Translator::new(&fixture,true,1.,&root,Rc::new(Assets::default()));
    t.advance_to(0.);
    assert!(t.model.stats.network_history.is_empty());
    t.advance_to(50.);
    assert_eq!(t.model.stats.network_history,vec![0.]);
    t.advance_to(1000.);
    // Observed from unmodified SystemTrendChart in the installed Qt host:
    // Process.qml's min-1ms Timer delivers the at=0 line on the first50ms tick.
    assert_eq!(t.model.stats.network_history,vec![0.,2155.7894736842104]);
    t.advance_to(2000.);
    assert_eq!(t.model.stats.network_history[2],512000.);
}

#[test]
#[ignore = "finite stats source-image diagnostic; CORE_STATS_OUT required"]
fn stats_sources() {
    let out=PathBuf::from(std::env::var_os("CORE_STATS_OUT").expect("CORE_STATS_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let state=std::env::var("CORE_STATS_STATE").unwrap_or_else(|_|"stats-120".into());
    let root=shared_common::rshell();
    let fixture:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join(format!("parity/fixtures/panels/{state}.json"))).unwrap()).unwrap();
    let mut t=shared_common::translate::Translator::new(&fixture,true,1.,&root,Rc::new(Assets::default()));
    for ms in (0..=122000).step_by(50) {t.advance_to(ms as f64);}
    let mut model=t.model;model.now=Duration::from_secs(122);
    let mut stats=StatsPopup::new();stats.open_for(Anchor{x:0.,y:10.,width:0.,height:38.},"DP-4",Duration::ZERO);
    stats.build(&model);
    for ms in (0..=1000).step_by(10) {stats.tick(Duration::from_millis(ms));}
    let mut scene=stats.build(&model);scene.native_materials=Some(true);
    let mut lines=Vec::new();let mut seq=0;
    fn walk(n:&Node,x:f32,y:f32,lines:&mut Vec<String>,seq:&mut usize,out:&std::path::Path) {
        let (x,y)=(x+n.frame.x,y+n.frame.y);
        if let Content::Rect(r)=&n.content {lines.push(format!("RECT x{x:?} y{y:?} frame{:?} radius{:?} opacity{:?}",n.frame,r.radii,n.opacity));}
        if let Content::Canvas(batch)=&n.content {
            lines.push(format!("CANVAS index{seq} x{x:?} y{y:?} frame{:?}",n.frame));
            for scale in [1.,2.] {
                let image=batch.raster((n.frame.width as f64*scale).round()as u32,(n.frame.height as f64*scale).round()as u32,scale).unwrap();
                let png=hyprv_ui::capture::qt_png(&image).unwrap();
                std::fs::write(out.join(format!("canvas-{seq}@{}.png",scale as u32)),png).unwrap();
            }
            std::fs::write(out.join(format!("canvas-{seq}.txt")),format!("{batch:#?}")).unwrap();*seq+=1;
            let color=|c:hyprv_ui::animation::Color| serde_json::json!([c.r,c.g,c.b,c.a]);
            let ops:Vec<_>=batch.operations.iter().map(|p|serde_json::json!({
                "path":p.path.elements(),"fill":p.fill.map(color),
                "gradient":p.gradient.map(|g|serde_json::json!({"y0":g.y0,"y1":g.y1,"top":color(g.top),"bottom":color(g.bottom)})),
                "stroke":p.stroke.as_ref().map(|s|serde_json::json!({"width":s.width,"color":color(s.color),"cap":format!("{:?}",s.cap),"join":format!("{:?}",s.join)}))
            })).collect();
            std::fs::write(out.join(format!("canvas-{}.json",*seq-1)),serde_json::to_vec_pretty(&serde_json::json!({"width":n.frame.width,"height":n.frame.height,"operations":ops})).unwrap()).unwrap();
        }
        for c in &n.children {walk(c,x,y,lines,seq,out);}
    }
    walk(&scene,0.,0.,&mut lines,&mut seq,&out);
    std::fs::write(out.join("nodes.txt"),lines.join("\n")).unwrap();
    if std::env::var_os("CORE_STATS_RENDER").is_some() {
        let image=hyprv_ui::offscreen::render_image_if_enabled(&mut hyprv_ui::render::Renderer::new(),&hyprv_ui::scene::Scene::new(scene),3072.,1728.,1.).unwrap().unwrap();
        std::fs::write(out.join("full.png"),hyprv_ui::capture::qt_png(&image).unwrap()).unwrap();
    }
}
