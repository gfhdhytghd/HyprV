# Panel Flickable integration — 2026-10-23

Every panel Flickable in the current QML is wired to `hyprv_widgets::flickable`.
This is input/position parity evidence; it does not claim GPU/pixel or live-device parity.
No live shell, compositor, device, user configuration or privileged service was changed.

## Inventory and settings

| Rust owner file | Actual QML | Properties reproduced |
|---|---|---|
| `wifi.rs` | `WifiFallback.qml:667-971` | VerticalFlick; StopAtBounds; width 364; viewport capped to the first five **current** card heights plus spacing; overflow-only interaction. The overlay MouseArea at 956-970 assigns/clamps contentY immediately: pixelDelta.y else angleDelta.y/2. This deliberately bypasses default wheel easing. |
| `control_panel.rs` | `ControlPanelBluetoothDetails.qml:468-584` (extracted from the file named ControlPanelPopup in the older spec) | AutoFlickDirection; StopAtBounds; width 364; viewport min(contentHeight,420); overflow-only interaction; default Qt wheel/drag/flick. |
| `calendar.rs` | `WeekCalendarPanel.qml:546-679`, helpers 260-328 | HorizontalAndVerticalFlick; StopAtBounds; deceleration 2600; pressDelay 80; content 21 days × 24 hours (44 px/hour); viewport follows all-day lanes. Custom vertical/dominant-axis wheel assignments; 132 px/notch vertically and one day/notch horizontally when handled. Rebase by seven days on contentXChanged, cancelling only the horizontal timeline and preserving the held drag anchor; week changes position near now. |

Search of `docs/spec/ui-panels*.md` and all top-level QML finds these **three** panel
Flickables, no panel ListView or ScrollView. The agent ListView is
`DynamicIsland.qml:1318` (already integrated in the preceding task). Mihomo uses
ordinary layouts/Repeaters, with no scrolling container. No artificial scroll areas
were added to these layouts.

The defaults remain 10 px startDragDistance, 1500 px/s² deceleration (except calendar),
2500 px/s maximum velocity, FollowBoundsBehavior movement, and Qt's default rebound.
Controller configuration remains overridable; those are the captured Qt 6.12 defaults.

## Details verified by the traces

- `Flickable::drag_both` shares Qt's threshold/grab between calendar axes, including
  diagonal movement whose minor axis initially remains below the threshold.
- MouseArea clicks survive small/paused gestures and dispatch once; grabbed drags do
  not activate rows. Calendar event-card presses are deferred for 80 ms, replayed on
  an early release, and cancelled on grab/reset. Its single deadline is included in
  `Panels::next_deadline` and cleared on replay/cancellation.
- The calendar's WheelHandlers (417,477,562) leave `orientation` at Vertical. An initial
  pure horizontal event falls through to default timeline Flickable physics; while a
  handler is active, it accepts subsequent horizontal events. Each of the three
  handlers has independent activation. The unobserved 100 ms active timeout is
  evaluated lazily at input, so no idle wakeup is scheduled just to change an unused
  property. Native phased sessions stay active through End, as Qt does.
- Disabling interaction after a list shrinks cancels a **held press**, but does not
  erase a released flick. A running flick retains its acceleration and brakes against
  the changed extent; a fixup retarget uses Qt's ExtentChanged OutExpo path. The
  mid-flick shrink probes first failed with immediate reset and now pass. The calendar's old build-time clamp was removed too: Qt temporarily reaches 702.670850 px
  after a viewport grows under a flick with a 696 px bound; the new trace checks the
  actual panel build preserves that motion rather than clipping it to 696.
- Synthesized ScrollUpdate without ScrollBegin cannot establish a pressed session.
  The shared controller now ignores that malformed sequence, matching Qt. Valid
  begin/update/momentum/end sessions are also captured.
- Wi-Fi close/unmap, Bluetooth page destruction, and calendar reset clear motion and
  pending work. No controller timer/worker is added; `tick(now)` advances frame time
  and `animating()` requests only the frames that are needed.

## Reproduce

```sh
python3 docs/reviews/scratch/panels-flickable/capture.py
cargo test -p hyprv-ui-panels -p hyprv-widgets --no-default-features --lib -- --nocapture
cargo test -p hyprv-ui-bar -p hyprv-ui-island --test flickable --test behavior
cargo check -p hyprv-ui-panels --lib
```

`scene.qml` extracts the exact settings and wheel/rebase handlers above into tiny
Qt Quick scenes, using ten Wi-Fi/Bluetooth rows and a calendar with no all-day lanes.
Delegate MouseAreas exercise the same child-event filtering; Rust tests build clipped
row hit regions and replay through the panel's actual input/tick methods. One Wi-Fi
close test also builds the actual panel tree. There is no rendering approximation in
contentX/contentY measurements.

`host.cpp` reuses the preceding task's QQuickRenderControl/QAnimationDriver method
(see `parity/README.md`). Nothing is shown or mapped. Timestamped native QMouseEvents
exercise Qt's own velocity estimator; QWheelEvents preserve pixels, angles, source,
and phase. Qt's basic pressDelay and WheelHandler-deactivation timers are delivered
to their real timerEvent implementations at their virtual deadlines, without real
sleeps. CSVs record every 16 ms frame from 0 through 3488 ms. Qt-only dummy clock
animation does not run in the Rust product.

The 78 traces cover wheel bursts/reversal, pixels versus angles, valid/missing phases,
momentum, drag threshold, pauses, bounds, early/held clicks, expansion during drag,
shrink at rest/during flick, viewport resize during flick, diagonal and horizontal drags, rebase, handler timeout,
and wheel interruption of either axis. Tests also assert click counts and no remaining
frame requests/deadlines at rest. `reference-hashes.json` records the independently
repeated captures; `sources.json` hashes inputs, QML and Rust implementation files.

## Results and boundary

78 panel traces, **17,082 frame samples**. Maximum coordinate error: Wi-Fi/Bluetooth
0.000009193791 logical px; calendar 0.000038809001 px (f64 controller → existing f32
layout fields); assertions require <0.0001 px. The earlier 48 shared-widget traces
(10,952 samples) still match with maximum error 0.000002217421 px. All 46 library/lifecycle and bar/island consumer tests pass; their logs are alongside this file.

Library Clippy passes after allowing the pre-existing renderer/text lint classes
listed in `results.json`. Unfiltered `-D warnings` also reports those existing lints;
this task does not edit the renderer/text implementation. New integration lints were
fixed. `clippy.log` records the scoped run.

Native input handoff: `ControlPanel::bluetooth_wheel(x,y,Wheel,model)` and
`CalendarPanel::wheel(x,y,horizontal,vertical,model)` preserve phases/source metadata.
The legacy panel `Pointer::Wheel` has deltas only; it remains source-compatible and
cannot reconstruct missing native Begin/End/source. The shell/engine owner must use
the native entry points for full touchpad fidelity (the same existing host limitation
recorded for bar/island). No shell/engine input files were edited here.

Qt sources: local `qquickwheelhandler.cpp` is the
[Qt v6.12.0 source](https://raw.githubusercontent.com/qt/qtdeclarative/v6.12.0/src/quick/handlers/qquickwheelhandler.cpp),
retaining its license header. Flickable/Timeline references remain in
`../flickable-implementation/qt-source/` with their original provenance.
