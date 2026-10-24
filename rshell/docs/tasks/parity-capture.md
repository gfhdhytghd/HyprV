# Task: parity-capture (reference images WITHOUT touching the live desktop)
You own /home/wilf/.config/HyprV/rshell/parity/ only.
Build a harness that renders the ORIGINAL QML components offscreen into PNGs, so the Rust port can be diffed pixel-by-pixel:
- Use QT_QPA_PLATFORM=offscreen (or a nested headless compositor if offscreen is impossible) and a separate process — never connect to or disturb the user's running quickshell/Hyprland session.
- Components import Quickshell.* modules; either run under `quickshell -p <harness dir>` with offscreen platform if that works, or provide mock QML modules (Quickshell, Quickshell.Io, Quickshell.Services.*, Quickshell.Wayland, Quickshell.Hyprland…) under parity/qml-mocks that expose the properties the components read, fed from JSON fixtures.
- Fixtures: parity/fixtures/<component>/<state>.json describing all input data; cover the "visual states worth a reference screenshot" lists in docs/spec/ui-*.md (start with ui-island.md, then ui-bar / ui-panels when they appear).
- Render each state with Item.grabToImage at scale 1.0 and at the user's real output scale(s) (read `hyprctl monitors -j`, read-only), light + dark theme, transparent background, into parity/reference/<component>/<state>@<scale>.png.
- For animations: find a way to step time deterministically (e.g. QQuickRenderControl + custom QAnimationDriver in a tiny C++/Python(PySide6) host, or pausing/seeking animations) and dump frame sequences at fixed ms offsets for the key transitions listed in the specs. If not feasible, document why and capture start/end states only.
- Fonts: must be the same fonts the live shell uses; fail loudly if a font is missing.
- One command: `parity/capture.sh [component]`. Document format in parity/README.md so `cargo xtask parity` (core agent) can diff Rust renders against these.
Never edit ../quickshell; copy or import files read-only.
When done, reply with a short summary.
