fn main() {
    for (domain, modules) in [
        ("media", &["notifications"][..]),
        (
            "desktop",
            &[
                "viewflow",
                "demo",
                "egpu",
                "recording",
                "theme",
                "applications",
                "startup",
            ][..],
        ),
    ] {
        let path = format!("../hyprv-svc-{domain}/src/lib.rs");
        println!("cargo:rerun-if-changed={path}");
        let source = std::fs::read_to_string(path).unwrap_or_default();
        for module in modules {
            let flag = format!("has_{domain}_{module}");
            println!("cargo:rustc-check-cfg=cfg({flag})");
            if source.contains(&format!("pub mod {module};")) {
                println!("cargo:rustc-cfg={flag}");
            }
        }
    }
}
