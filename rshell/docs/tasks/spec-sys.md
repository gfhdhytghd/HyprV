# Task: spec-sys
Read ../ARCHITECTURE.md first. Source: /home/wilf/.config/HyprV/quickshell (read-only), incl. scripts/, tests/, and hypr/*.lua that call the shell.
Write /home/wilf/.config/HyprV/rshell/docs/spec/services.md.
For EVERY system interaction (each script in scripts/, each Quickshell.Services.* usage, each Process/PollCommand/StreamCommand/Socket/FileView in QML, each systemd unit/conf file there, the agent-island python stack, calendar/weather, fan control, Windows VM, mihomo, deskflow/viewflow, hdmi IR, egpu, demo mode, theme toggle, ui-state, prevent-sleep, recording):
- what it reads/writes, exact output format consumed by QML (fields, types, units), polling interval today
- every action (command + args) and its side effects, required privileges
- the event-driven replacement: concrete D-Bus service/interface/signal, udev subsystem, netlink, inotify path, websocket endpoint, Hyprland socket2 event, etc. Model on KDE Plasma (plasma-nm/NetworkManagerQt SecretAgent, BluezQt + pairing agent, plasma-pa, mpris2 engine incl. position extrapolation + Seeked, PowerDevil/UPower/power-profiles-daemon, PowerDevil backlight+DDC, ksystemstats subscriber-counted sampling, KAuth for privileged ops, logind PrepareForSleep).
- recommended Rust crates (zbus, udev, pipewire, tokio-tungstenite, nix inotify, etc.)
- anything that truly cannot be event driven, and the minimum-wakeup strategy for it
Also list all IPC entry points (quickshell ipc call targets/functions and who calls them) and all files/env the shell depends on.
Finally propose the module list + typed state structs (Rust) for crates/hyprv-services, in docs/spec/services-api.md.
When done, reply with a 10-line summary.
