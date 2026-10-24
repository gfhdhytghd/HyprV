#[path = "../tests/common/mod.rs"]
mod common;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| common::crate_root().join("artifacts/actual/bar"));
    let count = common::render_all(&output)?;
    println!("Rendered {count} PNGs to {}", output.display());
    Ok(())
}
