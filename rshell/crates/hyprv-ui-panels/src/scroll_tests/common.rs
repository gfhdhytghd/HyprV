use serde_json::Value;
use std::path::PathBuf;
fn base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/panels-flickable")
}
fn sequences(mode: &str) -> Vec<String> {
    let manifest: Vec<(String, String)> =
        serde_json::from_str(&std::fs::read_to_string(base().join("manifest.json")).unwrap())
            .unwrap();
    manifest
        .into_iter()
        .filter(|(m, _)| m == mode)
        .map(|(_, name)| name)
        .collect()
}
fn sequence(mode: &str, name: &str) -> (Vec<Value>, String) {
    (
        serde_json::from_str(
            &std::fs::read_to_string(base().join(format!("{mode}-{name}.json"))).unwrap(),
        )
        .unwrap(),
        std::fs::read_to_string(base().join(format!("{mode}-{name}.csv"))).unwrap(),
    )
}
fn n(e: &Value, key: &str) -> f64 {
    e[key].as_f64().unwrap_or(0.)
}
fn pointer(e: &Value) -> Pointer {
    let x = e["x"].as_f64().unwrap_or(100.) as f32;
    let y = n(e, "y") as f32;
    match e["type"].as_str().unwrap() {
        "press" => Pointer::Press {
            x,
            y,
            button: Button::Left,
        },
        "move" => Pointer::Motion { x, y },
        "release" => Pointer::Release {
            x,
            y,
            button: Button::Left,
        },
        "wheel" => Pointer::Wheel {
            x: 100.,
            y: 200.,
            angle_x: n(e, "ax") as f32,
            angle_y: n(e, "ay") as f32,
            pixel_x: n(e, "px") as f32,
            pixel_y: n(e, "py") as f32,
        },
        _ => panic!("not a pointer"),
    }
}
fn samples(text: &str) -> impl Iterator<Item = (u64, f64, f64, usize)> + '_ {
    text.lines().skip(1).map(|l| {
        let v: Vec<_> = l.split(',').collect();
        (
            v[0].parse().unwrap(),
            v[1].parse().unwrap(),
            v[2].parse().unwrap(),
            v[3].parse().unwrap(),
        )
    })
}
static MAX_ERROR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn report(mode: &str, traces: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    eprintln!(
        "{mode}: {traces} Qt traces, {} coordinate checks, max error {:.12} logical px",
        CHECKS.load(Relaxed),
        f64::from_bits(MAX_ERROR.load(Relaxed))
    );
}
fn check(name: &str, time: u64, expected: f64, actual: f64) {
    use std::sync::atomic::Ordering::Relaxed;
    CHECKS.fetch_add(1, Relaxed);
    MAX_ERROR.fetch_max((expected - actual).abs().to_bits(), Relaxed);
    assert!(
        (expected - actual).abs() < 0.0001,
        "{name}@{time}: Qt={expected}, Rust={actual}, error={}",
        (expected - actual).abs()
    );
}

#[allow(dead_code)] // Shared by native-wheel and custom-wheel panel tests.
fn native_wheel(e: &Value, horizontal: bool) -> hyprv_widgets::flickable::Wheel {
    use hyprv_widgets::flickable::{ScrollPhase, Wheel};
    Wheel {
        angle_delta: n(e, if horizontal { "ax" } else { "ay" }),
        pixel_delta: n(e, if horizontal { "px" } else { "py" }),
        synthesized: e["pixel"].as_bool().unwrap_or(false),
        phase: match n(e, "phase") as u8 {
            1 => ScrollPhase::Begin,
            2 => ScrollPhase::Update,
            3 => ScrollPhase::End,
            4 => ScrollPhase::Momentum,
            _ => ScrollPhase::None,
        },
    }
}
