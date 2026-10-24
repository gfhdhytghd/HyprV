# Laptop profile: QuickAdjust delta vs main (spec)

Owner: spec-bar (QuickAdjust is spec-bar's per the panels ownership split). Read
`ui-panels-stats.md` §1 (desktop QuickAdjust) first. This file covers only how the laptop
branch differs. It supersedes the QuickAdjust parts of `laptop-panels.md` §2.1–§2.2 where the
two disagree; they currently agree.

Line references:
- `L:n` = `git show origin/laptop:quickshell/shell.qml` at `24972f7`;
- `M:n` = the main working tree's `quickshell/shell.qml`.

Hardware semantics (curve, HDR SDR white, ALS, persistence) belong to svc-power and are
specified in `laptop-services.md` "Backlight and HDR brightness". The shell never implements
the curve.

## 0. Summary

| # | item | main (desktop) | laptop |
|---|---|---|---|
| 1 | `QuickAdjustPopup.qml` | — | **identical file** (geometry, slider, animation, auto-hide) |
| 2 | UI step `brightnessUiStepPercent` | 2 (M:149) | **1** (L:156) |
| 3 | `snapBrightnessPercent(v)` | `clamp(round(clamp(v)/2)*2)` (even) | `clamp(round(v))` (any integer 0..100) |
| 4 | IPC `showBrightnessIncrease/Decrease` | ±2 | **±1** |
| 5 | set path (`applyBrightnessPercent`) | serialised `brightness_set(n, false, true)`, one in flight, pending replaced, sequence-numbered; probe debounce **stopped** on each set (M:2782-2840) | **fire-and-forget** `brightness_set(n)` per value, then **restart the 90 ms probe debounce** (L:2877-2882) |
| 6 | `finishBrightnessPercent` / `brightness_finish` | exists (control panel release) | **removed** (QuickAdjust never used it on either branch) |
| 7 | probe result guard | applied only if no newer request, not interacting, nothing pending/in flight (M:2993-3020) | **unguarded**: every finite result applied (L:3078-3101) |
| 8 | card y | `primaryBar.exclusiveZone` = 58 | 58 with the bar full, **25 with the bar compact** (exclusive zone `round(25+33*reveal)`) |
| 9 | who triggers the OSD | desktop Lua → `hyprv-shell ipc call quickAdjust …` (after cutover) | laptop Lua → **`quickshell -p … ipc call quickAdjust …`** (`origin/laptop:hypr/controls.lua:46-47`) |

Everything else is identical: volume mode, `applyValue` volume exec, mute toggle, `show` /
`hide` animations (120 ms OutQuad opacity, 160 ms OutCubic scale from 0.96, TopRight origin,
140 ms close finalize, 3000 ms auto-hide), the 278×62 card, icon U+F00DF, and the input mask.

## 1. Brightness level model (laptop)

```
clampBrightnessPercent(v) = max(0, min(100, Math.round(v)))                 L:2863-2865
snapBrightnessPercent(v)  = clamp(Math.round(clamp(v) / 1) * 1) = clamp(round(v))   L:2867-2869
updateBrightnessPercentLocally(v): brightnessPercent = snap(v); return it    L:2871-2875
```

- `QUIRK_LAPTOP_BRIGHTNESS_STEP_1`: every integer 0..100 is a reachable, displayed level.
  Fill fraction = `brightnessPercent / 100` on the existing slider. There is no marker or
  discontinuity at 70, where the HDR stage starts, because the slider is linear.
- JS `Math.round` is round-half-up. For the non-negative inputs here, Rust `f64::round`
  (half away from zero) is identical. Negative drag overshoot clamps to 0 first.
- rshell code points to change, both currently hard-coded to the desktop step:
  - `hyprv-ui-panels/src/control_panel.rs:120-124` `snap_brightness` (step 2). This module is
    svc-power's; QuickAdjust imports it. Make the step a runtime profile parameter: `snap(v,
    step)` with step 2 on desktop, 1 on laptop. Do not fork the function.
  - `hyprv-shell/src/assembly.rs:1643-1652` `preview_brightness`: ±2 and the even snap
    `((v+1)/2*2)`. On laptop use ±1 and `clamp(v,0,100)` with no evening.

## 2. Set path (`applyBrightnessPercent`, L:2877-2882)

```
applyBrightnessPercent(v):
    n = updateBrightnessPercentLocally(v)          // optimistic UI value, immediately
    exec detached: hyprctl eval 'require("hypr.controls").brightness_set(' + n + ')'
    brightnessProbeDebounce.restart()              // 90 ms single-shot (L:3103-3109)
```

- It is called from `QuickAdjustPopup.applyValue` (identical file, `QuickAdjustPopup.qml:58-68`)
  on every distinct slider value. On laptop, the control-panel brightness slider uses the same
  function with no release handler (`laptop-panels.md` §1.2).
- **Native port:** emit `Action::BrightnessSet(n)` only. **Never `BrightnessFinish` on
  laptop.** `QUIRK_NO_BRIGHTNESS_FINISH` (`quick_adjust.rs:18`) already holds for QuickAdjust
  on both profiles. On laptop, svc-power's laptop planner (`CurveKind::Laptop`) consumes the
  request. Its contract is "one hardware op in flight, latest wins, final value always
  applied" (`laptop-services.md`). The UI may therefore send every value. It must keep order
  and must never drop the last one. No `hyprctl` exec at runtime.
- The host's per-output `input_values` de-dup in `assembly.rs` `panel_action` (one action per
  distinct value) matches QML `valueChanged` semantics and stays unchanged.
- `QUIRK_LAPTOP_BRIGHTNESS_PROBE_AFTER_SET`: each set restarts the 90 ms probe debounce, so a
  read-back happens 90 ms after the **last** value of a drag. Main instead *stops* the
  debounce on set and probes only after finish. Native: after the 90 ms quiet period,
  re-read the provider's reported level (the analogue of `brightness_get`, below) and apply it
  (§3).

## 3. Probe / read-back (L:2916-2925, L:3077-3101)

```
refreshBrightnessStatus(showQuickAdjust):
    if showQuickAdjust: _showQuickAdjustAfterBrightnessProbe = true
    if probe running: _brightnessProbeQueued = true; return
    run: hyprctl repl 'return require("hypr.controls").brightness_get()'
probe exit:
    parsed = Number(trim(stdout))
    if exit == 0 && isFinite(parsed): brightnessPercent = snap(parsed)      // NO guard
    if _showQuickAdjustAfterBrightnessProbe: clear it; quickAdjustPopup.show("brightness")
    refreshControlPanelStatus()
    if queued: clear it; run again
```

- `QUIRK_LAPTOP_BRIGHTNESS_PROBE_UNGUARDED`: the result is applied even mid-drag. The laptop
  `brightness_get` returns `wanted`, then `busy.level`, before reading hardware
  (`origin/laptop:hypr/controls.lua:77-87`). It therefore echoes the latest requested level,
  and the slider does not jump back while the request is pending. Native equivalent: the
  provider's UI level = pending requested level if any, else the in-flight level, else the
  inverse curve of the confirmed hardware raw. The shell applies it unconditionally.
- `nil` (hardware not yet known) gives NaN and is ignored. Native: `None` changes nothing.
- External changes (ALS/wluma writing intel_backlight) reach the UI only when a probe runs:
  on show, panel open, or after a set. QML never watches for them, so the OSD **does not pop
  up** on ALS changes.
  - Native: the provider may push its level into the model. The **OSD must not be shown** by
    such pushes.
  - Pushing a value into an already visible slider is a deviation that is invisible in
    practice, since the QML would show the same value at the next probe. Record it as
    `QUIRK_LAPTOP_BRIGHTNESS_PASSIVE_UPDATE = false` (QML-exact: only update on probe
    triggers).

## 4. IPC entry points (L:3500-3530; identical bodies except the step)

| fn | laptop behaviour |
|---|---|
| `showBrightness()` | `show("brightness")`, then probe |
| `showBrightnessLevel(level: int)` | if finite: `brightnessPercent = snap(level)` (no evening on laptop); `show("brightness")`; `refreshControlPanelStatus()`; restart the 90 ms probe debounce |
| `showBrightnessIncrease()` | `previewBrightnessDelta(+1)`: `brightnessPercent = snap(current+1)`, `show("brightness")`, restart probe debounce. **Local preview only: it sends no set.** |
| `showBrightnessDecrease()` | the same with −1 |
| `showVolume()` | `show("volume")`, `refreshAudioStatus()` (unchanged) |

- Callers on laptop:
  - `brightness_set(level, true)` → `notify("showBrightnessLevel", level)`
    (`origin/laptop:hypr/controls.lua:89-97`);
  - `brightness_step(±2)` from `XF86MonBrightnessUp/Down` and `ALT+XF86AudioRaise/LowerVolume`
    (`origin/laptop:hypr/media-binds.lua:22-25`), which calls `brightness_set(get+delta, true)`;
  - volume → `showVolume` 80 ms later (`controls.lua:125`).
- Keys step the hardware by **2** while the UI step is **1**. The OSD shows the exact level the
  compositor sent, which can be odd. This is why desktop's even snap must not be applied on
  laptop.
- The compositor invokes `quickshell -p '<cfg>/quickshell' ipc call quickAdjust …`. At
  cutover this must reach `hyprv-shell` (request filed in `docs/requests/hyprv-shell.md`,
  "Laptop bar requests" item 2).

## 5. Placement with the adaptive bar

`QuickAdjustPopup.updatePopupPosition()` (identical, `:150-174`):
- y = `max(0, primaryBarWindow.exclusiveZone || 58)` = `round(25 + 33*reveal)`, i.e. **25**
  when the primary bar is compact, 58 when full. In compact the card sits 10 px below the
  15 px strip.
- x right-aligns to `quickAdjustAnchorItem` = the primary bar's `rightTrayPill` (L:4306),
  via `mapToGlobal`. That item still exists and is laid out while compact (only
  `contentRoot.visible` is false and y is shifted), so x is the **same as in full mode**,
  aligned to the right edge of an invisible pill.
- `QUIRK_QA_POSITION_FROZEN_WHILE_MAPPED`: the position is recomputed only on map
  (`onVisibleChanged`), width/height change or screen change. Calling `show()` while mapped
  does not reposition. If the bar toggles while the OSD is up, the card keeps its old y until
  it hides (3 s idle) and is shown again.
- Layer: QuickAdjust is Overlay, as is the compact strip's corner window. Stacking between two
  Overlay surfaces follows map order. They do not overlap (strip y 0–15, card y ≥ 25), so the
  order is moot.

## 6. Data contract changes

```rust
struct QuickAdjustInputs {            // ui-panels-stats.md §1.5, plus:
    brightness_step: u8,              // 2 desktop, 1 laptop (profile)
    bar_bottom: i32,                  // live primary-bar exclusive zone: 25..58 on laptop
    /* unchanged: mode, brightness_percent, audio_*, anchor_right_x */
}
// actions (laptop): BrightnessSet(u8) only; no BrightnessFinish. Volume actions unchanged.
```

## 7. Parity fixtures (laptop, light + dark, scale 2, 1440×900 logical)

1. `lap-qa-brightness-37`: `showBrightnessLevel 37` with the bar full. Card at y 58, value
   37, an odd value that desktop would round to 38.
2. `lap-qa-brightness-compact`: the same with the bar compact. Card at y 25 with the strip
   visible above it.
3. `lap-qa-step`: `showBrightnessIncrease` ×3 from 50 → 53 (local preview, no set action
   recorded).
4. `lap-qa-drag`: drag 40→71 → action log `BrightnessSet(41…71)` in order, no Finish, and one
   probe 90 ms after the last value. A probe answer equal to the last request leaves the
   value unchanged.
5. `lap-qa-volume`: unchanged from desktop, but at y 25 when compact.
6. `lap-qa-toggle-while-shown`: the OSD shown while compact, then a bar toggle. The card
   stays at y 25 (frozen) until it remaps.
