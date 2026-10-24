# Isolated release-shell soak

These are verification tools, not production processes. No installation or live
session connection is used. Dependencies: bubblewrap/user namespaces, KWin,
Hyprland, private dbus-daemon, Python dbus + GI, C compiler, wayland-scanner,
wayland-client development files; NVIDIA accounting is optional. `grim` provides
selected private-session screenshots.

Build normally as the desktop user:

```sh
cargo build --release --locked -p hyprv-shell
python3 docs/verify/soak-tools/run.py --seconds 3720 --out docs/verify/raw/soak/NEW-RUN
```

Once `NEW-RUN/status.json` says `running`, use a second terminal for the side
hotspots (the clock hotspot intentionally keeps the island compact):

```sh
python3 docs/verify/soak-tools/extra-input.py docs/verify/raw/soak/NEW-RUN
```

To replay the exact tested executable after the workspace changes, add
`--binary docs/verify/raw/soak/release-62m/hyprv-shell-release` to `run.py`.
The original launch driver is preserved under the run's `tools-at-launch/`.

Then analyze the completed evidence:

```sh
python3 docs/verify/soak-tools/analyze.py docs/verify/raw/soak/NEW-RUN
```

`run.py` refuses root. It creates a fresh output directory and copies the release
executable, whose SHA256 is recorded. It launches its private bus/compositors/shell
inside user/PID/network/IPC/UTS namespaces, with a read-only root filesystem and
only its output/private runtime writable. No inherited session environment is
used. No D-Bus activation dirs are configured; both service buses point at the
test bus. Shell actions are disabled by `--read-only`; the driver only sends UI
IPC/input plus workspace/output commands to the explicit private compositor
socket. The shell process stays alive across the whole timed interval.

Every 180 seconds the driver cycles calendar morph/scroll, control panel,
OSD previews, twelve agent sessions, media owner/playback state, workspace,
secondary output hotplug and mock logind sleep/resume. The supplementary driver
verifies actual stats and expanded island layer geometry, and scrolls the agent
list. Do not count a successful pointer command alone as proof of a UI transition.

Metrics: 5-second `/proc` samples (RSS, PSS, private dirty, fds/types, threads,
CPU ticks, kernel timerfds/arming, layer snapshots, IPC latency); 60-second
per-process NVIDIA graphics accounting. The LD_PRELOAD library observes
`eglMakeCurrent` → swap wall time, swap duration, and timerfd create/settime.
This includes CPU drawing/submission and driver waiting, excludes preceding
scene construction and GPU/presentation completion, and is **not** a whole-frame
presentation benchmark. It adds instrumentation overhead and one trace fd.
Logical Tokio timer cardinality is unavailable through this interface.

The analyzer retains all raw samples and uses comparable settled cycle phases
for growth screening, after the supplementary workload has had a complete cycle.
It reports slopes/ranges and separates large growth screens from small monotonic
retention observations. Neither automatically proves a leak. Frame quantiles
exclude the first 120 seconds; per-window tables retain the full run.

The original run's supplementary input began at 427.9 seconds, after visual
inspection identified that the original center-clock hover did not expand apps.
Its frozen launch tools, separate supplementary source, logs and source hashes
are retained with the raw run. Artwork downloads/decoding, real PipeWire/audio,
NM/BlueZ devices, hardware suspend, fractional DPR and physical input are outside
this workload.
