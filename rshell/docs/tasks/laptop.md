# Laptop (Dell XPS) profile — task brief (2026-10-24)

Goal: the same `hyprv-shell` binary replaces Quickshell on the **laptop** branch
(`origin/laptop`, Dell XPS) with exact functional + visual parity to that branch's
Quickshell, and is as frugal as possible in **minimal state** (bar collapsed /
non-detailed stats, nothing open, on battery).

## Source of truth
- Laptop QML/scripts/hypr config: `git show origin/laptop:<path>` (do NOT check out the
  branch in the shared tree; use `git worktree add /tmp/.../hyprv-laptop origin/laptop`
  under your scratch area if you need a tree; remove it when done).
- Desktop parity is already done (docs/spec/*). Spec only the **delta**: everything the
  laptop branch has or renders differently vs. the current main working tree that rshell
  was specced from. Write it to `docs/spec/laptop-*.md`, same precision rules as before
  (geometry, colors, fonts, animations, quirks as QUIRK_* consts, legacy beats KDE).

## Architecture rules
- One binary, one codebase. Profile chosen at runtime: `HYPRV_PROFILE=desktop|laptop`
  override, otherwise auto-detect (DMI `/sys/class/dmi/id/product_name`, battery
  presence) — document the rule in docs/ARCHITECTURE.md. No `#[cfg]` forks for laptop.
- Desktop behaviour must stay byte-identical (desktop parity/e2e must keep passing).
- Laptop has no nct6799 fan helper: hyprv-privhelper cooling must be inert there (no
  units enabled for it unless the laptop branch needs a privileged action — check).
- Still: event-driven, no runtime exec except user-initiated actions without native API,
  Qt-free runtime, shared CARGO_TARGET_DIR, CARGO_INCREMENTAL=0, disk floor 30 GB.

## Minimal-state resource budget (laptop)
In minimal state with the session idle on battery:
- 0 timer wakeups attributable to the shell beyond those required by the visible bar
  (clock minute tick; battery from UPower signals, not polling; no sysstats sampling of
  hidden detailed stats; network rate sampling only if visible).
- No GPU work when nothing changes; free/shrink caches and destroy hidden surfaces
  (panels, popups, large overlays) — recreate on demand within the animation budget.
- Target steady RSS ≤ 120 MiB and idle CPU ≤ 0.1% in a nested controlled run; report
  real numbers and the wakeup attribution table. Any regression on desktop is a blocker.

## Testing
No XPS hardware here. Every laptop-only source (UPower battery, backlight / HDR
brightness, lid, power-profiles, laptop thermals, touchpad gestures) needs a fake
backend (dbus session/system mocks, fake sysfs trees) and nested-compositor e2e +
parity renders against the laptop Quickshell reference (render the laptop QML in the
parity harness with the same fakes).

## Rules (unchanged)
No sudo, no git commit/push, never touch the live desktop session/shell/helper, only
write in assigned paths, coordinate via docs/requests/, Claude agents compact after big
tasks.
