//! Laptop QuickAdjust parity (docs/spec/laptop-quickadjust.md, spec-bar) against the laptop
//! Qt captures (`parity/laptop/reference/laptop-panels`, `origin/laptop@24972f7`, eDP-1
//! 1440×900). The references are whole-shell frames, so only the 278×62 card region is
//! compared: x 1152 (right-aligned to the tray pill), y = the bar exclusive zone latched at
//! map (25 compact, 58 full). In compact mode the top-right corner shade (x ≥ 1413, y 15–42)
//! overlaps the card's corner and belongs to the bar, so the region stops at x 1412.
//!
//! Output: `$HYPRV_PANELS_LAPTOP_QA_OUT` (default `<target tmp>/panels-laptop-qa`).
mod shared_common;
use shared_common::*;
use std::collections::BTreeMap;

/// Card rect plus a 2 px margin (logical) for a card mapped at `y`. While the bar expands
/// over a card frozen at y 25 (`lap-qa-toggle-while-shown`), only its part below the bar
/// pills (y ≥ 49) is the card's own.
fn region(stem: &str, y: f32) -> [f32; 4] {
    if stem == "lap-qa-toggle-while-shown" {
        [1150., 49., 262., 40.]
    } else if y < 58. {
        [1150., y - 2., 262., 66.]
    } else {
        [1150., y - 2., 282., 66.]
    }
}
/// QUIRK_QA_POSITION_FROZEN_WHILE_MAPPED: the y each fixture's card was mapped at.
fn mapped_y(stem: &str) -> f32 {
    match stem {
        "lap-qa-brightness-37" | "lap-qa-step" => 58.,
        _ => 25.,
    }
}
/// docs/ARCHITECTURE.md parity gate (same as `shared_parity.rs`).
const MAX_MEAN: f64 = 2.0;
const MAX_BAD: f64 = 0.02;
const GATE_MAX: u8 = 8;
const GATE_PAINTED_MEAN: f64 = 0.5;

fn crop(img: &Img, scale: f32, region: [f32; 4]) -> Img {
    let k = scale as f64;
    let [x, y, w, h] = region.map(|v| (v as f64 * k).round() as usize);
    let mut px = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        px.extend_from_slice(&img.px[(row * img.w + x) * 4..(row * img.w + x + w) * 4]);
    }
    Img { w, h, px }
}

#[test]
fn laptop_quick_adjust_matches_laptop_references() {
    let out = std::env::var_os("HYPRV_PANELS_LAPTOP_QA_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("panels-laptop-qa"));
    std::fs::create_dir_all(&out).unwrap();
    let refs = rshell().join("parity/laptop/reference/laptop-panels");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(refs.join("manifest.json")).expect("laptop manifest")).unwrap();
    let mut expected: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for im in manifest["images"].as_array().unwrap() {
        expected.entry(im["state"].as_str().unwrap().into()).or_default().push(im["file"].as_str().unwrap().into());
    }
    let filter = std::env::var("PANELS_FIXTURE").ok();
    let mut report = String::from("file\tresult\tmean\tbad\tmax\tpainted_mean\n");
    let mut failures = Vec::new();
    let mut passed = 0;
    for path in fixtures_in("parity/laptop/fixtures/laptop-panels") {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if !stem.starts_with("lap-qa-") || filter.as_ref().is_some_and(|f| !stem.contains(f.as_str())) {
            continue;
        }
        let want = expected.get(&stem).cloned().unwrap_or_default();
        if want.is_empty() {
            failures.push(format!("{stem}: no references in manifest"));
            continue;
        }
        let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut produced = Vec::new();
        for dark in [false, true] {
            for scale in [1f32, 2.] {
                let mut r = Runner::new(&path, dark, scale);
                assert!(r.laptop, "{stem}: not a laptop fixture");
                let region = region(&stem, mapped_y(&stem));
                r.crop = region;
                r.run_steps(&fixture["steps"]);
                for e in &r.errors {
                    failures.push(e.clone());
                    report += &format!("{stem}[{}@{scale}]\tREPLAY-ERROR {e}\t\t\t\n", if dark { "dark" } else { "light" });
                }
                for cap in r.captures {
                    produced.push(cap.file.clone());
                    let Some(full) = Img::load(&refs.join(&cap.file)) else {
                        failures.push(format!("{}: no reference", cap.file));
                        report += &format!("{}\tMISSING-REFERENCE\t\t\t\n", cap.file);
                        continue;
                    };
                    let reference = crop(&full, scale, region);
                    if (reference.w, reference.h) != (cap.img.w, cap.img.h) {
                        failures.push(format!("{}: size {}x{} vs {}x{}", cap.file, cap.img.w, cap.img.h, reference.w, reference.h));
                        continue;
                    }
                    cap.img.save(&out.join(format!("mine-{}", cap.file)));
                    reference.save(&out.join(format!("ref-{}", cap.file)));
                    let d = diff(&cap.img, &reference, Some(&out.join(format!("heat-{}", cap.file))));
                    let ok = d.mean <= MAX_MEAN && d.bad <= MAX_BAD && d.max <= GATE_MAX && d.painted_mean <= GATE_PAINTED_MEAN;
                    report += &format!("{}\t{}\t{:.3}\t{:.4}\t{}\t{:.3}\n", cap.file, if ok { "PASS" } else { "FAIL" }, d.mean, d.bad, d.max, d.painted_mean);
                    if ok {
                        passed += 1;
                    } else {
                        failures.push(format!("{}: mean {:.3} bad {:.4} max {}", cap.file, d.mean, d.bad, d.max));
                    }
                }
            }
        }
        for f in &want {
            if !produced.contains(f) {
                failures.push(format!("{f}: not produced by the replay"));
                report += &format!("{f}\tNOT-PRODUCED\t\t\t\n");
            }
        }
    }
    std::fs::write(out.join("report.tsv"), &report).unwrap();
    println!("{passed} passed; report: {}", out.join("report.tsv").display());
    assert!(failures.is_empty(), "{} failure(s) (report {}):\n{}", failures.len(), out.display(), failures.join("\n"));
}
