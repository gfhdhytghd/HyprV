# Functional mock provider fixtures

These seeds feed the real Assembly in the **private** `--nested-functional-test`
compositor. They do not replay pointer handlers. Input and independently specified
expected typed service commands live in `src/functional/suite.rs`.

- bt/bt-many/bt-off, wifi/wifi-many/secure, calendar, mihomo-offline: original mock streams from
  parity/fixtures/panels (cp-bt-three/cp-bt-many/cp-bt-off, wifi-connected-4/wifi-many/
  wifi-expanded-secure, calendar-week, cp-mihomo-offline), with scene=desktop.qml and no replay steps.
- idle/music/agent/dual: original parity/fixtures/island props mounted inside the
  assembled desktop fixture's root props; standalone island.qml layout is unused.
- tray: deterministic synthetic SNI items with activation/menu/check/disabled/
  submenu entries, enough items for overflow, and synthetic battery details. Media
  is paused to isolate tray input from unrelated waveform deadlines.

All credentials are deliberately synthetic (`testpass124`). No provider actors,
real buses, hardware writes, app launches or real-session input are used. Fixtures
are settled at virtual2500ms before native input so initial provider stream delivery
cannot move a hit target on the first click. Report and full commands are in
crates/hyprv-shell/README.md and docs/verify/e2e.md.
