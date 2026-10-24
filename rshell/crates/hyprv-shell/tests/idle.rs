//! Always part of `cargo test --workspace --all-targets`; private compositor only.
#[test]
fn quiet_nested_shell_has_no_repaints_or_extra_main_dispatches() {
    let report = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/idle-regression").join(std::process::id().to_string());
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_hyprv-shell"))
        .arg("--nested-idle-test").arg(&report)
        .status().expect("launch private compositor idle regression");
    // The harness pins its ELF against concurrent builds. CI retains the small
    // receipts, not one more frozen debug executable after every test run.
    for entry in std::fs::read_dir(&report).expect("idle report directory") {
        let entry = entry.expect("idle artifact");
        if entry.file_name().to_string_lossy().starts_with("hyprv-shell-test-") {
            std::fs::remove_file(entry.path()).expect("retire pinned idle test executable");
        }
    }
    assert!(status.success(), "see {}", report.display());
}
