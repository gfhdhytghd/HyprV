mod common;
/// Explicit opt-in because this compares independently captured QML assets and
/// applies the per-image premultiplied acceptance rules in ARCHITECTURE.md.
#[test]
#[ignore = "run with --ignored after generating matching QML references"]
fn qml_bar_references_via_gate_adapter() {
    let root = common::crate_root();
    let status = std::process::Command::new("python3")
        .arg(root.join("scripts/parity.py"))
        .status()
        .expect("run QML parity adapter");
    assert!(
        status.success(),
        "QML parity differs; inspect artifacts/parity/gate/gate.md"
    );
}
