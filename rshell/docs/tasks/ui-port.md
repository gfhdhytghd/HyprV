# UI port tasks (shared rules)
Each UI area is its own crate so agents don't collide:
- crates/hyprv-ui-bar     — everything in docs/spec/ui-bar.md, ui-bar-layout.md, ui-bar-popups.md
- crates/hyprv-ui-panels  — docs/spec/ui-panels*.md (control panel, quick adjust, stats, wifi, calendar, mihomo, VM, deskflow/viewflow, startup)
- crates/hyprv-ui-island  — docs/spec/ui-island.md
Contract each crate exposes (the host in crates/hyprv-shell, owned by agent 'engine', mounts them):
- `Model` structs built from service State types (hyprv-svc-* crates) — no direct I/O in UI crates; actions go out as the service Action enums via a callback/sender.
- `build(&Model, &Theme) -> Node`, `update(&mut Scene, &Model)` that only edits changed nodes (Behaviors retarget per spec), pointer/keyboard/scroll handlers, and surface requirements (size, anchors, namespace, exclusive zone, keyboard interactivity) as data for the host.
- Use hyprv-widgets (read its README; request missing pieces in docs/requests/widgets.md) and hyprv-ui directly.
- Offscreen PNG tests for every "visual state worth a reference screenshot" in the spec; when rshell/parity/reference has the image, add a parity test (cargo xtask parity) and drive the diff to ~0. Text-derived sizes must come from real text layout (see docs/spec/COVERAGE.md and docs/spec/text-rendering.md when present).
- Reproduce QML behavior exactly, including quirks listed in the specs (decision pending from user; keep each quirk behind a clearly named const so it can be flipped).
Rules: write only inside your crate; no sudo, no git commit; no live surfaces except brief tests.
Reply with a short summary when done.
