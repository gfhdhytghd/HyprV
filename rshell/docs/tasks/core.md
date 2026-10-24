# Task: core (crates hyprv-core, hyprv-shell skeleton, hyprv-privhelper skeleton)
Read ../ARCHITECTURE.md first. You own /home/wilf/.config/HyprV/rshell/crates/hyprv-core, crates/hyprv-shell (binary skeleton only), crates/hyprv-privhelper (skeleton), and rshell/xtask. Do NOT touch crates/hyprv-ui (another agent builds it in parallel).
1. hyprv-core: state store with typed topics and diff publication; Service trait (start/stop, subscriber-counted activation for sampling services like ksystemstats); tokio runtime on its own thread bridged to a calloop channel for the UI thread; logind PrepareForSleep + clock-change (timerfd TFD_TIMER_CANCEL_ON_SET) helpers; minute-aligned clock source; config + theme loading (look at how quickshell gets colors: MatugenColors.qml and any matugen output files, and replicate reading the same source files, with inotify reload).
2. IPC: Unix socket server at $XDG_RUNTIME_DIR/hyprv/shell.sock, and CLI `hyprv-shell ipc call <target> <fn> [args..]` with the same targets/functions as the IpcHandlers in ../quickshell/shell.qml (bar, controlPanel, calendar, quickAdjust …). Also `hyprv-shell kill`, single-instance lock.
3. hyprv-shell main skeleton wiring core together (UI integration later).
4. hyprv-privhelper skeleton: minimal root helper with a strict command allowlist (fan control first — read ../quickshell/scripts/fan-control-helper.sh and related .service/.conf to learn what it does), D-Bus system bus interface + polkit check, systemd unit + dbus policy files in rshell/packaging/ (do NOT install them).
5. xtask with `cargo xtask parity` stub (diff two PNG dirs, tolerance, HTML/markdown report).
Tests for store/IPC. Keep dependencies lean; zero idle wakeups.
When done with a milestone, reply with a short summary.
