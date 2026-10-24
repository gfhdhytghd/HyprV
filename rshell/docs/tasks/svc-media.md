# Task: svc-media
You own /home/wilf/.config/HyprV/rshell/crates/hyprv-svc-media (create it; workspace globs crates/*).
Modules: audio (PipeWire), spectrum (demand-only PipeWire capture + FFT, same output shape as scripts/audio-spectrum.sh), mpris (KDE mpris2 engine semantics incl. position extrapolation + Seeked), cider, tray (StatusNotifierWatcher/Host + dbusmenu), notifications (SwayNC).

Read ../ARCHITECTURE.md, docs/spec/services.md (relevant sections — it is long, grep it), docs/spec/services-api.md, and crates/hyprv-core/README.md (store/Service/SamplingLease/runtime APIs — use them, do not reinvent; if hyprv-core needs a generic addition, write the request in docs/requests/core.md instead of editing it).
Rules: pure event-driven (D-Bus signals, udev, netlink, inotify, sockets, websockets); no fork/exec of CLI tools at runtime (no nmcli/playerctl/hyprctl/systemctl/curl subprocess); sampling only under SamplingLease; zero idle wakeups. Model behavior on the KDE counterpart named in the spec. Every QML-visible field and every action in the spec must exist with identical semantics (the UI port depends on it). Expose per-module: typed State struct (serde), Action enum, `spawn(ctx) -> handle` using hyprv-core.
Testing: unit tests with mocked transports; integration tests against the real session/system bus must be READ-ONLY (no connecting to wifi, pairing, changing volume/brightness/fans/services). Put any manual verification steps in the crate README.
Write ONLY inside your crate directory. No sudo, no git commit, never restart quickshell/Hyprland or system services.
When done, reply with a short summary + list of gaps.
