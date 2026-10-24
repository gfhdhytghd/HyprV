# Combined renderer review: hyprv-ui, round 2

Reviewed 2026-10-23. **Six verified findings** follow. This review covers the combined mesh, text, MultiEffect, image/mipmap, Canvas and rotation implementation, including the CPU blending-mode change that landed during review. Production crates were not edited. All executable probes and instrumentation live in [scratch/hyprv-ui-r2](scratch/hyprv-ui-r2/).

The source was refreshed during review to include concurrent changes; the final audited files are pinned by [source-hashes.json](scratch/hyprv-ui-r2/source-hashes.json). Locations below refer to that snapshot. Reproduce with `python3 docs/reviews/scratch/hyprv-ui-r2/reproduce.py`: it builds the frozen copy and requires **six deliberately failing regression assertions plus three passing controls on each of NVIDIA EGL and Mesa llvmpipe**, and **31 passing existing library tests**. See [results.json](scratch/hyprv-ui-r2/results.json), [NVIDIA log](scratch/hyprv-ui-r2/nvidia-probes.log), and [Mesa log](scratch/hyprv-ui-r2/mesa-probes.log). These are standalone surfaceless EGL contexts; no live Wayland display, service actions, root execution or installation was used. They exercise production renderer code, not full Wayland presentation or Qt screenshot parity.

<a id="ui-r2-1"></a>
## UI-R2-1 [P2] Direct image upload rejects valid GPU-backed images and stops rendering

**Location:** `crates/hyprv-ui/src/native_gl.rs:178` (`sampled_image` eligibility), `native_gl.rs:409` (context-free `Image::read_pixels`); fatal propagation at `crates/hyprv-ui/src/wayland.rs:926`.

**Failure scenario:** A scene contains an opted-in QtGl NativeRendering text leaf, enabling the shared native target for the scene, and a valid `Content::Image` obtained from a Skia GPU surface snapshot. The positive axis-aligned image passes native eligibility. Upload then asks the GPU image for CPU pixels without its DirectContext. This fails, producing `NativeGl("native image pixels")`. The image renderer propagates the error instead of trying its Skia fallback; the Wayland host sets `running = false`. An otherwise supported image can therefore terminate shell rendering merely because another leaf selects the native pipeline. Rotating the image or applying a blocking clip changes eligibility and can make the same source work, creating conflicting behavior between renderer paths.

**Verification:** `review_r2_gpu_image_acceptance` creates an actual GPU-backed red snapshot and a QtGl text sibling, asserts that production `native_gl_required` selects the path, and successfully renders with only `HYPRV_IMAGE_GL_DISABLE` set. Removing that override causes the expected render-error assertion to fail on both drivers.

**Suggested fix:** Return unsupported before upload for GPU-backed images and let Skia sample them, or implement texture sharing/context-aware readback with explicit origin handling. Unsupported source representation must return `Ok(false)`, not a fatal renderer error. Preserve a regression containing both the text trigger and GPU-backed image.

<a id="ui-r2-2"></a>
## UI-R2-2 [P2] Changing CPU blending mode reuses effects captured with the old mode

**Location:** `crates/hyprv-ui/src/render.rs:140` (`set_gpu_exact_blending`), `render.rs:358` (effect cache eligibility).

**Failure scenario:** Render a translucent MultiEffect subtree with the default GPU-exact CPU blending, then call `set_gpu_exact_blending(false)` and repaint the same scene. The setter changes the blending flag but neither invalidates effects nor changes their cache key. The captured image keeps the old compositing results while uncached nodes use the new mode. Identical scenes and renderer settings consequently produce different parity images depending on render history. This finding concerns CPU/parity rendering; the flag is deliberately inactive on GPU canvases.

**Verification:** `review_r2_blending_mode_invalidation` composites eight translucent Skia rectangles inside a default MultiEffect. A fresh renderer using the requested mode differs in **1,568 channel bytes** from the warmed renderer after the switch. The control first establishes that the two requested modes produce different pixels, avoiding a vacuous cache assertion.

**Suggested fix:** On an actual mode change, invalidate retained effect images, or include the capture/blending configuration in their cache key. Require warmed-after-switch output to equal a fresh renderer in both directions.

<a id="ui-r2-3"></a>
## UI-R2-3 [P2] The renderer cache-flush API cannot release its new EGL-owned caches

**Location:** `crates/hyprv-ui/src/render.rs:144` (`clear_caches`), `crates/hyprv-ui/src/egl.rs:31` (separate native context ownership), `crates/hyprv-ui/src/qt_gl_rect.rs:121` (mesh-buffer LRU), `crates/hyprv-ui/src/native_gl.rs:499` (native destruction).

**Failure scenario:** Animate rounded-rectangle dimensions, close the component, and explicitly clear renderer caches—for example through the existing rectangle-mode change. CPU mesh references are cleared, but native `Context::rectangles` retains a second mesh/VBO cache, and `Context::textures` retains its texture/source images. These caches belong to SharedEgl and are inaccessible to `Renderer::clear_caches`, despite that API's promise to release cached GPU resources. A long-lived shell keeps the inactive resources until LRU pressure or EGL teardown. Clearing caches also forces new CPU meshes to get new pointer identities, so drawing identical geometry again can temporarily add duplicate VBO entries.

**Verification:** `review_r2_clear_gpu_retention` draws 300 distinct rounded-rectangle widths and one image, then calls `Renderer::clear_caches`. Before and after it observes exactly **300 VBOs / 2,340,000 bytes**, plus **16,384 bytes** of native texture storage. VBO sizes come from actual GL `BUFFER_SIZE` queries. Both drivers reproduce the unchanged counts.

**Suggested fix:** Give the EGL/context owner an explicit coordinated resource-release operation, invoked while its context is current, and use it together with renderer cache invalidation. Alternatively split and clearly name CPU-only invalidation versus complete GPU release. Avoid depending on a currently active paint guard for flushing between frames. Give the mesh cache a byte budget and expose per-cache retained-byte diagnostics. `Context::drop` already deletes these resources: this is a broken flush/lifetime boundary and bounded retention, **not proof of a teardown leak or unbounded growth**.

<a id="ui-r2-4"></a>
## UI-R2-4 [P2] Mipmap allocation bypasses the native texture-cache byte budget

**Location:** `crates/hyprv-ui/src/native_gl.rs:413` (eviction), `native_gl.rs:439` (base-only accounting), `native_gl.rs:459` (`generate_mipmap`).

**Failure scenario:** Two 1024×1024 image leaves request mipmapping. The native cache counts their RGBA base levels as exactly 8 MiB and retains both. Generating the remaining mip levels does not update either the aggregate charge or each texture's eviction charge. The advertised 8 MiB threshold therefore admits at least **10.67 MiB** of logical texel storage for these two textures, before driver overhead. Switching a cached image from Linear to Mipmap has the same accounting gap. The excess stays allocated when subsequently sampling Linear; the chain still exists.

**Verification:** `review_r2_mip_budget` renders both actual mipmapped textures without GL errors, then reads the implementation's charge and counts the full RGBA8 chains from their allocated base extents: **8,388,608 charged versus 11,184,808 texel bytes**. The deliberately failing assertion requires the complete chain to fit the budget. This measures logical texture storage, not driver allocation granularity or NVML totals.

**Suggested fix:** Budget the complete chain before creating or upgrading a texture, update its stored charge, and evict other entries before allocation. Retain that charge if filtering later stops using the existing chain. Test both initially mipmapped and Linear→Mipmap transitions, including narrow/non-square images.

<a id="ui-r2-5"></a>
## UI-R2-5 [P2] Live Shared-subtree effects are evicted on every unrelated scene edit

**Location:** `crates/hyprv-ui/src/render.rs:227` (`prune::walk`), `render.rs:235` (`effect_cache.retain`); compare Shared rendering at `render.rs:460`.

**Failure scenario:** An immutable `Content::Shared(Rc<Node>)` contains an expensive effect while an ordinary sibling animates position. Rendering and `native_gl_required` descend into Shared content, but cache-pruning traversal only follows `children`. After each sibling edit it treats the Shared effect's still-live NodeId as dead and evicts its cached output and pyramid. The next draw reconstructs the unchanged source, blur levels and composition. A stationary shared popup can thus incur full effect preparation on every frame of an unrelated animation. The bounds-cache pruning traversal has the same reachability mismatch once its threshold is reached.

**Verification:** `review_r2_shared_effect_survives_unrelated_edit` renders a Shared effect, confirms its cache entry exists, moves only its sibling, and invokes the normal prune operation. The still-live entry is absent. The immutable effect's revision and scale have not changed.

**Suggested fix:** Centralize traversal of both `children` and `Content::Shared`, with a visited NodeId set for shared DAGs, and use it consistently for live caches and native eligibility. Add a regression that checks actual effect reuse across unrelated scene edits, not just equal output pixels.

<a id="ui-r2-6"></a>
## UI-R2-6 [P3] Text clear leaves distance-field raster masks retained

**Location:** `crates/hyprv-ui/src/text.rs:433` (`df_masks`), `text.rs:713` (`TextEngine::clear`).

**Failure scenario:** QtRendering text generates device-placement-specific distance-field masks during scrolling, scaling or rotation changes. Calling `TextEngine::clear`, including through the renderer's general cache clear, empties lines, native masks and distance fields but omits `df_masks`. Those raster images remain referenced across a requested cache flush. Their existing count cap prevents unlimited growth, but the cleanup operation does not release the most placement-sensitive text cache and leaves up to its high-water allocation resident.

**Verification:** `review_r2_text_clear_releases_df_masks` paints `CACHE`, confirms masks were generated, calls clear, and still sees **five entries**. This is CPU raster-image retention; it does not establish that these masks caused the soak's GPU increase.

**Suggested fix:** Clear `df_masks` with the other text caches, and include all future text-cache fields in one explicit cache-owner structure or tested reset operation. Check the cache is empty after a warmed render, not only on a newly constructed engine.

## Cross-path checks and duplication

The three passing probes on both drivers are meaningful negative evidence:

- Alternating direct-GL rectangles/text with Skia rectangles/text matches the all-Skia control exactly in the tested scene (maximum channel delta zero). No GL-state corruption was reproduced there.
- Eight warmed-renderer scale/rotation/theme transitions match fresh-renderer output exactly. This checks retained effect scale keys and ordinary edit-driven invalidation, not every transform/clip/source combination.
- An asymmetric source with a downward shadow matches between the specialized Qt shadow program and generic composition (zero pixels differing by more than 2/255). The suspected texture-origin mismatch was rejected.

There are still useful consolidation opportunities, separate from the verified defects: `Programs::apply_prepared` always produces a fully composed output image while also retaining its source/pyramid in `NativeEffect`; `Renderer::node` immediately composes the prepared textures again when `effect_at` succeeds (`effects.rs:339`–`effects.rs:342`, `render.rs:389`–`render.rs:402`). The cached image is needed for fallback transforms/layers, but a lazy fallback image could avoid the extra cold-frame composition and retained output texture for effects that remain on the direct path. This is an optimization tradeoff, not an additional correctness finding. Likewise, CPU sampling, Skia sampling and direct texture sampling need distinct backend implementations, but should share eligibility/configuration and cache accounting contracts rather than silently disagree about supported image representations.

The reviewed ownership is UI-thread confined: SharedEgl uses `Rc`/`RefCell`, retained nodes can contain `Rc`, and native target/FreeType/image caches are thread-local. No added unsafe Send/Sync implementation or newly reproduced thread race was found. That is a source/targeted-probe conclusion, not a concurrent-runtime stress proof. Existing frame-driven animation and one-shot deadline scheduling were not replaced by polling timers in these renderer changes.

## Relation to the 5 MiB soak observation

[The 62-minute soak](../verify/soak.md) recorded a graphics-memory plateau from 56 to 61 MiB, not continuing monotonic growth. This review independently demonstrates persistent native mesh/texture caches, incomplete explicit cache release, and uncharged mip levels. Those are plausible contributors and now have actionable probes, but **the original 5 MiB cannot be causally attributed to any one of them**: the soak pinned an earlier executable, source changed concurrently, and NVML includes driver/Skia high-water allocation. Current native destruction releases its owned objects; this review does not establish an unbounded GPU leak. A follow-up should tag live GL objects and Skia cache bytes in the same pinned binary throughout repeated identical warm/close/flush cycles, and distinguish live resources from allocator/driver reservations.
