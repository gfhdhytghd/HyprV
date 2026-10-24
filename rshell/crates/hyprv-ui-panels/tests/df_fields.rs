//! Distance-field bytes against Qt's own (experiments/text/df/df-*.json, written by
//! `textref --dumpdf family weight text out.json`).
mod common;
use hyprv_ui::animation::Color;
use hyprv_ui_panels::qtext::{df_fields, Font, TextBox};

#[test]
fn fields_match_qt_bytes() {
    let dir = common::root().join("experiments/text/df");
    let cases: [(&str, &'static str, i32, &str); 6] = [
        ("st", "HarmonyOS Sans SC", 500, "启动用户应用"),
        ("stb", "HarmonyOS Sans SC", 400, "启动常用应用并恢复工作区布局。"),
        ("mh", "sans-serif", 700, "Refresh Dashboard ↗"),
        ("jb", "JetBrainsMono Nerd Font", 700, "Mihomo · rule 47 KB/s"),
        ("cal", "sans-serif", 700, "周四 23"),
        ("cala", "sans-serif", 600, "Week 43 · 十月"),
    ];
    let mut worst_all = 0;
    for (name, family, weight, text) in cases {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("df-{name}.json"))).expect("dump")).unwrap();
        let qt: Vec<_> = v.as_array().unwrap().iter().filter(|g| g["bw"].as_f64().unwrap() > 0.).collect();
        let ours = df_fields(&TextBox::new(text, Font::qt(16., weight).family(family), Color::default()));
        assert_eq!(qt.len(), ours.len(), "{name}: glyph count");
        for (q, (id, w, h, data)) in qt.iter().zip(&ours) {
            assert_eq!(q["glyph"].as_u64().unwrap() as u16, *id, "{name}: glyph id");
            let (qw, qh) = (q["w"].as_u64().unwrap() as usize, q["h"].as_u64().unwrap() as usize);
            assert_eq!((qw, qh), (*w, *h), "{name} glyph {id}: size");
            let qd: Vec<u8> = q["data"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect();
            let (mut worst, mut n) = (0i32, 0usize);
            for (a, b) in qd.iter().zip(data) {
                let d = (*a as i32 - *b as i32).abs();
                worst = worst.max(d);
                if d > 0 {
                    n += 1;
                }
            }
            if worst > 0 {
                eprintln!("{name} glyph {id}: {n} bytes differ, max {worst}");
            }
            worst_all = worst_all.max(worst);
        }
    }
    assert_eq!(worst_all, 0, "field bytes differ from Qt");
}
