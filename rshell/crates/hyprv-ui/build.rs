fn main() {
    println!("cargo:rerun-if-changed=csrc");
    cc::Build::new()
        .cpp(true)
        .file("csrc/distance_field.cpp")
        .file("csrc/path_simplifier.cpp")
        .file("csrc/raster_line.cpp")
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-fwrapv")
        .warnings(false)
        .compile("hyprv_distance_field");
}
