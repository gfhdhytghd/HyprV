# Panel Lua migration verification

Status: PARTIAL. Static checks and simulated behavior pass. Live acceptance is blocked by the execution sandbox (Unix socket creation returns EPERM). No commit was made.

## Changed files

- `hypr/panels.lua`: in-process state, async spawn polling, 300 ms sidechat debounce, duplicate cleanup, geometry, pin/unpin, focus guards and tracing.
- `hypr/scripts/panel-launch.sh`: launch-only helper retaining AI_HUB_COMMAND, checkout search, ai-hub fallback and launch log. Used only when a panel is missing. Lua tracks the launcher PID for timeout cleanup.
- `hypr/scripts/dropdown.sh`, `hypr/scripts/sidechat.sh`: POSIX-shell compatibility wrappers, one IPC call each. Argument validation prevents Lua injection. Dispatch receives a function, not the result of executing the action.
- `hypr/hyprland-main.lua`, `hypr/hyprland-dropitem.lua`, `hypr/hyprland-plugins.lua`: direct Lua actions for gestures, binds and edge callbacks.
- `hypr/tests/panels-test.lua`: simulated regression checks.

`rotate-screen-ccw.sh` needs no change: its existing reflow calls use the wrappers. `dropcalender.sh` was not changed. Pre-existing unrelated edits were preserved.

The wrapper approach preserves external paths without paying for shell/IPC on gestures, binds or edges. AI_HUB_COMMAND and AI_HUB_REPO now come from the compositor's environment; per-invocation environment overrides on the CLI wrapper are not forwarded. Shell/PID work occurs only during application launch, not during existing-window toggles.

## Executed commands and outputs

From `/home/wilf/.config/HyprV`:

```sh
bash /home/wilf/.codex/skills/hyprland-lua/scripts/check-hyprland-lua.sh hypr/panels.lua hypr/hyprland-main.lua hypr/hyprland-dropitem.lua hypr/hyprland-plugins.lua
```

```text
syntax ok: hypr/panels.lua
syntax ok: hypr/hyprland-main.lua
syntax ok: hypr/hyprland-dropitem.lua
syntax ok: hypr/hyprland-plugins.lua
```

```sh
lua hypr/tests/panels-test.lua
```

```text
PASS: 3x cycles per panel, idempotence, cross-case, special restore guard, geometry/transform, debounce, duplicate cleanup, pending hide, invalid monitor
```

```sh
sh -n hypr/scripts/dropdown.sh hypr/scripts/sidechat.sh
bash -n hypr/scripts/panel-launch.sh
git diff --check
```

Each produced no output and exited 0.

```sh
export HYPRLAND_INSTANCE_SIGNATURE=de4688ff25ddec109a88b384dbb73804c04f1c49_1793761279_1494059963
hyprctl status
hyprctl version
hyprctl reload
hyprctl configerrors
hyprctl monitors -j
```

Each produced exactly:

```text
Couldn't set socket timeout (2)
```

A direct Python AF_UNIX socket diagnostic produced:

```text
PermissionError(1, 'Operation not permitted')
```

Local source confirms `hyprctl dispatch` wraps `hl.dispatch(...)` and requires a function/dispatcher (`src/ipc/s1/Commands.cpp`, `LuaBindingsDispatcherUtils.cpp`). Query fields and dispatcher options were checked against `/home/wilf/data/Hyprland/src/config/lua`. This does NOT establish the running compositor's API version.

## Not verified live

- Successful reload and empty configerrors.
- Three show/hide cycles through real entry points for each panel.
- Empty monitor specialWorkspace and restored previous focus after each cycle.
- Dropdown-focus / sidechat-show / dropdown-hide / sidechat-hide cross-case.
- Targeted show, hide, both reflows and physical transformed-output geometry.
- Actual missing-window launch, timeout cleanup and duplicate-window close.
- Real keyboard binds, touchpad gestures and edge events.
- Final hidden-panel state and normal-window focus.

No successful compositor IPC was possible. No focus or workspace test was performed. Do not interpret the simulated PASS as live acceptance.

## Live follow-up (2026-11-03, Claude)

- Found on the live compositor: calling a dispatcher object directly (`hl.dsp.x(...)()`) is a silent no-op inside `hyprctl dispatch` functions; Hyprland reports `dispatcher objects cannot be called directly; use hl.dispatch(dispatcher)`. The first port logged `shown` while nothing moved.
- Fixed: all dispatches in `hypr/panels.lua` and `hypr/viewflow-workspace.lua` now go through `hl.dispatch(...)`. `panels-test.lua` mocks dispatcher objects as non-callable, so the old form now fails the test.
- Live checks passed on Hyprland 0.57.0: 3 show/hide cycles per panel through the `.sh` wrappers; dropdown/sidechat cross case; `sidechat.sh show DP-4` twice (idempotent); `hide` twice; `reflow` for both; unknown monitor returns rc 7; fast double toggle is debounced; `SUPER+SHIFT+T` and `SUPER+SPACE` via ydotool; `hyprctl reload` with no config errors. `specialWorkspace` stayed empty in every step.
- Not live-tested: touchpad gestures, edge on_enter/on_exit, spawn path when the window does not exist (would kill the user's running terminal/AI Hub).
