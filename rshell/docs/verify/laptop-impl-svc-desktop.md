# Laptop Phase B — svc-desktop implementation report (2026-10-24)

Request: `docs/requests/hyprv-svc-desktop.md` "svc-power → svc-desktop: laptop CP exact-name
recorder input"; spec `docs/spec/laptop-panels.md` §1.5 (`screenRecording` = `pgrep -x`
on exact names, event-driven in rshell).

## Result

| item | status |
|---|---|
| Laptop `screenRecording` uses `pgrep -x` semantics on the kernel comm | **DONE** (`Match::Comm`) |
| Event-driven, no polling on laptop | **DONE**: fanotify exec + inotify + pidfd + a lease snapshot |
| Desktop detection unchanged | **DONE**: the `Match::Cmdline` path is the previous code; no new source is active |
| Fake-process tests: wrapper argv false, exact comm true, truncated long comm false | **DONE** |
| `recording::spawn(ctx)` honors the shared latched `Profile` | **DONE**: no duplicated detection |

## Semantics
- **Laptop (`Profile::Laptop` → `Match::Comm`):** a process counts iff its
  `/proc/PID/stat` comm (the same value as `/proc/PID/comm`) is **exactly** `wf-recorder`,
  `wl-screenrec` or `gpu-screen-recorder`.
  - argv is never consulted, so wrappers (`sh -c wf-recorder`, `exec -a wf-recorder sleep`) do
    not count.
  - The kernel's 15-byte comm limit is preserved. `gpu-screen-recorder` runs as
    `gpu-screen-reco` and never matches, as with `pgrep -x` on the legacy laptop script.
  - A renamed process whose comm is exactly a recorder name does count.
- **Desktop (`Profile::Desktop` → `Match::Cmdline`):** unchanged.
  - Detection is still `pgrep -f '(^|/)(name)( |$)'` on the command line.
  - The 1.5 s lease-scoped scan, pidfd exits, the initial snapshot, start/stop and
    `wait_for_recorder` work as before.
  - The new select branches resolve to `pending` there: no exec source is created and no
    follow-up timers exist.
- **Stop:** unchanged on both profiles (pkill-style 15-byte comm, source order).

## Laptop event sources (no period anywhere)
1. **Exec:** unprivileged fanotify `FAN_OPEN_EXEC` inode marks on every `PATH` directory
   entry named like a recorder.
   - `PATH` comes from the shell env, falling back to `/usr/local/bin:/usr/bin`.
   - The fd is waited on with `AsyncFd`; readiness is cleared only on EAGAIN.
2. **Late comm:** the event is queued when the binary is opened, before the kernel sets
   the new comm.
   - Each exec event is answered by a scan now plus one-shot follow-ups at 50 ms, 250 ms
     and 1 s.
   - These are bounded per event, not a period.
3. **Installs and upgrades:** a long-lived inotify watch on those `PATH` directories,
   filtered to the recorder names.
   - On create, rename or delete, the current inodes are re-marked (an upgrade is a new
     inode) and one scan runs.
4. **Exit:** a pidfd per detected recorder (existing code).
5. **Panel lease:** acquiring `scan_lease` takes one snapshot. This catches recorders
   started outside `PATH` or renamed by prctl. There is no 1.5 s tick on the laptop.
6. **Degradation:** if fanotify is unavailable (kernel < 5.13), it is logged once. Then
   only the startup snapshot, lease snapshots, and the action path remain. Still no polling.

## Tests (`crates/hyprv-svc-desktop/src/recording.rs`)
- `comm_is_exact_and_never_argv`: exact names; the truncated `gpu-screen-reco`; trailing
  space, prefix, suffix and case variants.
- `profiles_select_their_own_semantics_over_one_proc_tree`: a fake `/proc` with seven
  processes.
  - Laptop finds `{exact, wl-screenrec, renamed}`.
  - Desktop finds `{exact, /usr/bin/wf-recorder argv, gpu-screen-recorder, wl-screenrec}`,
    i.e. the old results.
  - It also checks the `Profile` → `Match` mapping.
- `real_processes_exact_comm_wrapper_and_truncation`: real children.
  - Exact comm true: a copy of `sleep` named `wf-recorder`.
  - Truncated long comm false: `gpu-screen-recorder` → `gpu-screen-reco`.
  - Wrapper argv false: `exec -a wf-recorder sleep`.
  - The desktop matcher still matches all three.
- `laptop_detection_is_event_driven`: the real actor (`spawn_with(Match::Comm)`) with a
  scratch `PATH` and no lease.
  1. An exec of a marked binary is detected, and its exit is seen.
  2. A binary installed after start, and an upgraded binary (new inode), are re-marked
     and detected.
  3. An exact-comm recorder outside `PATH` stays unseen for 4.5 s, which is more than two
     of the old 1.5 s / 2 s poll periods. Acquiring the lease finds it.
  4. A script whose interpreter is a marked binary execs an unmarked exact-name recorder
     0.4 s after the event. Only the follow-up scans find it.
  - Each step first waits out the previous step's follow-ups.
- **Mutation checks**, each reverted afterwards:
  - with no exec branch the test fails with "exec not detected";
  - with no re-mark on directory change it fails with "install not marked";
  - with no follow-ups it fails with "comm set after the exec event was missed";
  - with the desktop tick enabled on the laptop it fails with "found without an event:
    something polls".
- **Stress:** `recording::` ran 20/20 times with `--test-threads=8` under 8 niced busy
  loops.
- `cargo test -p hyprv-svc-desktop`: 87 unit (2 opt-in ignored), 2 race and 5 live
  read-only tests pass. `cargo clippy -p hyprv-svc-desktop --all-targets` is clean.

## Also in this change
- `util.rs`: `impl AsRawFd for ExecWatch`. This is additive; the agent island does not
  use it.
- `theme/mod.rs`: the RAM-stick RGB worker (desktop `ENE DRAM` only) is not started on
  `Profile::Laptop`. Without this, each laptop theme change would try to run `openrgb`.
  Desktop behavior is unchanged.

## Files (only `crates/hyprv-svc-desktop`)
```
10385072b67d8940c572b78b4637a2dcbc9cfb9b5c714ca5d07e096299fb9cc2 src/recording.rs
421518c56dfe9bf59c2608f8826f2b526ab3d645a08a624d9c6ede4e435ead52 src/util.rs
d92c03861cbdc2606be9e0ba3468fd2bfc9a70b93881b27dbc50f56eb6a1a586 src/theme/mod.rs
6c826630c289789e572b8a7226fba1ba777f0480fba2f495e3adffe139844bed src/theme/ram_rgb.rs (rustfmt only)
```
- The public API is unchanged: `recording::spawn(ctx)` and `RecordingHandle`.
- New public items: `Match`, `comm_matches` and `spawn_with`.
- Host forwarding of `screen_recording` to the laptop control panel is engine's (filed
  separately by svc-power).

No release was built or installed (another publisher's `build_release.py` was running).
No sudo, no commit, no live session, compositor or helper actions; tests used scratch
binaries and processes only.
