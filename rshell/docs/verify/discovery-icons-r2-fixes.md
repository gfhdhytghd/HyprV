# Discovery and icon review fixes — 2026-10-23

Fixed WIDGETS-R2-1/2, SHELL-R2-1/2 and DESKTOP-R2-1/2. Production changes are confined to the assigned widgets icon-theme reader, shell asset/delivery paths and desktop process-discovery code. Coordination and handoff notes are in the three crate request files. Concurrent QtSvg/tray sizing work was re-read and preserved; no hyprv-ui or hyprv-ui-bar source was edited.

| Finding | Change | Regression evidence |
| --- | --- | --- |
| WIDGETS-R2-1 | Detect collision-chain cycles and stop at 4096 records. Read cache/theme files through a bounded regular-file handle. Validate whole count-table spans, cap counts at 65536 and C strings at 4096 bytes. | Self/two-node cycles; acyclic chain exceeding the work budget; oversized table count. |
| WIDGETS-R2-2 | Invalid nonzero bucket/next offsets return None, enabling directory scanning. Preserve the actual GTK empty sentinel 0xffffffff (and existing zero termination). | Malformed-cache parser checks and a full private-theme lookup finding a real icon despite a bad cache. Existing real-system-cache test passes. |
| SHELL-R2-1 | Live tray file/theme lookup and rasterization run on Tokio's blocking pool. Only dimensions and premultiplied RGBA bytes cross the calloop channel. Open files once with O_NONBLOCK, reject non-regular files, and enforce a read ceiling during reading. | 128 MiB sparse invalid file rejected; FIFO cannot block the reader; oversized decoded dimensions rejected, small valid PNG accepted. Original resource probe repeated below. |
| SHELL-R2-2 | Cache keys include provider request, physical raster target, ceil(DPR) theme scale and pixmap-selection target. | Actual asynchronous 1.25→1.30 transition and fresh-key control both yield 30×30 after the earlier 29×29 image; fixture cache gets the same regression. |
| DESKTOP-R2-1 | Re-stat cached process identities/ppids before deriving ancestry, including non-agent ancestors. Refresh named argv and reject observed stat/argv/pidfd identity inconsistencies. | Reused and reparented ancestors compare equal to a fresh scan and do not report unrelated Codex activity. |
| DESKTOP-R2-2 | Initial and older PIDs are revalidated every leased probe; PID allocation is no longer used as evidence that a cached PID could not exec. | Initial and late exec with unchanged loadavg endpoint; same-name exec changes argv classification. Original equivalence probes now pass. |

Bounds: encoded icon/cache files 16 MiB; index.theme and SVG input 1 MiB; decoded raster sides at most 4096 (at most 16,777,216 pixels); live tray cache 128 entries and a 64 MiB image/ARGB budget; at most two outstanding file jobs. Metadata/path lengths and supplied pixmap bytes are checked before retaining request copies. Image placeholders are shared by bounded request size, avoiding identity churn and redraw loops under saturation. Output/item removal releases obsolete entries and placeholders; pending jobs retain their slots until completion. Late replies cannot replace newer generations. No new polling timer or helper process was added.

The deterministic private fixture path keeps synchronous completion, but cache misses perform I/O on a scoped worker rather than the UI thread. Live Assembly never waits on that fixture loader. The asset test/dump helper and original Qt sizing/material choices remain supported. File-backed album art uses the same bounded regular-file reader off the UI/reactor. The new widgets tray-raster helper also receives bounded encoded reads and dimension validation; it currently has no production call sites.

Discovery retains the new-PID allocation window and 60-second readdir sweep. Cached PID stats and named argv now refresh on every **existing subscriber-leased 1.5-second probe**, on the blocking pool. This deliberately gives up the previous nearly constant-cost cached-identity assumption: correctness across exec/reparent/reuse requires revalidation. No unleased discovery timer is introduced, and no new idle/soak performance result is claimed.

Validation: **182 library tests passed** (shell 46, desktop 72, widgets 64), zero failures; two existing diagnostic dump tests remain ignored. Fourteen new ordinary regressions cover the findings plus ordering, job bounds, stable metadata/image identity, cache saturation and output/item removal. Existing fixture icon expectations and widget reference-image assertions pass. Strict all-targets Clippy passes for all three affected crates.

The original oversized-file probe now measures **19356 KiB peak RSS** versus **19164 KiB** for its missing-icon control; the pre-fix measurement was **407912 KiB** versus **20044 KiB**. The 128 MiB file and probe executables are removed immediately. This establishes rejection before unbounded read/copy for that input; it is not a new whole-shell memory soak.

Reproduction from rshell:

```sh
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_INCREMENTAL=0
cargo test -p hyprv-widgets -p hyprv-shell -p hyprv-svc-desktop --lib
cargo clippy -p hyprv-widgets -p hyprv-shell -p hyprv-svc-desktop --all-targets -- -D warnings
python3 docs/reviews/scratch/discovery-icons-reconcile/probe_assets_fixed.py
python3 docs/reviews/scratch/discovery-icons-reconcile/probe_processes_fixed.py
```

[Results and source hashes](../reviews/scratch/discovery-icons-reconcile/fix-results.json), [test log](../reviews/scratch/discovery-icons-reconcile/fix-tests-complete.log), [Clippy log](../reviews/scratch/discovery-icons-reconcile/fix-clippy-complete.log), [resource receipt](../reviews/scratch/discovery-icons-reconcile/assets-fixed-results.json), [process equivalence receipt](../reviews/scratch/discovery-icons-reconcile/process-fixed-results.json).

Earlier failed checkpoints are retained: a temporary fixture API integration failure, the initially overlooked GTK empty sentinel noted by engine, an unrelated concurrent effects.rs E0499 fixed by its owner, and an unavailable offline libz-sys index entry. Final normal online Cargo checks pass; no dependency-cache permissions or unrelated engine source were changed. No full parity recapture, installation, live-session mutation or root execution was performed. All builds reused the shared target with incremental compilation disabled; only compact receipts/probe sources remain.
