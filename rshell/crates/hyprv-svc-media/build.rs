fn main() {
    println!("cargo:rerun-if-changed=vendor/cavacore/cavacore.c");
    println!("cargo:rerun-if-changed=vendor/cavacore/cavacore.h");
    println!("cargo:rerun-if-changed=csrc/cava_shim.c");
    cc::Build::new()
        .file("vendor/cavacore/cavacore.c")
        .file("csrc/cava_shim.c")
        .include("vendor/cavacore")
        .flag("-std=c99")
        // Same flags as cava (Makefile.am -std=c99; distro -O2): ISO mode keeps fp-contract off.
        .opt_level(2)
        .warnings(false)
        .compile("hyprv_cavacore");
    println!("cargo:rustc-link-lib=fftw3");
    println!("cargo:rustc-link-lib=m");
}
