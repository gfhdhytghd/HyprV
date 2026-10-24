mod common;
#[test]
fn every_visual_state_exports_png_at_native_scales() {
    let root = common::crate_root();
    let output = root.join("artifacts/actual/bar");
    let count = common::render_all(&output).expect("all visual fixtures render");
    assert!(count >= 450, "visual coverage unexpectedly shrank: {count}");
    println!("Exported and checked dimensions of {count} PNGs");
}
