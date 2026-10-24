//! Replays every `parity/fixtures/island` fixture (light/dark, @1/@2) and diffs it
//! against `parity/reference/island`. Writes mine/heat PNGs and `report.tsv` to
//! `$HYPRV_ISLAND_PARITY_OUT` (default `<target tmp>/island-parity`).
mod common;
use common::*;

/// Mean premultiplied channel delta (0..255) and fraction of pixels with delta > 48.
const MAX_MEAN: f64 = 2.0;
const MAX_BAD: f64 = 0.02;

#[test]
fn island_matches_qml_references() {
    let out = std::env::var_os("HYPRV_ISLAND_PARITY_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("island-parity"));
    std::fs::create_dir_all(&out).unwrap();
    let filter = std::env::var("ISLAND_FIXTURE").ok();
    let refs = rshell().join("parity/reference/island");
    let mut report = String::from("file\tmean\tbad\tmax\n");
    let mut failures = Vec::new();
    let mut compared = 0;
    for path in fixtures() {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if filter.as_ref().is_some_and(|f| !stem.contains(f.as_str())) {
            continue;
        }
        for dark in [false, true] {
            for scale in [1., 2.] {
                for cap in run_fixture(&path, dark, scale) {
                    let Some(reference) = Img::load(&refs.join(&cap.file)) else {
                        failures.push(format!("{}: no reference", cap.file));
                        continue;
                    };
                    if (reference.w, reference.h) != (cap.img.w, cap.img.h) {
                        failures.push(format!("{}: size {}x{} vs {}x{}", cap.file, cap.img.w, cap.img.h, reference.w, reference.h));
                        continue;
                    }
                    cap.img.save(&out.join(format!("mine-{}", cap.file)));
                    let d = diff(&cap.img, &reference, Some(&out.join(format!("heat-{}", cap.file))));
                    compared += 1;
                    report += &format!("{}\t{:.3}\t{:.4}\t{}\n", cap.file, d.mean, d.bad, d.max);
                    if d.mean > MAX_MEAN || d.bad > MAX_BAD {
                        failures.push(format!("{}: mean {:.3} bad {:.4}", cap.file, d.mean, d.bad));
                    }
                }
            }
        }
    }
    std::fs::write(out.join("report.tsv"), &report).unwrap();
    println!("compared {compared} captures; report: {}", out.join("report.tsv").display());
    assert!(compared > 0);
    assert!(failures.is_empty(), "{} parity failure(s) (report: {}):\n{}", failures.len(), out.display(), failures.join("\n"));
}
