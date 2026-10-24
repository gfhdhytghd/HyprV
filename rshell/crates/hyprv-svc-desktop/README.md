# hyprv-svc-desktop

Desktop-domain service actors for the Rust shell: **agent_island, deskflow, viewflow, ir,
egpu, demo, recording, theme, applications, startup**. They replace the Python agent stack
and the shell scripts the QML panels used to poll.

All of them follow the same rules:
- Purely event-driven: inotify, kernel uevents, pidfds, D-Bus signals and the Hyprland
  sockets. A process scan runs only while a lease is held.
- No fork/exec at runtime. Apps and the recorder are started by the systemd user manager
  as transient units.
- Measured idle cost is zero wakeups (see *Verification*).

## Wiring

```rust
let ctx = hyprv_svc_desktop::Ctx::new(store, runtime_handle, shutdown_rx); // Env::from_process()
let s = hyprv_svc_desktop::spawn_all(&ctx);
let sub = s.deskflow.subscribe();                         // hyprv_core Subscription<DeskflowState>
s.deskflow.dispatch(DeskflowAction::Set(Target::MacOs))?; // fire-and-forget, survives popup close
let out = s.ir.request(IrAction::List).await?;            // or await the result (legacy stdout text)
```

- Each module exposes a typed `XxxState` (serde), an `XxxAction` enum, a `TopicKey`
  marker (`XxxTopic`) and `spawn(ctx) -> XxxHandle`. The handle is a
  `Handle<State, Action>`, which wraps a `hyprv_core::store::Topic` and a bounded queue.
- Errors are `ServiceError { code, message, retryable }`. Messages match the legacy
  stderr text where the source had one, Chinese strings included.
- `Lease` (`agent_island.scan_lease`, `recording.scan_lease`) is a demand-only topic.
  Holding `lease.acquire()` turns on the 1.5 s process scan. Dropping it stops the scan.
- `demo::spawn(ctx, inhibitor)` takes a `SleepInhibitor`. The shell should pass an adapter
  over svc-power's `inhibit` actor. The default holds its own logind `Inhibit` FD.
- `viewflow::spawn_with(ctx, ir, resolver)` takes a `PeerResolver` (see Gaps).

## Modules: state ↔ QML, actions ↔ legacy commands

| Module | State (QML name) | Actions (legacy) | Event sources |
|---|---|---|---|
| agent_island | `active`, `count` (`agentIslandActive/Count`). `sessions[]` with `id, source, source_label, title, project, status, current_tool, detail`, plus `base_progress`/`activity_epoch_s`. `AgentSession::progress_at(now)` is the QML `progress` (12 s phase, capped at 0.94; `pinned` running ProcessScan rows show `base_progress`, as the legacy re-anchored scan did). Also `pending_count`. | `FocusSession(id)` (`agent-island-action.py focus`). No approve/answer/type exists. | inotify on `state.json` only; a pidfd per referenced PID; one next-expiry deadline (20 s / 900 s / 86400 s). Scan lease: the legacy 1.5 s scan (`ScanClock`) over an incremental `/proc` table (IDs allocated since the last probe per `/proc/loadavg`, pidfds for agent-named PIDs, full readdir sweep every 60 s); the scan's effect is recomputed only when the legacy view would observably change. Rows are in legacy dict order (`LegacyOrder`); without the lease, discovered rows expire 900 s after the last scan, as legacy's did. Parity tests replay `tests/fixtures/agent_parity` against the legacy Python (`tests/agent_parity_legacy.py`) |
| deskflow | `target` (windows/macos from the config regex), `busy`, `last_error` | `Set(t)` (`set`), `Reflow`, `DisplaySwitch(t)` | inotify on `deskflow-server.conf` |
| viewflow | `target` (`status`: `hdmi-target`, else the route), `hdmi_selected`, `input_route`, `topology_supervised`, `peer_ready`, `busy` | `Set(t)` | inotify on the selector files, config and runtime readiness files; unit `ActiveState` streams |
| ir | `device` / `device_error` (exactly one Pico), `carrier_hz`, `duty_cycle`, `buttons[]`, `busy` | `Send`, `Learn{force}`, `Scan{seconds}`, `SetPin`, `Status`, `Show`, `List` (hdmi-switch-ir.py) | inotify on `/dev/serial/by-id` and the database |
| egpu | `devices[]{bdf,label,externally_verified,removal_chain}`, `last_results`, `busy` | `Remove(None\|Some(bdf))`. The reply is the legacy `device=…/status=…` text | kernel uevents (`pci`, `thunderbolt`) |
| demo | `enabled` (`demoModeEnabled`), `source`, `headless`, `created_headless`, `acquired_inhibitor`, `busy` | `Enable`, `Disable`, `Toggle` (status text identical) | inotify on the journal dir |
| recording | `active` (`screenRecording`), `recorders[]{pid,name,managed}` | `Toggle`, `Start`, `Stop` | pidfds; transient-unit cgroup; scan lease |
| theme | `theme`, `variant`, `dark_mode` (`darkMode` = literal `theme=dark` line), `fluent_icons{light,dark,base}`, `application_errors` | `ToggleTheme`, `SetTheme`, `SetVariant`, `Apply` | inotify on `ui-state.env` |
| applications | `applications[]{desktop_id,name,exec,icon,categories,terminal}` (app_fetcher order and dedupe) | `Activate(id)`, `ActivateByName`, `Launch(LaunchTarget)`, `OpenUri` | inotify on the XDG application dirs |
| startup | `approval_path`, `decided`, `approved` | `Decide(bool)`. Writes `launch\n` only on an explicit yes | none |

Notifications: deskflow, viewflow, demo and theme send the same `notify-send` messages the
scripts and QML sent, through `org.freedesktop.Notifications`. **The UI must not send them
again.**

`LaunchTarget` covers the `shell.qml` runDetached sites:
- launcher (rofi), lock (hyprlock), logout (wlogout), btop terminal, pavucontrol
- network editor and Bluetooth manager, with the same fallback chains as the source
- `viewflow-windows start --anchor x y` / `menu`

## Runtime execs (policy: one-shot, user-initiated, legacy parity)
Each of these runs only inside an explicit user action, never on a timer or for status. Each is exactly what the legacy shell did.

| Action | Exec | Legacy source |
|---|---|---|
| Theme apply (`theme/legacy.rs`) | `xsettingsd` restart: SIGTERM to the running process via pidfd, then detached `xsettingsd` when `$DISPLAY` is set | `apply-ui-state.sh` `restart_xsettingsd` |
| Theme apply | `plasma-apply-colorscheme Orchis\|OrchisDark` (5 s timeout, result ignored) | `apply-ui-state.sh` |
| Theme apply | detached `awww-daemon` if none is running (then a 0.2 s wait), then `awww img BG --transition-fps 180 --transition-type wipe --transition-duration 2` | `apply-ui-state.sh` |
| Theme apply | WeChat appearance: `xdotool search/getwindowname/key/mousemove/click/windowclose` and `xwininfo -id`, with the same coordinates, titles, size windows and waits. Hyprland queries and dispatches use the socket. | `apply-ui-state.sh` `sync_wechat_appearance` |
| Recording start | transient user unit running `~/.config/hypr/scripts/record-script.sh --fullscreen-sound` (systemd execs it; the script itself uses hyprctl/pactl/wf-recorder) | `ControlPanelPopup.qml:230` |

Everything else stays native:
- Kitty uses its remote-control socket plus SIGUSR1.
- Ghostty, setcursor and focus go through the Hyprland socket.
- SwayNC uses `ReloadCss`.
- XFConf and dconf are written over D-Bus.
- The activation environment is set over D-Bus.
- Konsole uses D-Bus.
- GTK/Qt/Kvantum/kdeglobals/konsolerc are edited as files, with exact awk semantics.

## Agent hook contract

`hyprv-agent-hook --source claude|codex [--event E]` reads hook JSON on stdin. It is a
drop-in for `agent-island-hook.py`:
- same state dir (`$HYPRV_AGENT_ISLAND_DIR`, default `~/.cache/hyprv/agent-island`)
- same `state.lock` flock and the same compact `state.json` (version 1, unknown fields kept)
- observation-only: a pending request is dropped immediately
- always exits 0

The active window comes from the Hyprland socket instead of `hyprctl`. Python hooks and the
Rust hook can coexist.

`hyprv-agent-hook install [--python]` ports `agent-island-install-hooks.py`:
- same Claude/Codex events, timeouts, matchers and `statusMessage`
- same Codex `[features] hooks = true` edit
- replaces entries carrying either marker (`agent-island-hook.py` or `hyprv-agent-hook`)
- `--python` installs the exact legacy command
- **Safety** (fixes for review DESKTOP-1..7):
  - Installers serialize on `$XDG_CACHE_HOME/hyprv/agent-hook-install.lock`.
  - All three documents are read and transformed first. Any read, UTF-8 or parse error aborts with nothing written.
  - Each replace is a compare-and-swap against the bytes that were read. A concurrent edit is rebased: the managed-hook edit is re-applied onto the newer document. If it keeps changing, the install stops.
  - `config.toml` is edited with `toml_edit`, so comments, order, quoted/dotted/inline tables and multiline strings are kept. The result is re-parsed before it is written.
  - Hook paths are `shlex.quote`d. Ordinary paths keep the exact legacy text.
  - A managed entry must be exactly `…/hyprv-agent-hook --source S` or `…/python* …/agent-island-hook.py --source S`. A command that merely mentions a marker is kept.
  - Backups are `NAME.hyprv-bak.<ms>.<rand>`, created with `O_EXCL|O_NOFOLLOW`.
  - Temporary files are exclusive and random, with the mode set before any data, fsynced, and renamed through a held directory fd. File modes are preserved; new files are 0600.
  - Theme edits share the same writer, and their read errors abort.
- **No diff noise**: documents are edited as order-preserving JSON (`ojson.rs`).
  - Existing keys keep their position and new keys append, which is Python `dict` semantics.
  - New hook entries use the Python key order.
  - Output keeps the file's own indent and trailing-newline style.
  - A file is written only when its bytes change. A reinstall writes nothing and leaves no backup.
  - A versioned backup is kept only when a write happens, and the file mode is preserved.
  - A test checks that the real `~/.claude/settings.json` and `~/.codex/hooks.json` round-trip byte-identically; it only reads them.

This is explicit maintenance. It is never run at startup.

**Codex app-server adapter** (`appserver.rs`) ports `agent-island-codex-appserver.py`.

The shell attaches to a running app-server and never spawns one:
- Endpoint: `$HYPRV_CODEX_APP_SERVER_SOCK`, or `$CODEX_HOME/app-server-control/app-server-control.sock`. That is the managed daemon's `--listen unix://` socket, which speaks JSON-RPC over WebSocket.
- It sends the same `initialize` request.
- It applies `thread/started`, `thread/status/changed` and `thread/closed` exactly as the Python did: `codexapp:` ids, the status/detail/progress table, title/cwd/preview/transcript_path.

Reconnecting:
- While the endpoint is absent, the adapter waits on inotify with no timer.
- After a disconnect or a refused connect, it retries after 4 s (the Python's interval), backing off to 5 min.
- Any change to the endpoint resets the backoff.

The actor is **read-only by default**: no lock, no writes. Process-scan sessions live in an
in-memory overlay. `agent_island::spawn_with(ctx, true)` restores the legacy write-back
observer.

## Verification

Automated, all read-only (`cargo test -p hyprv-svc-desktop`): 45 unit tests and 5 live tests.

Unit tests (mocked transports):
- agent model parity with the Python logic
- hook installer
- read-only overlay, using a fake `/proc`
- Kitty RC framing over a fake socket
- Hyprland wire format over a fake socket
- IR serial protocol over a socketpair
- NEC decode
- deskflow template, checked byte-for-byte against the live config
- eGPU topology, on a fake sysfs
- demo monitor math and journal
- recording `pgrep` pattern
- the awk-exact INI/assignment/xsettings editors
- dconf GVariant changeset, byte-identical to GLib
- Exec field codes
- app index rules
- name-filtered inotify
- startup approval

Live tests (`tests/live_readonly.rs`):
- all actors publish on the real session (write-capable dirs redirected to temp)
- the agent island follows an atomic `state.json` write
- the hook binary end-to-end
- read-only Hyprland and systemd queries

### Agent-island cost (2026-10-23)
**Setup.** Live hook state: 173 KB, 109 sessions, rewritten ~2×/s by running agents and the legacy observer. The method is the same as `docs/verify/perf.md`:
- `verify.py`'s `proc_snapshot`/`delta`
- 8 s warm-up, demand released at 5 s
- desktop group only: `--exclude net,media,power,sys`

Probe snapshots are private, in a scratch copy of `.check`: *before* is the probe's last snapshot, *after* is this source. Built with `--features desktop`. Each row is 3 interleaved 30 s windows. Raw rows are in `perf/agent-island-2026-10-23.json`.

| build | mode | before CPU % | after CPU % | before → after sched/s | probe-process ms per publication |
|---|---|---|---|---|---|
| debug (as perf.md) | live | 3.381 (2.43–3.92) | 0.920 (0.87–1.00) | 20.4 → 14.6 | 21.7 → 5.6 |
| release | live | 0.665 (0.57–0.80) | 0.211 (0.17–0.23) | 20.8 → 13.0 | 3.71 → 1.41 |
| release | agents idle (frozen state) | 0.000 | 0.000 | 0 → 0.15 | – |

**Probe overhead.** The probe's own work is included in those numbers:
- a mirror task that re-reads and rewrites the 173 KB file on every directory event
- ~52 KB of JSONL serialized per publication

**Actor alone.** `cargo run --release --example agent_cost -- 30` on the live directory: 22.9 ms CPU in 30 s, which is **0.076% of a core** for 26 publications. With a frozen state it is 0 timeslices.

**Per event** (`--example agent_bench`, release, live file):

| path | cost per event |
|---|---|
| old full path (`Value` parse + two full serializations) | ~1.55 ms |
| new: one changed session | 0.18 ms |
| new: unchanged bytes | 0.05 ms |
| debug old path | 12.7 ms |

**Changes that got there:**
- The read-only mode no longer serializes the whole state twice.
- The top level is parsed into borrowed `RawValue`s, and only sessions whose raw text changed are parsed again (`agent_island/incremental.rs`, checked against the full path in a test).
- Display rows are cached per session.
- Liveness comes from the pidfd set instead of `stat`.
- The refresh runs inline: no blocking-pool hop, no lock in read-only mode.
- At most one refresh and publication per 16 ms frame-equivalent, and only if the state changed.
- Row ordering has a deterministic tie-break, so an unchanged state never republishes.
- There is one long-lived inotify watch per actor that re-arms itself after atomic replaces, so directories are never rescanned per event.
- Process watches are pidfds, and the scan runs only under the lease. Transcripts are not watched.

Idle cost: `cargo run -p hyprv-svc-desktop --example idle_probe -- 20` measured **0**
voluntary context switches on the `hyprv-services` thread over 20 s with every actor running.
`PROBE_ONLY=agent,viewflow,…` isolates modules.

### Manual verification (performs real actions — run deliberately)
1. **IR**: `IrAction::Status`, then `Send("input1")` / `Send("input2")`. Confirm the HDMI
   switch changes input. Compare with `hdmi-switch-ir.py send input1`.
2. **Deskflow**: `Set(MacOs)` with `deskflow.service` running. Check:
   - `~/.config/Deskflow/deskflow-server.conf` is mode 0600 with `right(..) = linhaikuodeMac-mini.local(0,100)`
   - `~/deskflow.log` gains "reloaded configuration"
   - IR input2 fires
   Then `Set(Windows)`. Stop the service and check `Set` succeeds without SIGHUP.
3. **Viewflow** (topology supervisor active): `Set(MacOs)` should send IR, write
   `~/.local/state/hyprv/hdmi-target`, and return `macos\n`.
4. **Demo**: on a machine with an external output, `Toggle` twice. Verify:
   - the portrait mode
   - the headless mirror (`hyprctl monitors`)
   - touch transform reset
   - the inhibitor (`systemd-inhibit --list`)
   - full rollback, and the journal dir removed
5. **Recording**: `Toggle` starts `hyprv-recording-*.service`, and the state shows the
   recorder. `Toggle` again sends SIGINT; check the mp4 is finalized and the unit is gone.
6. **Theme**: `ToggleTheme` twice. Check:
   - symlinks
   - gtk/qt/kvantum INI files
   - `gsettings get org.gnome.desktop.interface color-scheme`
   - Kitty/Ghostty/SwayNC/Konsole reloads
   - `application_errors` lists only the unsupported items (see Gaps)
7. **Apps**: `Launch(Lock)`, `Activate("org.gnome.Nautilus.desktop")`. The app should run in
   an `app-hyprv-*` unit (`systemctl --user list-units 'app-hyprv-*'`).
8. **eGPU**: needs the privhelper method below. Use the two-step confirm in the panel.
9. **Agent focus**: with a Claude session in Kitty/Ghostty, `FocusSession(id)` focuses its window.

## Gaps
1. **eGPU**: `hyprv-privhelper` now implements `RemoveExternalGpu(s) -> a(ss)`, with strict BDF validation, a sysfs revalidation, and polkit action `org.hyprv.shell.egpu-remove`.
   - Root refuses devices that are external only by the legacy bus>=3 heuristic. They need Thunderbolt ancestry or the kernel `removable` attribute. Such devices show `externally_verified=false`, and removal returns `InvalidArgs`.
   - The packaging is not installed, so live removal is untested.
2. **Viewflow peer discovery**: Windows configs with `peer_discovery` need SSH identity verification, which has no native adapter. This only matters when `viewflow-display-topology.service` is inactive.
3. **Sleep inhibitor**: the default demo inhibitor is private. Until the shell wires svc-power's inhibit actor, the prevent-sleep tile won't show it. A legacy `prevent-sleep.pid` is not honoured.
4. **Recording** keeps the record-script exec (see the table above). Native capture is not ported.
5. **Untested live**:
   - Hyprland mutating requests (`eval`, `output create|remove`, `reload`, `setcursor`, Lua `hl.dsp.*`)
   - the theme execs (WeChat automation especially)
   - a live app-server thread notification. The handshake and `initialize` were verified read-only against the running daemon.
6. Desktop entries: no `TryExec`/`OnlyShowIn`/localized names, matching app_fetcher. `Terminal=true` apps run via `alacritty -e`.
7. None of the real actions were exercised, by task policy. See *Manual verification*.
