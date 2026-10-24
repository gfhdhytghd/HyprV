# Adversarial re-review: shell icon assets

**Implementation follow-up:** all findings in this report are fixed locally; see [fixes and regression evidence](../verify/discovery-icons-r2-fixes.md). Original review evidence below describes the pre-fix checkpoint.

Reviewed 2026-10-23. [Method and reproduction](scratch/discovery-icons-reconcile/README.md), [actual assets-module probe result](scratch/discovery-icons-reconcile/assets-results.json), [source fingerprints](scratch/discovery-icons-reconcile/source-hashes.json). No live services, user files or production crates were changed.

<a id="shell-r2-1"></a>
## SHELL-R2-1 [P1] Tray icon filenames trigger unbounded reads and copies on the UI thread

**Location:** `crates/hyprv-shell/src/assets.rs:127`, `assets.rs:51`, `assets.rs:140`; call site `crates/hyprv-shell/src/assembly.rs:288`.

**Failure scenario:** a StatusNotifier item supplies an IconName absolute filename, or a relative filename resolved through IconThemePath/theme lookup, pointing to a very large regular file. icon_file synchronously reads the entire file with no byte limit, then decode copies all bytes into Skia Data; failed image decoding also tries the SVG loader. This happens during bar asset construction, before any rejection based on content. A tiny icon request therefore permits large allocation/copy work and stalls the UI thread. Valid images also have no decoded-pixel budget before the full source buffer allocation. The existing 4096×4096 restriction applies only to D-Bus IconPixmap arrays, not file icons.

**Verification:** `probe_assets.py` compiles a wrapper including the actual assets.rs and calls its public tray function using a private 128 MiB sparse all-zero file. Peak RSS measured with wait4 is **407912 KiB**, versus **20044 KiB** for a missing-icon control in the same executable. Its expected bounded-resource assertion fails. The file is not a valid image and is ultimately rejected, proving the amplification precedes successful image decoding. Probe binary and sparse file are removed in finally. No OOM kill or decompression bomb is claimed as reproduced.

**Suggested fix:** apply a bounded read from an opened regular-file handle, with a hard encoded-byte ceiling (for example the existing art_bytes 16 MiB ceiling), rather than trusting a prior pathname metadata check. Check dimensions with checked multiplication and a decoded-pixel budget before allocation/decoding. Move file I/O and expensive preparation off the UI thread where feasible while preserving the architecture's typed/byte transfer boundary. Retain intentional absolute-icon support; rejecting only `..` would not address this bug. Test oversized regular files, changed/truncated files and valid small icons.

## Other coverage

Theme-relative traversal is confirmed but is not separately classified as a privilege-boundary defect because absolute filenames are explicitly supported; see [widget review](hyprv-widgets-r2.md#path-handling-observation). Malformed theme caches introduce a separate UI hang and fallback error reported to widgets. The combined host/reconcile ordinary tests passed; no additional host lifecycle defect was verified in this checkpoint. A concurrent tray-sizing change has the separate cache defect below.

<a id="shell-r2-2"></a>
## SHELL-R2-2 [P2] Tray image cache omits DPR although themed raster dimensions depend on it

**Location:** `crates/hyprv-shell/src/assets.rs:68`, `assets.rs:71`, `assets.rs:73`, and `assets.rs:114`.

**Failure scenario:** two output scales round to the same provider request size, so tray uses the same `(item.key, request)` cache entry and same_tray_icon returns true. However, themed_icon now rasterizes at `round(request * dpr)` and chooses theme entries using `ceil(dpr)`. Those values can differ for that same key. After a fractional-scale output change, or when rendering the same tray item for two differently scaled outputs on the UI thread, the second output reuses a raster at the wrong resolution or from the wrong theme scale directory. The cache bypasses all later DPR-dependent lookup work.

**Verification:** [probe_dpr.py](scratch/discovery-icons-reconcile/probe_dpr.py) includes the actual updated assets.rs and uses a private simple SVG. Same item at DPR 1.25 produces 29×29. Reusing it at DPR 1.30 still produces 29×29; changing only item.key to force a fresh lookup at DPR 1.30 produces 30×30. The expected cached/fresh dimension-equivalence assertion fails; [receipt](scratch/discovery-icons-reconcile/dpr-results.json). Both DPR values round the provider request to 23, demonstrating the cache collision. This was rerun against the concurrent tray-cache/sizing update, not the older assets implementation.

**Suggested fix:** include all size-selection/rasterization inputs in the cache key: provider request, actual physical raster target and theme scale (or validated DPR bits). Keep item icon-content identity as a separate comparison. Add same-item cross-output/fractional-scale transitions and compare cached and fresh images, including a transition across ceil(DPR) theme-directory selection.
