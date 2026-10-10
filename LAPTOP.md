# Dell XPS laptop configuration

The `laptop` branch preserves the migrated HyprV desktop customizations and adapts them for the 2024 XPS 13 9350. Desktop `main` remains unchanged.

- Quickshell keeps WiFi, Bluetooth, media, recording, theme, DND and sleep controls. Battery charge, status and power use the existing real battery probe; the power button shows and cycles the power-saver, balanced and performance profiles.
- Windows VM / Viewflow desktop controls and chassis / pump fan sliders are removed from the laptop panel. Dell firmware manages cooling; shell startup never restores the desktop fan controller.
- Internal eDP-1 uses native resolution at scale 2; external screens remain automatic. Sunshine uses the Intel graphics defaults.
- Brightness retains the backlight-first backend and 50-step mapping from b37a9bc; the real XPS reports intel_backlight with a maximum of 400. No external DDC polling is needed for the internal panel.

This branch does not contain VM disks or model weights. Migration exclusions are managed separately by the installation scripts. LazyCat requires a fresh login on this machine.
