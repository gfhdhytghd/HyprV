# parity/laptop — laptop (Dell XPS) profile references, fakes and e2e scaffold

Plan: `docs/verify/laptop-qa-plan.md`. Specs: `docs/spec/laptop-*.md`.

## Layout

```
gen-laptop-fixtures.py     fixtures/<component>/*.json (JSON is the source of truth)
capture-laptop.sh          export origin/laptop@24972f7 (read-only, temp dir) -> ../capture.sh
fixtures/                  laptop-bar, laptop-panels, laptop-island-legacy
reference/                 rendered PNGs + manifest.json (source_ref, profile: laptop)
b1-desktop-identity.py     gate B1: desktop recapture with the extended mocks == published
fakes/
  fake-system-bus.py       UPower, PowerProfiles (both names), login1 on a private bus;
                           org.hyprv.Test.Step/State scenario driver for the nested suite
  make-sysfs.py            fake /sys tree (power_supply, backlight, thermal, hwmon, dmi)
  hypr/controls.lua        stub hypr.controls (logs brightness calls; real plan() mapping)
  scenarios/*.json         battery/profile/lid/thermal/DMI worlds for both consumers
e2e/run-laptop-e2e.py      fakes self-test + lap.* cases -> docs/verify/laptop-e2e.md
verify-laptop.py           rshell replay vs reference (desktop verify.py gate) -> docs/verify/parity-laptop.md
```

## Commands

```sh
parity/laptop/capture-laptop.sh                      # all laptop components, scales 2 1, both themes
parity/laptop/capture-laptop.sh laptop-bar lap-compact-idle
PARITY_OUT=$S/b1 parity/capture.sh && python3 -I parity/laptop/b1-desktop-identity.py $S/b1
python3 -I parity/laptop/e2e/run-laptop-e2e.py
```

Screen: `eDP-1` 2880×1800 at scale 2 (1440×900 logical). Crops: bar `[0,0,1440,70]`,
bar + island `[0,0,1440,300]`, panels the whole output.

## Gate status of each component

| component | gated | why |
|---|---|---|
| `laptop-bar` | yes | adaptive bar, battery states, island hidden while compact (`laptop-island.md` §4) |
| `laptop-panels` | yes, **inside each panel's region** (`verify-laptop.py --regions`) | CP without fans, battery popup, QuickAdjust at the laptop geometry. References are whole-shell frames; panel replays render panel surfaces only, so the replay supplies `{file: [x,y,w,h]}` and only that rect is compared (CP card: `[579,273,282,354]`) |
| `laptop-island-legacy` | **no** | legacy interactive pending card; D1 = read-only (user, 2026-10-24) makes these expected divergences, kept for the record |

The island itself is gated by the existing desktop `island` references: the laptop
DynamicIsland differs from main's only in the withdrawn D1 deltas.

## Scene construction (laptop captures only)

`capture-laptop.sh` sets `PARITY_SCENE_IN_WINDOW=1`. parity-host then builds the scene inside the scale-N window
(`beginCreate` → parent into the window → render target → `completeCreate`). Mock window contents start under the
scene's `earlyHolder` (`ParityState.earlyParent`).
- **Why:** items created during startup must load at the output DPR, as Quickshell does on the XPS (qApp DPR 2):
  - the laptop CP page Loader, which is always active;
  - the strip/tray icons;
  - tooltip layers.
  Window-less construction made them load at qApp DPR 1. That gave 44 px instead of 88 px SVG rasters and soft 1×
  layers (bisected in `docs/requests/hyprv-ui-panels.md`).
- **Desktop captures** keep the original window-less construction, so the signed-off desktop set stays
  byte-identical (gate B1). The desktop references carry the same DPR-1 artifact for startup-loaded images (98 bar
  and 6 island images at @2); this is recorded and was not changed.

## Mock extensions made for the laptop QML (all additive; gate B1 proves the desktop set unchanged)

- `Quickshell/PopupWindow.qml`: bar tooltips. The scene stacks popups above every layer.
- `Quickshell/Networking`: `DeviceType`, `NetworkBackendType` and `WifiSecurityType`.
  - Enum order comes from `quickshell-network.qmltypes`; labels come from the quickshell
    binary.
  - Device/network objects come from `services.wifi.devices`.
- `Services/Mpris`: entries with `dbusName` become QtObject players. Plain entries are
  unchanged.
- `ParityState`:
  - processes take an optional `"sequence"`, the n-th start of a polled command such as
    `system-snapshot.sh`;
  - `@ASSETS@` is substituted inside `services`.
- `capture.sh`: the manifest records `source_ref`/`profile` when set, and the fixture path
  is relative to `PARITY_FIXTURES`.
