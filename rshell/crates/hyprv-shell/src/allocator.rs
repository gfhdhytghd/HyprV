//! Bound retained free libc heap pages without a trim timer or cache invalidation.
//!
//! This executable targets Linux/glibc. Keep allocator policy in the executable,
//! rather than changing process-global settings when a UI library is imported.
const M_ARENA_MAX: i32 = -8;
const M_TRIM_THRESHOLD: i32 = -1;
const M_MMAP_THRESHOLD: i32 = -3;
const TRIM_THRESHOLD: i32 = 2 * 1024 * 1024;

fn explicitly_tuned(variable: &str, tunable: &str) -> bool {
    std::env::var_os(variable).is_some()
        || std::env::var("GLIBC_TUNABLES").is_ok_and(|settings| {
            settings.split(':').any(|entry| {
                entry.split_once('=').is_some_and(|(name, _)| name == tunable)
            })
        })
}

/// Call before constructing service threads, fonts or GL contexts. Freed top-of-
/// heap pages can return to the OS after large temporary allocations. Live font,
/// glyph and render caches are untouched; no malloc_trim runs on a reveal frame.
/// Explicit glibc environment settings retain precedence.
pub fn initialize() {
    unsafe extern "C" { fn mallopt(parameter: i32, value: i32) -> i32; }
    if !explicitly_tuned("MALLOC_ARENA_MAX", "glibc.malloc.arena_max") {
        // SAFETY: glibc malloc.h M_ARENA_MAX; called before our worker threads.
        unsafe { mallopt(M_ARENA_MAX, 2); }
    }
    if !explicitly_tuned("MALLOC_MMAP_THRESHOLD_", "glibc.malloc.mmap_threshold") {
        // SAFETY: restore glibc's initial128KiB boundary even if a linked-library
        // constructor raised the dynamic boundary before Rust main. Large temporary
        // allocations then return through munmap instead of fragmenting an arena.
        unsafe { mallopt(M_MMAP_THRESHOLD, 128 * 1024); }
    }
    if !explicitly_tuned("MALLOC_TRIM_THRESHOLD_", "glibc.malloc.trim_threshold") {
        // SAFETY: positive, conservative glibc malloc.h M_TRIM_THRESHOLD value.
        // Fixing it prevents large startup allocations raising the automatic
        // threshold until tens of MiB of freed memory can remain resident.
        unsafe { mallopt(M_TRIM_THRESHOLD, TRIM_THRESHOLD); }
    }
}

/// Return freed startup/font/material scratch pages before entering the UI loop.
/// Live allocations remain valid; this does not invalidate any rendering cache.
/// Never call this from a frame, a close animation or an idle maintenance timer.
pub fn release_startup_scratch() {
    unsafe extern "C" { fn malloc_trim(pad: usize) -> i32; }
    let _timing = hyprv_ui::diagnostics::scope("allocator.startup_trim");
    // SAFETY: glibc only releases allocator-owned, free pages. No application
    // pointer is passed and no allocation/typeface/GL resource is destroyed.
    unsafe { malloc_trim(0); }
}
