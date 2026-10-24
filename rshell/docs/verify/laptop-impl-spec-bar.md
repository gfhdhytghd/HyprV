# Laptop Phase B — spec-bar implementation report (2026-10-24)

Specs: `docs/spec/laptop-bar.md`, `docs/spec/laptop-quickadjust.md`. Runtime profile only
(`Model.profile` / `QuickAdjust::with_profile`, from `hyprv_core::profile`); no cfg forks.
Nothing committed. Public API: `docs/requests/hyprv-ui-bar.md` ("laptop bar API landed").

## What landed

`crates/hyprv-ui-bar` (mine):
- Adaptive bar: per-output `expanded`, starts compact; `reveal` animated with the Hyprland
  move curve (overshoot kept); global 500 ms `ipc::ToggleGate`; exclusive zone 25↔58 and
  Overlay/Top layer follow `reveal`; content (island, calendar) hidden while compact.
- Compact strip in the corner window (`surfaces::compact_strip`), corners at
  `topCornerOffset`, strip input mask (`corner_input`), strip clicks
  (`Controller::pointer_compact`), `HostAction::ToggleBar`.
- Left pill without Viewflow; `CpuPower` module replaces temperature (12/8 padding,
  severity colour, hover-only) with the 350 ms glass tooltip (`tooltip.rs`).
- Battery from UPower (5-way mode incl. idle); battery popup reachable on laptop.
- IPC `layoutStatus`/`dataStatus` live laptop values (`ipc::Diagnostics` fields).
- Named quirks for every delta (`model.rs`), plus one found against the references:
  `QUIRK_NULL_ESTIMATE_IS_ZERO` (`Number(null)` → "0分钟 (按均值)"; no desktop fixture
  affected). (`QUIRK_TOOLTIP_GLASS_DPR1` was removed after the recapture, see below.)
- Replay: `examples/reference.rs` + `examples/reference/laptop.rs` replays laptop fixtures
  (virtual timeline, snapshot sequences, UPower/NM services, toggle/hover steps, island
  composed from `hyprv-ui-island`, production sourceSize×DPR icon rasters).

`crates/hyprv-ui-panels` (QuickAdjust is mine): step 1 via `snap_brightness_for`, never
`BrightnessFinish` on laptop, card position latched while mapped. Shared edits logged in
`docs/requests/hyprv-ui-panels.md` (`panels.rs` one line; `tests/shared_common` laptop-only
preview + 90 ms probe emulation).

## Re-run against the recaptured references (2026-10-24 12:39)

The capture host now builds laptop scenes inside the scale-N window
(`PARITY_SCENE_IN_WINDOW=1`; `docs/verify/laptop-impl-spec-island.md`). The first replay
against the new refs passed every @1 image but failed most @2 images. Three of my
workarounds had matched the old DPR-1 harness artifact instead of Qt:

| diff vs new refs (@2) | cause | fix |
|---|---|---|
| tray icons, ~2056 px, max 94 | replay swapped in the `@2-initial` DPR-1 tray icons | override removed (`examples/reference/laptop.rs`) |
| strip control icon, 40 px, max 49 | SVG rasterised at 200 px, not 200×DPR | `surfaces::strip_icon(dpr)` rasterises at 200×DPR (the spec's open "200 vs 200×DPR" question: it is ×DPR) |
| full-bar control icon, 16 px, max 12 | the desktop fixture loader forces the canonical host's DPR-1 32 px raster | laptop replay uses the production `baked` sourceSize×DPR raster |
| tooltip glass, max 24 | `QUIRK_TOOLTIP_GLASS_DPR1` | quirk deleted; the glass uses the output DPR |

The runtime changes (strip icon raster, tooltip glass) only affect laptop-only surfaces, and
the desktop result is unchanged.

## Evidence (production GL path, private headless Weston, recaptured refs)

| set | result |
|---|---|
| laptop-bar (`parity/laptop/verify-laptop.py --component laptop-bar`, new refs) | **248/248** (94 exact, max 4); was 128/248 before the fixes |
| laptop battery popups `lap-battery-popup-*` (gate_metrics, new refs) | **28/28** (max 2) |
| laptop QuickAdjust `lap-qa-*` card region (`tests/laptop_quick_adjust.rs`, new refs) | **32/32**, no change needed |
| desktop bar (`parity/reference/bar`, gate_metrics) | **312/312**, 308 exact, max 2 — same as the prior owner receipt |
| unit tests | full `hyprv-ui-bar` suite green after the fixes, incl. `tests/laptop.rs` 6/6; QuickAdjust laptop tests pass; bar + panels suites green except below |

## Not done / open

- `tests/laptop_parity.rs` (svc-power, `lap-cp-*`) fails 37 @2 images (max 15–17). No
  `lap-cp-*` fixture touches the QuickAdjust path I changed; left to svc-power.
- Desktop nested e2e (204) not run by me: `hyprv-shell` is being rewired concurrently by
  engine/shell. Host gaps for them (strip input routing, tooltip popup mapping, the 90 ms
  laptop brightness probe revert) are filed in `docs/requests/hyprv-shell.md`.
- `lap-qa-toggle-while-shown` compares only the card part below the bar pills (the bar
  draws over the frozen card); full-frame composition of bar + OSD is host territory.
