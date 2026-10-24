# hyprv-ui-island

Port of `quickshell/DynamicIsland.qml` (+ `AgentTaskModel.qml`; shared controls from `hyprv-widgets`,
island-local Qt Canvas spectrum paths) per `docs/spec/ui-island.md`. Pure UI: no I/O, no timers of its own,
no layer surface (the island lives in the bar surface).

## Public API (for the hyprv-shell mount)

```rust
use hyprv_ui_island::{Inputs, Media, Island, Pointer, IslandAction, update, model};

// 1. Inputs from service topics (host owns subscriptions):
let inputs = Inputs {
    dark,                                   // ui-state theme
    now_hh_mm: local.format("%H:%M"),       // minute ticks
    media: Media::from_mpris(&mpris_state, stepped_position_s),
    spectrum: model::spectrum_values(&spectrum_state),
    agents: model::agents(&agent_island_state, now_epoch_s),
    calendar_open: calendar_presented,      // host calendarPanelPresented
    calendar_progress,                      // host calendarProgress
    art: decoded_art_for(&mpris_state.art_url), // host-decoded image or None
};
let mut island = Island::new(inputs, bar_width, now);   // one per output

// 2. Each service delta / host change:
island.set_inputs(next, now);
island.set_host_width(bar_width, now);      // output resize
island.set_calendar(open, progress, now);   // per-frame calendar fade without cloning Inputs

// 3. Pointer in BAR-SURFACE coordinates (the island centres itself, top margin 10):
let out = island.pointer(Pointer::Motion { x, y }, now);   // Press/Release/Leave/Wheel
// Native grab loss: Pointer::Cancel (or island.cancel_pointer(now)); Leave alone is NOT a cancel.
dispatch(out.actions); set_cursor(out.cursor);

// 4. Frames: call while animating or when a deadline fires
let more = island.frame(now);               // true while Behaviors run
let deadline = island.next_deadline();      // 230/700 ms timers, 66 ms idle wave
// 5. Scene: island-local root (0,0,w,h), clip; host places it at geometry().rect
let node = island.build();                  // or update(&mut scene, island_root_id, &island)
// Rebuild only when island.visual_rev() differs from the value at the last build
// (None = animating → rebuild every frame). update() does this itself.
let g = island.geometry();                  // width/height/attached_center_offset/expanded/rect
```

- `Geometry` feeds `hyprv-ui-bar::model::IslandGeometry` / `HostAction::Island`:
  animated width/height, `attached_center_offset` (host shifts its centre container),
  `expanded` (surface → 820 now, back to 58 230 ms after it clears), `rect` (input
  region part 2, host coordinates).
- `IslandAction` → services: `PlayPause/Previous/Next/Seek(s)` → `mpris::MprisAction`
  (the host applies the optimistic `mediaPlaying = !mediaPlaying` for PlayPause),
  `FocusMediaApp` → `MprisAction::Raise` (+ `FocusFallback` via Hyprland),
  `FocusAgent(id)` → `AgentAction::FocusSession`, `Lock`/`Power` → app activation of
  hyprlock / wlogout, `ToggleCalendar` → host calendar toggle for this output.
- Media position: the legacy island stepped once per second (1 s poll, §6.2). Sample
  `MprisState::position_seconds(monotonic_us)` on a 1 s cadence **only while the
  expanded music view is visible** and pass it in `Media.position_s`.
- `clip_hover_to_input_region` (default true) limits hover to the 58-px strip + island
  rect like the live input region; the parity harness disables it.
- Wakeups: a stationary island has `animating() == false` and `next_deadline() == None`,
  except the QML idle wave (66 ms) while a compact/expanded spectrum is visible and
  paused or without spectrum data — required for parity, stops when hidden.

## Behaviour notes / quirks (named constants in `island.rs`)

- `QUIRK_SWIPE_TRANSLATION_RESET` (true): measured on Qt 6.12 — the DragHandler's
  `translation` is reset before `onActiveChanged` runs, so swipe-to-hide never fires.
- `QUIRK_SWIPE_CANNOT_RESTORE`: §4.3 left-swipe cannot restore (only relevant if the
  above is flipped).
- `QUIRK_HOTSPOTS_BLOCK_BUTTON_HOVER`: z-99 side hotspots take hover, idle lock/power
  never show their hover fill (reference `idle-hover-lock`).
- Qt delivery model reproduced: topmost hover-accepting MouseArea wins, ancestor
  HoverHandlers still hover, sibling HoverHandlers below an accepting area do not;
  hover is frozen while a button is held; hover is re-delivered every frame under a
  still pointer (frame-synchronous hover). TapHandlers fire only when no MouseArea
  grabbed the press.
- `WHEEL_STEP_PX` (60): agent list wheel step; parity only checks the settled offset.

## Validation

The current strict production-GPU result is **380/380 bit-exact**. All eleven
max-1 rendering residuals are closed; the user-approved canonical recapture
explicitly parks the pointer for the last stale-hover fixture. No thresholds or
images are waived. [Per-class metrics, regressions and limitations](VALIDATION.md).
`Island::build()` now opts its subtree into the engine's existing Qt GPU materials
and direct native-text path; the public mount API is unchanged and runtime stays
Qt-free. The loose CPU-parity numbers below are historical, not the strict gate.

```sh
cargo test -p hyprv-ui-island            # unit + behaviour + parity
cargo clippy -p hyprv-ui-island --all-targets --no-deps -- -D warnings
```

`tests/parity.rs` replays all 35 `parity/fixtures/island/*.json` fixtures on a 60 Hz
virtual clock exactly like `parity/host/main.cpp` (settle 1500 ms, pointer parked in the
corner, hover/press/drag/wheel/frames steps, `Qt.callLater` in the same turn), renders
offscreen at @1 and @2 in light and dark, and diffs against `parity/reference/island`
(premultiplied mean channel delta ≤ 2.0/255, ≤ 2 % pixels > 48). Result 2026-10-23
against the 03:20 recapture: **380/380 captures pass, average mean 0.18, worst 1.61**
(mid-morph frames). Mine/heat PNGs and `report.tsv` go to `$HYPRV_ISLAND_PARITY_OUT`.
The calendar fixtures use a harness copy of the host `centerSection` morph (owned by
the bar/shell host); only the island fade inside it is this crate's.

`tests/calendar_morph.rs` (piece B) replays the shared **panels** calendar fixtures
(`parity/fixtures/panels/calendar-*`, crop of the full desktop): this island, the bar
host's `centerSection` morph and `hyprv-ui-panels`' `CalendarPanel`, fed by the panels'
shared fixture translator. It is the reference implementation of the calendar host
morph for the bar/shell (52/52 captures pass, worst mean 1.44). Dev-dependencies:
`hyprv-ui-panels`, `chrono`.

Not covered by references: real fractional output scales (only @1/@2), album art from
http(s) URLs (host decodes), live pointer/scroll physics (Qt flick deceleration).
Assets: `assets/codex.png`, `assets/claude.png` copied from `quickshell/assets/agent/`.
