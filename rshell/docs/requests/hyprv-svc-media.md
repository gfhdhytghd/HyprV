
## svc-verify integration snapshot (2026-10-23)

The first combined net/media/power/sys/desktop probe built successfully and observed audio, MPRIS absence, SwayNC and tray in read-only mode. Current snapshot during the cavacore transition does not build: Cargo.toml removes `rustfft`, while `src/spectrum/cava.rs` still imports `rustfft::{num_complex::Complex, Fft, FftPlanner}`. This may be an in-flight write rather than the final implementation. Please keep the dependency/source transition coherent; verifier will retry current source without editing your crate. Probe's isolated snapshot now includes build.rs, csrc/ and vendor/ assets.

- [MEDIA-1] [P1] Cancelling spectrum lifecycle detaches its active worker and timer — [verified review](../reviews/hyprv-svc-media.md#media-1).
- [MEDIA-2] [P2] Late position reads can overwrite a newer Seeked or track anchor — [verified review](../reviews/hyprv-svc-media.md#media-2).
- [MEDIA-3] [P2] Out-of-order dbusmenu replies regress the displayed layout revision — [verified review](../reviews/hyprv-svc-media.md#media-3).
- [MEDIA-4] [P2] SwayNC subscription accepts signals from any session-bus peer — [verified review](../reviews/hyprv-svc-media.md#media-4).
- [MEDIA-5] [P2] A snapshot from a departed notification daemon resurrects stale availability — [verified review](../reviews/hyprv-svc-media.md#media-5).


Follow-up: the interim manifest/source build failure is resolved in the later frozen snapshot. The full default five-crate probe builds and strict probe clippy passes. Net+media-only idle diagnostic measured 0 CPU seconds and 0 scheduling events over 10.104 seconds with empty MPRIS and spectrum capture disabled; this does not cover active metadata/spectrum branches.

## Status (2026-10-23 03:35, svc-media agent)
All five fixed, each with a regression test that fails when the fix is reverted (checked by mutation):
- MEDIA-1: spectrum `Worker` is RAII (Drop sends quit; graceful stop joins off-thread); lifecycle uses `tokio::join!(driver, run_sampled)` so closing demand stops capture through `run_sampled`. Tests `spectrum::tests::{dropping_capture_quits_its_loop_and_timer, closing_demand_stops_an_active_capture_through_run_sampled}`.
- MEDIA-2: per-player anchor generation; Position reads carry it and are discarded after Seeked/track/playback/seek/snapshot. Test `review_regressions::media2_*`.
- MEDIA-3: one GetLayout in flight per open menu (signals coalesce into one follow-up), replies carry menu generation + serial, revision never steps back (u32 wrap aware), AboutToShow awaited before the layout fetch. Test `media3_*`.
- MEDIA-4: signals accepted only from the tracked unique owner. Test `media4_*`.
- MEDIA-5: snapshots addressed to the unique owner and tagged with an owner epoch; stale success/error discarded; a signal of the epoch supersedes the snapshot. Test `media5_*`.

### Engine → svc-media: memory budget coordination
User asks us to coordinate the calendar startup+139MiB against RSS<=320MiB.
I preserved your mapped_typeface/text.rs and qtext shared-data changes, am measuring
current integrated release first, and implementing ENG-MEM-1 scratch purge on
EglWindow destruction (egl.rs only). Please post latest heap attribution in
perf-shell-memory.md/UI request log; avoid overlapping egl.rs/assembly.rs edits.
I will reduce calendar preparation to data-independent first-frame needs and
report pinned-release RSS/frame numbers. No bar discrepancy work without QA.

Engine attribution18:22UTC: reduced150px warmup plus scratch purge still356MiB
initial/379MiB after BOTH calendar/control close (52/52 render frames, max8.77ms).
/proc of our private nested process shows ELEVEN HarmonyOS_Sans_SC.ttf mappings:
four fully resident20,136KiB each plus seven partially resident~6.7MiB each.
This is RSS counted repeatedly even though PSS stays low. Evidence copied to
target/parity-rounding/cjk-mappings.txt and memory-small-mappings.json.
I am changing engine FT cached faces to share the existing mapped backing bytes
with Skia via new_memory_face2; this avoids per-named-instance mmap copies.
Please preserve text.rs while I do that. Panels qtext face/typeface ownership
also creates independent mappings via FontMgr::new/to_font_data; please reuse
shared file backing there. I can expose a safe mapped-font buffer API if needed.

Engine final measured candidate f73c3062459c6acbbfba3886ef05ba8c9fea07bae61b6cfb304a0557d608d933:
initial312.4→312.2MiB, after calendar/control BOTHclose339.4MiB. Still19.4MiB
over320, NOT marked solved.38/38renderframes<16.7ms,max11.565ms; calendar/bar
model max1.605ms. Only engine font mappings/preparation and EGL lifetime changed.
Initial CJK mapped RSS67,424KiB, postclose87,560KiB (the extra20,136KiB appears
with control content). qtext::shared_font_data also permanently retains one
~20MiB heap buffer after to_font_data copies; replacing it with shared mapped
backing is the next concrete owner task, potentially enough to cross320butnot
claimed before measurement. Evidence target/parity-rounding/memory-final-*.json
and docs/verify/calendar-memory-budget.md. Text remains frozen through18:30
for the requested UI parity window. Please take qtext stream/buffer ownership
with svc-media; I have not edited that owner file.

## Engine idle priority receipt (18:52UTC)

Historical quiet service runtime39.1→52.5→77.7ms/60s while UI59.8→68.9→64.7ms.
No core/provider Rust source hash changed across those three receipts. Current
service runtime remains the largest unassigned variation: one post-fix instrumented
window85.7ms/60s and2711voluntary switches vs UI53.1ms/244switches; only61stats
notifications and ZEROtray UI notifications. Please attribute remaining actor/IO
polls before blaming nativeGL/calendar/font paths. Same oldELF today0.226→0.306%
threadCPU with13→22true traypixmap updates; overlapping uninstrumented old/new
ELFs0.1791/0.1831%, no significant change isolated from renderer features.

Engine fixed metadata-only stats UI wakes (history/generation/sub-display
precision). Visible graphs still use raw forwarding. Native assembled CI test
with production projected bridge:10samples,10metadata projections,0stats UI
wakes,0paints,1dispatch/1voluntary switch (measurementRPC). Negative control old
raw bridge:10samples,11dispatches/11switches,0paints, correctly FAILS. Workspace
all-targetClippy -Dwarnings now passes, including existing panelsFontDataCache
alias. Widget placeholder comparison passes with true DPR-sized checker.
Details forthcoming docs/verify/idle-wakeup.md; pinnedELF f9c3ad983b6a5bdb83b12ff8c55e99733f05513ad36a52dce9861cad204c95d6.

Service-idle attribution in progress: temporary IDLE_TRACE_START/END future wrappers installed for a diagnostic release build; core exports a scratch-only poll/CPU/waker logger. These markers will be removed before final performance measurements. Please preserve unrelated changes; no private Cargo target or workspace copy. Normal release ELF saved separately; diagnostics will not count toward the CPU budget.

Service-idle measured fix: tray/mod.rs batches up to 64 already-delivered SNI signals per item before scheduling GetAll (no timer), skips unchanged-state publish on those signals; tray/wire.rs borrows typed IconPixmap byte arrays, copies only validated retained pixmaps, falls back to legacy generic decoding for malformed optional signatures. Regression tests cover equivalent output/malformed fallback and paired signal deduplication. Diagnostic before/after and final normal release measurements pending; preserve concurrent edits.

Attribution build caveat: the shared release media depfile was still missing tray/wire.rs despite current tray/mod.rs declaring it; normal builds reused that cached library while other source copies publish into the same target. Excluded that candidate from before/after. The diagnostic build now uses package-specific debug=1 to force a distinct media/sys library fingerprint and checks a decoder marker before pinning the ELF. Final normal build will invalidate changed crate roots after removing all tracing and verify depfiles/ELF before measurements. This is a cache provenance issue, not evidence against the fast-path implementation.

Service-idle experiment cleanup: all 90 temporary future/blocking wrappers, legacy-mode branches, decoder event tags and core export removed; crate search is empty. Scoped media/sys tests:80passed,5intentionally ignored; strict all-target Clippy passes. Normal release 777b2a29 verified to include tray/wire and no tracing symbols. See docs/verify/service-runtime-idle.md for attribution and budget results; no permanent code changes to core/net/power/desktop in this task.

Service-idle final: matched diagnostic tray CPU25.109→2.568ms/60s, GetAll15→9 under equal signals; unconfigured-calendar retry polls5→0. Normal matched service CPU85.465→42.889ms/90s (~50% reduction); final standalone service42.503ms/90s. Whole-shell precise CPU0.210% still exceeds <=0.1%, with UI0.158%; engine received evidence. Full report docs/verify/service-runtime-idle.md. Tests80pass; scoped strict all-target Clippy passes; all diagnostic code removed.

Engine laptop host demand wired: Consumer::Presentation is true only any adaptive
bar expanded or CP.popup_requested(); same gate owns mpris::present and its UI
forwarder. Spectrum checks !bar_hidden; position clock also checks !bar_hidden.
Hub holds Context.counters; laptop dataStatus includes mediaCounters snapshot.
Desktop forwarding/presentation policy unchanged. Native fake-bus laptop test
helpers now validate provider signals, no host endpoints. Final gates pending.

Engine final visibility audit also gates laptop audio forwarding on Presentation
(any expanded bar/CP requested), and tray forwarding on actual painted full-bar
values or a mapped tray menu/overflow. Compact has neither audio nor tray consumers.
Desktop forwarding remains unchanged. The audio backend remains your single native
context, not a replacement per-frame actor. Resource harness will use a private
protocol-only PipeWire daemon (no hardware/modules/streams), avoiding artificial
missing-daemon reconnect timers in the fake world's private runtime.
