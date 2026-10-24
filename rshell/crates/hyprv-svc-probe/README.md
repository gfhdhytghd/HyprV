# Read-only service probe

Starts the buildable net, media, power, sys and desktop modules on one core runtime/store. Emits initial and changed typed topic state as JSONL with timestamps/revisions. There are no dispatch/action calls. Missing providers and deliberately disabled read paths emit explicit unavailable records.

From rshell root:

```sh
python3 crates/hyprv-svc-probe/scripts/build.py build --release
crates/hyprv-svc-probe/.check/target/release/hyprv-svc-probe --duration 20 --idle-after 5
python3 crates/hyprv-svc-probe/scripts/verify.py equivalence
python3 crates/hyprv-svc-probe/scripts/verify.py idle
python3 -m unittest discover -s crates/hyprv-svc-probe/scripts -p 'test*.py'
cargo test --manifest-path crates/hyprv-svc-probe/.check/Cargo.toml -p hyprv-svc-probe
```

The build script snapshots dependencies into an ignored owned workspace, preserving the shared root Cargo.lock while other agents work. Source SHA256s are saved under docs/verify. Build/run again when owners change their modules. Optional Cargo features support partial builds; exported desktop/media modules are detected at build time. `--exclude` accepts crate groups (`net`, `media`, `power`, `sys`, `desktop`) or `agent_island`/`viewflow` for matched-binary isolation. `active`, `no-agent` and `isolation` verifier modes are additional diagnostics; the required baseline is 60-second `idle`.

Idle drops demand-driven sampling subscriptions after five seconds while retaining event subscriptions. The shell/desktop is never hidden or restarted. Spectrum's capture gate stays false. MPRIS has no Cider lookup requester. Media/net observation mode prevents watcher/agent registration. Fan starts only when its helper name is already owned, avoiding privileged autoactivation. Backlight only reads sysfs/DDC Get VCP; no Set VCP or luminance/persistence actions are sent. Battery history, theme initialization, demo journals and agent state use probe-owned storage. Agent inputs are mirrored with a filtered watch and content deduplication.

The verifier locates the live main Quickshell config and imports only four session environment keys. It audits/runs original status branches, shadows cleanup-capable legacy storage, and reads provider properties directly for native QML paths. Calendar RPCs use an existing same-user socket and correlate IDs past the protocol greeting. It never activates DankCal, invokes UI RPCs, sends serial commands or controls services.

Reports: [equivalence](../../docs/verify/equivalence.md), [coverage](../../docs/verify/coverage.md), [performance](../../docs/verify/perf.md). Scheduling/context-switch counts are proxies, not exact kernel wakeups. Raw endpoints, source fingerprints and asynchronous reference timestamps retain evidence boundaries. Missing/populated-provider branches and visual/action parity remain explicitly unverified.

Performance defaults to the release executable. `--binary PATH` (or `HYPRV_PROBE_BINARY`) selects a frozen executable, `--label` preserves before/after raw filenames, and `--exclude`/`--seconds` support diagnostics. For example:

```sh
python3 crates/hyprv-svc-probe/scripts/verify.py idle --binary /absolute/probe-release-before --label release-before
python3 crates/hyprv-svc-probe/scripts/verify.py idle --binary /absolute/probe-release-after --label release-power-after --exclude net --exclude media --exclude sys --exclude desktop
```

Never compare instrumented tracing runs to ordinary release runs. Current repair evidence is in [idle-cpu-repair.md](../../docs/verify/idle-cpu-repair.md).

`--freeze-agent-input` is an explicit performance fixture: the agent actor stays running with one copied state snapshot; live hook mirroring is disabled and its app-server endpoint is redirected into the probe's absent fixture path. Normal runs always follow live inputs. Report frozen-input and live results separately; do not use the fixture to claim live event-processing costs disappeared.
