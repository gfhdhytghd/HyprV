//! Pixel parity of the panels against the SHARED Qt captures (`parity/reference/panels`,
//! produced by `parity/capture.sh panels` from `parity/fixtures/panels`). Unlike
//! `qml_parity.rs` this never skips: a missing reference, an un-replayable step or a
//! capture the replay did not produce is a failure.
//!
//! Calendar fixtures (bar centre morph: island + calendar) are validated by piece B in
//! `hyprv-ui-island/tests/calendar_morph.rs`; here they are only checked to have
//! references.
//!
//! Output: `$HYPRV_PANELS_PARITY_OUT` (default `<target tmp>/panels-shared-parity`) gets
//! `mine-*.png`, `heat-*.png` and `report.tsv`. `PANELS_FIXTURE=<substr>` filters.
mod shared_common;
use shared_common::*;
use std::collections::BTreeMap;

const MAX_MEAN: f64 = 2.0;
const MAX_BAD: f64 = 0.02;
/// docs/ARCHITECTURE.md parity gate: max channel diff ≤ 8/255, mean over painted pixels
/// ≤ 0.5/255, for every capture including animation frames.
const GATE_MAX: u8 = 8;
const GATE_PAINTED_MEAN: f64 = 0.5;

#[test]
fn panels_match_shared_qml_references() {
    let out = std::env::var_os("HYPRV_PANELS_PARITY_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("panels-shared-parity"));
    std::fs::create_dir_all(&out).unwrap();
    let refs = rshell().join("parity/reference/panels");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(refs.join("manifest.json")).expect("reference manifest")).unwrap();
    // state → expected files
    let mut expected: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for im in manifest["images"].as_array().unwrap() {
        expected.entry(im["state"].as_str().unwrap().into()).or_default().push(im["file"].as_str().unwrap().into());
    }
    let filter = std::env::var("PANELS_FIXTURE").ok();
    let mut report = String::from("file\tresult\tmean\tbad\tmax\tpainted_mean\n");
    let mut failures = Vec::new();
    let mut passed = 0;
    for path in fixtures() {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|f| !stem.contains(f.as_str())) {
            continue;
        }
        let want = expected.get(&stem).cloned().unwrap_or_default();
        if want.is_empty() {
            failures.push(format!("{stem}: no references in manifest"));
            report += &format!("{stem}\tMISSING-REFERENCE\t\t\t\n");
            continue;
        }
        for f in &want {
            if !refs.join(f).exists() {
                failures.push(format!("{f}: reference file missing"));
                report += &format!("{f}\tMISSING-REFERENCE\t\t\t\n");
            }
        }
        if is_calendar_fixture(&stem) {
            for f in &want {
                report += &format!("{f}\tPIECE-B\t\t\t\n");
            }
            continue;
        }
        let mut produced = Vec::new();
        for dark in [false, true] {
            for scale in [1f32, 2.] {
                let (caps, errors) = run_fixture(&path, dark, scale);
                for e in errors {
                    failures.push(e.clone());
                    report += &format!("{stem}[{}@{scale}]\tREPLAY-ERROR {e}\t\t\t\n", if dark { "dark" } else { "light" });
                }
                for cap in caps {
                    produced.push(cap.file.clone());
                    let Some(reference) = Img::load(&refs.join(&cap.file)) else {
                        failures.push(format!("{}: no reference", cap.file));
                        report += &format!("{}\tMISSING-REFERENCE\t\t\t\n", cap.file);
                        continue;
                    };
                    if (reference.w, reference.h) != (cap.img.w, cap.img.h) {
                        failures.push(format!("{}: size {}x{} vs {}x{}", cap.file, cap.img.w, cap.img.h, reference.w, reference.h));
                        report += &format!("{}\tSIZE\t\t\t\n", cap.file);
                        continue;
                    }
                    cap.img.save(&out.join(format!("mine-{}", cap.file)));
                    let d = diff(&cap.img, &reference, Some(&out.join(format!("heat-{}", cap.file))));
                    let ok = d.mean <= MAX_MEAN && d.bad <= MAX_BAD && d.max <= GATE_MAX && d.painted_mean <= GATE_PAINTED_MEAN;
                    report += &format!("{}\t{}\t{:.3}\t{:.4}\t{}\t{:.3}\n", cap.file, if ok { "PASS" } else { "FAIL" }, d.mean, d.bad, d.max, d.painted_mean);
                    if ok {
                        passed += 1;
                    } else {
                        failures.push(format!("{}: mean {:.3} bad {:.4}", cap.file, d.mean, d.bad));
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
