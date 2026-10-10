# Dell XPS laptop configuration

The `laptop` branch preserves the migrated HyprV desktop customizations and adapts them for the 2024 XPS 13 9350. Desktop `main` remains unchanged.

- Quickshell keeps WiFi, Bluetooth, media, recording, theme, DND and sleep controls. The bar shows CPU package power separately from battery charge/status. Power profiles and display/demo mode use horizontal, two-cell controls; the redundant battery card is removed from the panel. Power cycles the power-saver, balanced and performance profiles.
- Windows VM / Viewflow desktop controls and chassis / pump fan sliders are removed from the laptop panel. Dell firmware manages cooling; shell startup never restores the desktop fan controller.
- Internal eDP-1 uses native resolution at scale 2; external screens remain automatic. Sunshine uses the Intel graphics defaults.
- Brightness restores the direct brightnessctl slider implementation preceding 8128051, plus the earlier laptop keyboard helper, bound to intel_backlight. Slider feedback reads actual hardware percentage; no lookup-table rebuilding or delayed write queue is used. The real XPS has maximum backlight 400.

This branch does not contain VM disks or model weights. Migration exclusions are managed separately by the installation scripts. LazyCat requires a fresh login on this machine.

QQ, WeChat, Telegram and Discord on workspace 2 start manually on the laptop. Feishu and the other restored applications retain automatic startup. The bar has no Windows fusion button.

CPU temperature appears only when hovering the power item, with no separate bar temperature module. Install hardware/xps9350/90-xps-backlight.rules in /etc/udev/rules.d so the video group can write the panel brightness.

## Notification blur

Install `hardware/xps9350/swaync-blur-compat.conf` as
`~/.config/systemd/user/swaync.service.d/blur-compat.conf`, then reload user
systemd and restart swaync. This scoped GTK 4.24 compatibility setting lets
Hyprland layer blur rules apply; notification CSS remains unchanged.
