# Laptop Phase B — spec-panels implementation report (2026-10-24)

Scope: the control-panel delta from `docs/spec/laptop-panels.md` §1 in `crates/hyprv-ui-panels`. No other
owner's files were edited, apart from appended notes in `docs/requests/`. Nothing was committed or
installed, and the live session was not touched.

## Changes
- **Dependency:** `Cargo.toml` adds a dependency on `hyprv-core`, for the runtime profile only.
- **`src/control_panel.rs`:**
  - **Profile latch:** `ControlPanel.laptop` is latched from `hyprv_core::profile::profile()` in `new()`.
    `with_profile(bool)` sets it explicitly for tests and replay. There is no cfg fork.
  - **Laptop main page (278×350, §1.1):**
    - a Power split tile at (0,144), with icon, title "Power" and subtitle Battery/Balanced/Performance;
      it is active only in power-saver, and both halves emit `ProfileSet` with the usual cycle;
    - a Display split tile at (144,216), with subtitle "Demo mode"/"Normal" and `active = demo_mode`;
      both halves emit `ToggleDemo`;
    - the VM, Viewflow, Demo and profile toggles and both fan sliders are not built. Their actions are
      also refused on laptop.
  - **Brightness (§1.2, §1.4):** `snap_brightness_for(laptop, v)` snaps to a 1 % step on laptop. Release
    and cancel emit no `BrightnessFinish` on laptop.
  - **Positioning (§1.3):**
    - IPC `toggle_centered` centres the card, at (581,275) on 1440×900;
    - anchored opens stay right-aligned, with `y = max(10, min(target, H − h − 10))`;
    - a new `layout()` re-lays out when the content height moves the card. That only happens on
      laptop, because the desktop position never depends on height.
- **`src/panels.rs`:** adds `Panels::with_profile(laptop)`.
- **Removed controls:** the reveal targets 350 automatically, because the full height comes from the
  page. VM, Viewflow and Deskflow are neither instantiated nor polled. The panel crate has no I/O.
- **Tests:**
  - `laptop_tests` (3 unit tests): layout and centring, the anchored clamp, and tiles plus brightness
    (no finish).
  - The existing grab test now pins `with_profile(false)`.
- **Parity harness:**
  - `tests/laptop_parity.rs` is new.
  - `tests/shared_common` gets additive changes:
    - `fixtures_in(dir)`;
    - `Runner.laptop`, taken from `source_ref`;
    - IPC `bar toggle` maps the exclusive zone 25↔58;
    - the capture uses the model output size;
    - the translator maps `services.wifi.devices`.

## Not implemented, and why
- **eGPU no-device state:** there is nothing to port. `confirmEgpuAction` has no caller in either
  branch's QML, so no eGPU UI is visible on desktop or laptop (D-Q3 keeps it legacy). The panel emits no
  eGPU action.
- **QuickAdjust laptop step and position (`lap-qa-*`), battery popup (`lap-battery-popup-*`):** these
  belong to other owners (spec-bar / laptop-quickadjust.md, and ui-bar). `snap_brightness_for` is public
  for QuickAdjust to reuse.
- **`QUIRK_GLASS_SHADOW_LAYER_ALWAYS`:** no visible delta. The shadow layer is a sibling of the content,
  and at `surfaceOpacity 0` with a transparent `shadowColor` it is empty. The parity result below is
  byte-exact outside one icon.

## Verification
- `cargo test -p hyprv-ui-panels --lib`: 19/19 pass.
- Builds: `cargo build -p hyprv-shell -p hyprv-ui-bar -p hyprv-ui-island` is clean, and clippy on panels
  (all targets) shows no warnings in the touched files.
- **Desktop identity:** `shared_parity` on GL (private headless Weston) passes 248/248. All 248 PNGs are
  **byte-identical** (`cmp`) to `parity/verify/out/panels-gl/actual`.
- **Profile autodetect on this desktop:** Desktop. The DMI is "System Product Name", and the only battery
  is the HID trackpad, which is not System scope.
- **Laptop CP parity:** `laptop_parity`, GL, `lap-cp-*` × {light,dark} × {@1,@2} = 92 images, gated on
  the card region `[579,273,282,354]`:
  - 56 pass, including all 46 @1 images and the open/close animation frames.
  - The other 36 are @2 images. All 46 @2 images are **byte-exact (max 0) outside the 24×24 box of the
    theme toggle's `light-dark.svg`**. Inside it the difference reaches up to 46/255 at the glyph edges,
    in the 36 failing images.
  - The QML and the SVG are identical on both branches, and the Qt laptop reference differs from the Qt
    desktop reference for this icon (mean 0.87, max 46). My render matches the desktop reference.
  - This is referred to PQ-QA (`docs/requests/hyprv-ui-panels.md`). I have not special-cased it.
  - CPU raster: 55/92. Same picture, plus one @1 animation frame with painted mean 0.64.
- `verify-laptop.py` can't gate panels-only renders yet, because the references are whole-shell frames.
  A compare-region mode has been requested.

## Open items for others
- **PQ-QA:** the light-dark icon @2 reference divergence, and a region-gating mode in
  `verify-laptop.py`.
- **svc-power:** the laptop data feeding `Model.controls` (profile, brightness probe, exact-name
  recording). See the appended note in `docs/requests/hyprv-svc-power.md`.
- **e2e (`lap.cp.nofans`, `lap.power.profile.set`, `lap.brightness.slider`):** these need the
  shell-level laptop run. The panel side emits the expected action sequences, covered by the unit tests.
  QA's `lap.brightness.slider` row still expects `brightness_finish(71)`, which contradicts
  laptop-panels.md §1.4 and laptop-quickadjust.md. On laptop no finish is sent, so please align that
  e2e row.

## Recheck (2026-10-24, after the svc-power/engine changes; spec-bar's report of 37 @2 failures)
- **Rerun:** GL gives 56/92 pass and 36 fail; CPU gives 55/92 pass and 37 fail. These are the same numbers
  as the first run, so it is **not a regression** and the data is unchanged.
- **Where the failures are:**
  - Premultiplied max is 17 (CPU and GL), only inside the theme-icon box.
  - Outside that box, the difference is max 0 on GL and max 3 on CPU.
  - The extra CPU failure is an @1 animation frame (painted mean 0.64).
- **Cause (reference side):** the laptop Qt capture loaded `light-dark.svg` at DPR 1.
  - The coverage matches the 44 px texture to within 0.4/255; the desktop reference matches the 88 px mipmap
    to within 2/255.
  - The host creates the scene before parenting it to the window, and the laptop page is instantiated
    eagerly (§1.7).
- **Fix requested** from the parity-host owner in `docs/requests/parity-capture.md`: create the scene inside
  the window, then recapture `laptop-panels`.
- **No rshell change:** rshell renders what a real scale-2 surface shows.
