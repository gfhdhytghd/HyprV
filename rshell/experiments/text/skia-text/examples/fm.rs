use skia_safe as sk;
fn main() {
    let fm = sk::FontMgr::new();
    for (fam, w) in [("JetBrainsMono Nerd Font", 700), ("JetBrainsMono Nerd Font", 600), ("HarmonyOS Sans SC", 400), ("HarmonyOS Sans SC", 600), ("HarmonyOS Sans SC", 700), ("Inter", 400), ("Inter", 700), ("sans-serif", 400)] {
        let st = sk::FontStyle::new(sk::font_style::Weight::from(w), sk::font_style::Width::NORMAL, sk::font_style::Slant::Upright);
        let tf = fm.match_family_style(fam, st).unwrap();
        let (data, idx) = tf.to_font_data().unwrap();
        println!("{fam} {w}: family={} bytes={} idx={} var={:?} weight={:?}", tf.family_name(), data.len(), idx,
            tf.variation_design_position().map(|p| p.iter().map(|c| c.value).collect::<Vec<_>>()), tf.font_style().weight());
    }
}
