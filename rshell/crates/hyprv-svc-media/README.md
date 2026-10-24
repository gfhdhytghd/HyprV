# hyprv-svc-media

Media-domain actors for the HyprV shell: PipeWire **audio** and **spectrum**, **mpris**
(with the **cider** compatibility adapter), StatusNotifier **tray** and SwayNC
**notifications**. Every module exposes a serde `State`, an `Action` enum and
`spawn(&Context) -> Handle` built on `hyprv-core` (`StateStore` topics, `ServiceRuntime`,
`run_sampled`). Purely event driven: D-Bus signals and PipeWire loop events. No
subprocesses, no polling; measured **0 context switches over 10 s idle** on the
service and PipeWire threads (`examples/idle_probe.rs`, live session, read-only).
Build needs `libfftw3` (headers + library) for the vendored cavacore.

```rust
let ctx = hyprv_svc_media::Context::new(store, runtime);       // session/system bus injectable
let cider = cider::spawn(&ctx);
let mpris = mpris::spawn(&ctx, Some(cider.requester()));
let spectrum = spectrum::spawn(&ctx, mpris::playing_gate(&ctx, &mpris.topic));
let audio = audio::spawn(&ctx);
let tray = tray::spawn(&ctx);
let notifications = notifications::spawn(&ctx);
// actions return a PendingAction that outlives the initiating popup:
let pending = audio.action(audio::AudioAction::Increase);
```

`Context { read_only: true }` observes only: every action is refused, no bus names are
owned and no StatusNotifierHost is registered. Tasks never capture the `Context`
(it holds the `ServiceRuntime`; see `docs/requests/core.md` item 1).

## Modules

### `audio` — `AudioState` / `AudioAction` (KDE model: plasma-pa)
One PipeWire connection on thread `hyprv-pw-audio`: registry globals, `default` metadata
(`default.audio.sink`), Props of every `Audio/Sink` node and Route params of
`Audio/Device`s. Reconnects with exponential backoff only while PipeWire is absent.

| QML (`shell.qml:3292`) | field |
|---|---|
| `Pipewire.ready` | `ready` |
| `audioAvailable` | `available` |
| `audioMuted` | `muted` |
| `audioVolumePercent` = `max(0, round(volume*100))`, may exceed 100 | `volume_percent` |
| Quickshell cubic `audio.volume` | `volume`, `channel_volumes` |

Actions mirror `hypr/controls.lua:152` (`wpctl`): `SetVolume(pct)` (clamp 0..100, 2
decimals, limit 1.0), `Increase` (+5 %, limit 1.0, clamps down from >100 %), `Decrease`
(−5 %, no limit), `ToggleMute`, `SetMute`. Channel balance is preserved (the cubic
average becomes the target). Writes go through the device `Route` (`save=true`) when
the sink has `device.id` + `card.profile.device`, like pipewire-pulse; otherwise node
`Props`. A reply means "request sent"; the observed Props confirm it.

### `spectrum` — `SpectrumState { running, values }`
Exactly the legacy `cava` process of `scripts/audio-spectrum.sh`, in-process:
- **cavacore vendored unmodified** from cava 1.0.0 (the installed `/usr/bin/cava`
  version; MIT, notice kept — `vendor/cavacore/`), built by `build.rs` with `cc`
  (`-std=c99 -O2`, like cava) and linked to the system `libfftw3`. `csrc/cava_shim.c`
  only adds field accessors so the vendor files stay untouched.
- **What cava's binary does outside cavacore**, replicated in `spectrum/cava.rs`:
  config parsing (`sensitivity` 45 → ×0.45 applied *after* cavacore; `noise_reduction`
  35 → 0.35; `monstercat` 1.4 with the filter's ×1.5; `sdl_glsl_gain` 1.0; the legacy
  `integral`/`gravity`/`ignore` keys are not parsed by cava 1.0.0 and do nothing);
  default PipeWire input (48 kHz, S16, stereo, sink monitor, `node.latency` 512/48000,
  always-process, virtual, FL/FR remix with upmix); `write_to_cava_input_buffers`
  (raw S16 values as doubles, zero-and-restart on overflow); the main loop's
  `samples_to_use` (not high-framerate at 48 kHz/24 fps → everything buffered), buffer
  shift, ×sens, clamp 0..1, ×`ascii_max_range`; per-channel `monstercat_filter` in
  `float` with the double-typed `max`; mono `average` `(l+r)/2`; `int` truncation; and
  `print_raw_out` clamping. Frames come from a 24 Hz loop timer like cava's
  `nanosleep` loop (frames with no new samples still advance smoothing).
- **Verified bit-exact** with `examples/cava_diff.rs`: identical PCM fed to the
  installed cava through a FIFO (lockstep: one 2048-sample chunk per printed frame)
  and to our runner. Results 2026-10-23: 45 s of `DR3_Multivocality.ogg` 2110/2110
  frames identical (2077 non-silent), `clip_0001.ogg` 278/278, synthetic sine+noise
  376/376; repeat run identical; max |delta| 0.

Capture runs only while the topic has subscribers **and** the gate is open
(`mpris::playing_gate`: selected player available and playing) — the QML condition
`mediaUiVisible && mediaAvailable && mediaPlaying`. Stopping drops the stream and its
buffer and publishes `values: []`, like `onRunningChanged`.

### `mpris` — `MprisState` / `MprisAction` → `MprisOutcome` (KDE model: mpris2 engine)
Subscribe-then-snapshot: `NameOwnerChanged` (arg0namespace), `PropertiesChanged` and
`Seeked` on `/org/mpris/MediaPlayer2`, then `ListNames` + `GetAll`. Signals for players
still being fetched are buffered and replayed. Owner restart = remove + rediscover.

| QML root (`shell.qml:166`) | field |
|---|---|
| `mediaAvailable`, `mediaPlaying` | `available`, `playing` |
| `mediaTitle`, `mediaArtist`, `mediaArtUrl` | `title`, `artist` (`", "`-joined), `art_url` |
| `mediaPlayerName` (bus name minus prefix) | `player_name` |
| `mediaLengthSeconds` | `length_seconds` |
| `mediaPositionSeconds` | `position.seconds_at(monotonic_us())` |
| `media-status.sh` `cider=` | `cider` |

Selection is `BarServices.qml:88` (playing → paused-with-content → content → paused →
first). The legacy `updateMediaState` rule is kept: a paused update without content for
the same player keeps cached title/artist/art/length/position. Position is a
`PositionAnchor` (µs, CLOCK_MONOTONIC instant, rate, playing, length) that the UI
extrapolates only while presenting media; the actor re-reads `Position` once on
playback/rate/track change, re-anchors on `Seeked`, and re-snapshots when core's `clock::SleepTopic`
`resume_generation` changes (the shell host runs `clock::run_sleep_topic` once). Actions: `PlayPause`/`Play`/`Pause`/`Next`/`Previous`
(capability-checked, sent to the **selected** player), `Seek(seconds)` (`seekMedia`:
clamp to [0,len], `SetPosition(trackid)` or relative `Seek`, local anchor set
immediately, one re-read after 180 ms), `Raise` (`Raise()` if `CanRaise`, otherwise
`MprisOutcome::FocusFallback { pid, class }` for the Hyprland actor — PID of the bus
owner or `…instanceNNN`, class `Cider`/`Spotify`, as `shell.qml:2890`), `Refresh`.
`playerctld` (a proxy that re-exports another player) is ignored.

Hold `mpris::present(&store)` (a `MediaPresentationTopic` subscription) while media UI
is presented (desktop `mediaVisible`; laptop `mediaUiVisible` = a bar expanded or the
control panel requested): rising demand triggers one Cider repair, and while held a
confirmed playing Cider gets the 30 s fallback. Position needs no read on reveal: the
event-maintained anchor is current, so the first presented frame is already right.

**Work attribution.** `Context::counters` (`Counters`, `snapshot()` →
`CounterSnapshot`) counts MPRIS actor turns, `Get Position` calls, Cider repair firings
and spectrum capture starts. With media playing but not presented (laptop compact bar)
all four stay constant and the service thread does not wake
(`tests/laptop_media.rs`).

### `cider` — `CiderState` / `CiderAction::Lookup` → `Option<CiderTrack>`
Exact port of `scripts/cider-metadata-fallback.py`: API v2 then v1 on
`HYPRV_CIDER_API_BASE` (default `http://127.0.0.1:10767`), 0.7 s timeout, first
without token then each `apptoken` (env `HYPRV_CIDER_APP_TOKEN`/`CIDER_APP_TOKEN`,
`~/.config/hyprv/cider-app-token`, `~/.config/HyprV/.cider-app-token`; tokens never
leave the module), candidate scoring, artwork `{w}{h}{f}{c}` formatting, then the
IndexedDB LevelDB structured-clone scan. A newer lookup cancels an in-flight one.
mpris drives it like `BarServices.qml:150-205`: 200 ms debounce after any selected
snapshot change for chromium/cider players, one queued repair, results applied only
to the same snapshot key, merge rules of `media-status.sh:102-118` (API replaces,
cache only repairs empty/"Cider"/"Chromium" fields), 30 s retry only while presented,
Cider confirmed and playing. Lookup only when the bus owner's `/proc/PID/comm` is
`Cider` and length > 0.

### `tray` — `TrayState` / `TrayAction`
Host `org.kde.StatusNotifierHost-<pid>-hyprv` registered with the existing watcher;
serves `org.kde.StatusNotifierWatcher` (KDE semantics: `service/path` keys, owner
tracking) **only if the name is free**, never replacing kded. Items: `GetAll` on
registration, coalesced re-read on any `New*` signal. `TrayItem` exposes Quickshell's
`id`, `title`, `status`, `tooltipTitle`, `tooltipDescription`, `onlyMenu`, `hasMenu`,
`icon` (themed name, `?path=<IconThemePath>` appended — the form `trayIconSource`
splits), raw ARGB pixmaps (validated, bounded) and registration order. Menus: `MenuOpen`
(`AboutToShow(0)`, `GetLayout`, `Event(0,"opened")`, live updates via
`LayoutUpdated`/`ItemsPropertiesUpdated` subscribed only while a menu is open),
`MenuAboutToShow`, `MenuEvent{clicked|hovered}`, `MenuClose`. Entries carry
`text` (mnemonics resolved), `enabled`, `visible`, `is_separator`, `button_type`,
`check_state`, `has_children`, `icon_name`, `icon_data`. Item actions: `Activate`,
`SecondaryActivate`, `ContextMenu`, `Scroll{delta, horizontal}`.

### `notifications` — `NotificationState` / `NotificationAction` → `Option<u32>`
SwayNC API verified by introspecting the deployed swaync 0.12.6 (the spec's guessed
`org.erikreider.swaync` path is wrong; the real one is name/interface
`org.erikreider.swaync.cc`, path `/org/erikreider/swaync/cc`). `GetSubscribeData`
snapshot on owner appearance, then `SubscribeV2` (legacy `Subscribe` if V2 is absent).
`alt`/`tooltip`/`text` reproduce `swaync-client -swb` byte-for-byte (checked against a
live capture); `dnd`, `has_dot` follow `shell.qml:500-501`. On owner loss `available`
goes false and the last alt/tooltip are kept (the QML stream just stops). Actions:
`ToggleVisibility` (`-t -sw`), `ToggleDnd` (`-d -sw`), `SetDnd`, `SetVisibility`,
`ReloadCss` (`-rs`), and freedesktop `Notify` (with the
`x-canonical-private-synchronous` hint) for shell messages.

## Timers that exist (all demand-scoped)
- reconnect backoff (0.5 s … 32 s + jitter) only while PipeWire / the session bus is absent;
- mpris: 200 ms Cider debounce (one-shot after a snapshot change of a chromium/cider
  player), 30 s Cider fallback (presented + confirmed Cider + playing), 180 ms one-shot
  position re-read after a seek;
- per-call 5 s timeouts on D-Bus method calls, 0.7 s per Cider HTTP request.

## Tests
`cargo test -p hyprv-svc-media` — unit tests (volume model and Route/Props pods,
cava FFI runner, Cider parsing/scoring/LevelDB scan/fake HTTP server, MPRIS selection,
anchors, caching and merge rules, dbusmenu/pixmap parsing and bounds, swaync format)
and 5 integration tests on a **private `dbus-daemon`** with fake peers (MPRIS player:
snapshot, PropertiesChanged, Seeked, actions, capability refusal, owner loss/restart;
SwayNC: late appearance, signal, ToggleDnd, loss; tray: watcher served when absent,
item registration, `New*` refetch, menu open/click/close, owner vanish; read-only
refusals). Live checks are `#[ignore]`d and read-only:
`cargo test -p hyprv-svc-media --test live_readonly -- --ignored --test-threads=1`.

## Review fixes (2026-10-23)
MEDIA-1…5 from `docs/reviews/hyprv-svc-media.md` are fixed; regression tests live in
`spectrum::tests` and `tests/review_regressions.rs` (private dbus-daemon, fake peers).

## Manual verification (not automated; changes user-visible state)
Run with a non-read-only context in a dev shell build:
1. Volume: `Increase`/`Decrease`/`SetVolume(40)`/`ToggleMute` → compare with
   `wpctl get-volume @DEFAULT_AUDIO_SINK@`; check a hardware (HDMI/ALSA) sink saves
   via Route (volume survives a sink re-plug), check a Bluetooth sink, check balance
   is kept after setting an unbalanced volume in pavucontrol.
2. Spectrum: play music with media UI shown → `values` move at ≤24 Hz; hide → stream
   node `hyprv-spectrum` disappears from `pw-cli ls Node`.
3. MPRIS: play/pause/next/seek on Spotify, Firefox and Cider; position stays in sync
   after seeking from the player itself (Seeked) and after suspend/resume.
4. Cider: with Cider playing, title/artist/art repaired within ~0.2 s of a track change.
5. Tray: kill kded6 → shell serves the watcher, apps re-register (some need restart);
   right-click menus (Telegram, WeChat, Claude), check/radio entries, submenus.
6. SwayNC: DND toggle and control-center toggle from the bar; `swaync-client -rs`
   equivalent `ReloadCss` after a theme switch.

## Gaps / open items
- **pavucontrol on right-click** (`shell.qml:4140`) is app activation — belongs to the
  `applications` actor, not here.
- **Raise fallback** needs the Hyprland actor to focus by PID/class (returned as
  `MprisOutcome::FocusFallback`).
- **Optimistic `mediaPlaying = !mediaPlaying`** on play/pause (`shell.qml:4110`,
  `ControlPanelPopup.qml:336`) is left to the UI; the service publishes observed state.
- Legacy `playerctl` without `-p` targeted playerctl's own choice; actions now go to
  the shell's selected player (consistent with what the UI shows).
- Spectrum parity is proven on recorded PCM through cava's FIFO input; live capture
  differs only in how PipeWire quanta fall between frames, exactly as two runs of the
  legacy cava differ. The stream is always-process while active (as cava's), i.e. it
  keeps the graph running only while the media UI is shown and playing.
- Tray `icon` for pixmap-only items is empty (Quickshell returns an `image://` URL);
  the UI must render `icon_pixmaps`. Bundled LAIS icon and `trayItemPriority`
  ordering/nm-applet filtering remain UI concerns.
- Volume writes are not verified on this machine (read-only rule); see manual steps.
- Cider HTTP/LevelDB paths are tested with fakes only; no live Cider was running.
- `hypr/controls.lua` mic actions (`toggle-mic`, `mic-inc/dec`) are not shell actions
  and are not implemented.
