//! Independent original-QML observer values are in artifacts/observe-music-expand-collapse.
use super::*;
fn ms(t:u64)->Duration { Duration::from_millis(t) }
#[test]
fn moving_track_behavior_uses_last_presented_width() {
    let mut i=Island::new(Inputs {now_hh_mm:"09:41".into(),media:crate::Media {available:true,playing:true,length_s:225.,position_s:83.,..Default::default()},spectrum:vec![50.;16],..Default::default()},640.,ms(0));
    i.frame(ms(1000));
    let origin=i.origin();
    i.pointer(Pointer::Motion{x:origin.0+6.,y:origin.1+19.},ms(1000));
    assert!(i.derived().music_expanded);
    for t in [16,30,46,63,70,86,103,105,121,138,140,156,173,190,206,210] {
        i.frame(ms(1000+t));
        assert!((i.a.fill_w.get(ms(1000+t))-34.675555555555555).abs()<1e-9,"t={t}");
    }
    i.frame(ms(1400));
    assert!((i.a.fill_w.get(ms(1400))-135.01333333333332).abs()<1e-9);
}
#[test]
fn wave_phase_uses_js_double_arithmetic_and_qt_clock() {
    let mut i=Island::new(Inputs {media:crate::Media {available:true,playing:false,title:"Song".into(),player:"spotify".into(),..Default::default()},..Default::default()},640.,ms(0));
    i.frame(ms(1500));i.frame(ms(1516));
    assert_eq!(i.waves[0].phase,6.160000000000004);
    i.frame(ms(1582));
    assert_eq!(i.waves[0].phase,6.440000000000004);
}
#[test]
fn radius_binding_advances_once_then_restarts_from_presented_value() {
    let mut i=Island::new(Inputs {
        now_hh_mm:"09:41".into(),
        media:crate::Media {available:true,playing:true,title:"Midnight City".into(),player:"spotify".into(),..Default::default()},
        agents:vec![crate::Agent{id:"a".into(),source:"codex".into(),status:"running".into(),..Default::default()}],
        ..Default::default()
    },640.,ms(0));
    i.frame(ms(1000));
    let origin=i.origin();
    i.pointer(Pointer::Motion{x:origin.0+118.,y:origin.1+19.},ms(1000));
    assert!(i.derived().music_expanded);
    i.frame(ms(1400));
    i.pointer(Pointer::Motion{x:2.,y:2.},ms(1400));
    assert!(!i.derived().music_expanded);
    assert_eq!(i.a.mef_r.get(ms(1400)),30.);
    // Original QML trace: the first tick advances the pre-existing job;
    // subsequent height changes restart it before it advances again.
    for t in [17,34,50,60] {
        i.frame(ms(1400+t));
        assert!((i.a.mef_r.get(ms(1400+t))-42.976_211_424_252_25).abs()<1e-10,"t={t} got={} height={}",i.a.mef_r.get(ms(1400+t)),i.a.height.get(ms(1400+t)));
    }
}
#[test]
fn polish_hover_can_end_before_its_scale_job_advances() {
    let mut scale=crate::util::FrameBehavior::new();
    // Original QML interval0 trace: enters during the16ms polish, leaves
    // during30ms polish, and scale remains1 at30,60,105ms captures.
    scale.request(1.06,ms(16),true);
    assert_eq!(scale.value(ms(16)),1.);
    scale.begin_frame(ms(30));
    scale.request(1.,ms(30),true);
    for t in [46,60,76,93,105] {scale.begin_frame(ms(t));assert_eq!(scale.value(ms(t)),1.);}
    // A real input event is processed before the next driver tick.
    scale.request(0.92,ms(200),false);
    assert!((scale.value(ms(230))-0.95375).abs()<1e-12);
}
#[test]
fn anchor_offset_is_added_before_the_qreal_to_matrix_conversion() {
    let mut island=Island::new(Inputs::default(),640.,ms(0));
    island.a.width.snap(127.68376457725947);
    island.a.offset.snap(-1.27113702623907);
    // Independent original-QML agent-appear60 observation. Rounding each
    // intermediate f32 addition gives254.72885 instead of254.72887.
    assert_eq!(island.origin().0,254.72886297376093_f64 as f32);
}
