# Parity verification

Generated 2026-10-24 12:52 EDT by `python3 parity/verify/verify.py` (owner: parity QA). References: `parity/reference` (Qt RHI/OpenGL renders of the original QML, see `parity/README.md`). Heatmaps: red = premultiplied channel delta × 4 over the dimmed reference.

**Gate** (`docs/ARCHITECTURE.md`, tightened): images without distance-field text must be **bit-exact** (their references are bit-exact run to run); images with distance-field text max ≤ 8/255 and mean ≤ 0.5/255 over painted pixels; no structural difference in any image. `v1` = the earlier ≤ 8 / ≤ 0.5 gate applied to every image, shown to track progress.

Each crate is rendered twice: **cpu** = the crate's own harness raster (Skia CPU; panels uses an F16 surface), **gl** = the production path (`hyprv_ui::egl` EGL/GLES2 RGBA8 render target, read back offscreen via `parity/gpu`). The gate is decided on **gl** once a harness has wired the hook (`HYPRV_PARITY_BACKEND=gl`); until then the cpu result is provisional.

| component | backend | crate (owner) | render | images | pass | fail | missing | excluded | gate v1 pass | causes of failures |
|---|---|---|---|---|---|---|---|---|---|---|
| island | cpu | `hyprv-ui-island` (svc-media) | ok | 380 | 0 | 380 | 0 | 0 | 348 | fill/color 290, rect AA 63, svg/icon 26, text 1 |
| island | gl **(gated)** | `hyprv-ui-island` (svc-media) | ok | 380 | 380 | 0 | 0 | 0 | 380 | - |
| bar | cpu | `hyprv-ui-bar` (svc-power) | ok | 312 | 32 | 260 | 8 | 12 | 304 | fill/color 205, rect AA 55 |
| bar | gl **(gated)** | `hyprv-ui-bar` (svc-power) | ok | 312 | 300 | 0 | 0 | 12 | 312 | - |
| panels | cpu | `hyprv-ui-panels` (spec-bar) | ok | 300 | 100 | 200 | 0 | 0 | 300 | rect AA 144, fill/color 56 |
| panels | gl **(gated)** | `hyprv-ui-panels` (spec-bar) | ok | 300 | 300 | 0 | 0 | 0 | 300 | - |

**Gate status:** island PASS, bar PASS, panels PASS. `excluded` = live-unreachable states (decision: match live behaviour, `QUIRK_BATTERY_POPUP_NEVER_OPENS`); their references are kept for a later fix-forward.

Causes: `layout` = painted footprint moved > 1 logical px or mask IoU < 0.98; `text` / `svg/icon` / `canvas` = most failing pixels inside reference Text / Image / Canvas items (regions sidecars); `rect AA` = failing pixels outside those, ≥ 80 % on coverage edges; `fill/color` = interior shape pixels, or only small (≤ 8) differences spread over fills; `animation` = a mid-sequence frame ≥ 3× worse (mean) than the settled last frame of its sequence (timing/easing, or layout not following the animated geometry).


## Production GPU path vs CPU harness

Same fixtures, same crate code; the only difference is the rasteriser. `differing` = images whose CPU and GPU renders are not byte-identical (premultiplied).

| component | GPU hook | cpu pass | gl pass | images compared | differing | max delta | worst |
|---|---|---|---|---|---|---|---|
| island | wired | 0 | 380 | 380 | 380 | 50 | `dual-music-expand--expand-t0150--light@1.png` (50), `dual-music-expand--expand-t0150--dark@1.png` (50), `dual-music-expand--collapse-t0150--light@2.png` (35) |
| bar | wired | 32 | 300 | 304 | 272 | 3 | `workspace-urgent--light@1.png` (3), `workspace-named--light@1.png` (3), `workspace-hover--light@1.png` (3) |
| panels | wired | 100 | 300 | 300 | 256 | 3 | `qa-show-hide--show-t0060--light@2.png` (3), `qa-show-hide--show-t0060--light@1.png` (3), `qa-show-hide--show-t0060--dark@2.png` (3) |

## island (gl)

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| agent-appear | 20 | 20 | 0 | 0.000 | `agent-appear--appear-t0000--dark@1.png` | - | – |
| agent-compact-approval | 4 | 4 | 0 | 0.000 | `agent-compact-approval--dark@1.png` | - | – |
| agent-compact-claude | 4 | 4 | 0 | 0.000 | `agent-compact-claude--dark@1.png` | - | – |
| agent-compact-completed | 4 | 4 | 0 | 0.000 | `agent-compact-completed--dark@1.png` | - | – |
| agent-compact-progress-zero | 4 | 4 | 0 | 0.000 | `agent-compact-progress-zero--dark@1.png` | - | – |
| agent-compact-running | 4 | 4 | 0 | 0.000 | `agent-compact-running--dark@1.png` | - | – |
| agent-compact-unknown-source | 4 | 4 | 0 | 0.000 | `agent-compact-unknown-source--dark@1.png` | - | – |
| agent-expanded-1 | 4 | 4 | 0 | 0.000 | `agent-expanded-1--dark@1.png` | - | – |
| agent-expanded-3 | 4 | 4 | 0 | 0.000 | `agent-expanded-3--dark@1.png` | - | – |
| agent-expanded-9-scrolled | 8 | 8 | 0 | 0.000 | `agent-expanded-9-scrolled--scrolled--dark@1.png` | - | – |
| agent-expanded-done-row | 4 | 4 | 0 | 0.000 | `agent-expanded-done-row--dark@1.png` | - | – |
| calendar-host | 40 | 40 | 0 | 0.000 | `calendar-host--close-t0000--dark@1.png` | - | – |
| dual-agent-expand | 20 | 20 | 0 | 0.000 | `dual-agent-expand--expand-t0000--dark@1.png` | - | – |
| dual-collapsed-approval | 4 | 4 | 0 | 0.000 | `dual-collapsed-approval--dark@1.png` | - | – |
| dual-collapsed-done | 4 | 4 | 0 | 0.000 | `dual-collapsed-done--dark@1.png` | - | – |
| dual-collapsed-question | 4 | 4 | 0 | 0.000 | `dual-collapsed-question--dark@1.png` | - | – |
| dual-collapsed-running | 4 | 4 | 0 | 0.000 | `dual-collapsed-running--dark@1.png` | - | – |
| dual-music-expand | 60 | 60 | 0 | 0.000 | `dual-music-expand--collapse-t0000--dark@1.png` | - | – |
| idle | 4 | 4 | 0 | 0.000 | `idle--dark@1.png` | - | – |
| idle-hover-lock | 4 | 4 | 0 | 0.000 | `idle-hover-lock--dark@1.png` | - | – |
| idle-hover-power | 4 | 4 | 0 | 0.000 | `idle-hover-power--dark@1.png` | - | – |
| idle-press-lock | 16 | 16 | 0 | 0.000 | `idle-press-lock--press-t0000--dark@1.png` | - | – |
| music-appear | 20 | 20 | 0 | 0.000 | `music-appear--appear-t0000--dark@1.png` | - | – |
| music-compact-noart | 4 | 4 | 0 | 0.000 | `music-compact-noart--dark@1.png` | - | – |
| music-compact-paused | 12 | 12 | 0 | 0.000 | `music-compact-paused--wave-t0000--dark@1.png` | - | – |
| music-compact-playing | 4 | 4 | 0 | 0.000 | `music-compact-playing--dark@1.png` | - | – |
| music-compact-playing-nodata | 8 | 8 | 0 | 0.000 | `music-compact-playing-nodata--wave-t0000--dark@1.png` | - | – |
| music-compact-wideart | 4 | 4 | 0 | 0.000 | `music-compact-wideart--dark@1.png` | - | – |
| music-expand-collapse | 56 | 56 | 0 | 0.000 | `music-expand-collapse--collapse-t0000--dark@1.png` | - | – |
| music-expanded-nolength-hover-next | 4 | 4 | 0 | 0.000 | `music-expanded-nolength-hover-next--dark@1.png` | - | – |
| music-expanded-paused-cjk | 4 | 4 | 0 | 0.000 | `music-expanded-paused-cjk--dark@1.png` | - | – |
| music-expanded-playing | 4 | 4 | 0 | 0.000 | `music-expanded-playing--dark@1.png` | - | – |
| music-expanded-seeking | 8 | 8 | 0 | 0.000 | `music-expanded-seeking--held--dark@1.png` | - | – |
| music-swipe-from-side | 12 | 12 | 0 | 0.000 | `music-swipe-from-side--after-t0000--dark@1.png` | - | – |
| music-swipe-hide | 12 | 12 | 0 | 0.000 | `music-swipe-hide--after-t0000--dark@1.png` | - | – |

## bar (gl)

The bar export leaves the island slot empty; that rectangle (`foreign_island_local_rect`) is cleared in both images. Island contents are verified in the island section. `calendar-host` and `island-expanded-host` mount their production content without that mask; assembled full-screen coverage is additionally required by cutover preflight.

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| baseline | 4 | 4 | 0 | 0.000 | `baseline--dark@1.png` | - | – |
| battery-charging | 4 | – | – | – | – | excluded (live-unreachable) | – |
| battery-critical | 4 | – | – | – | – | excluded (live-unreachable) | – |
| battery-full | 4 | – | – | – | – | excluded (live-unreachable) | – |
| calendar-host | 4 | 4 | 3 | 0.007 | `calendar-host--light@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/calendar-host--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/calendar-host--light@1.png) |
| corner-bottom-left | 4 | 4 | 0 | 0.000 | `corner-bottom-left--dark@1.png` | - | – |
| corner-bottom-right | 4 | 4 | 0 | 0.000 | `corner-bottom-right--dark@1.png` | - | – |
| corner-top-left | 4 | 4 | 0 | 0.000 | `corner-top-left--dark@1.png` | - | – |
| corner-top-right | 4 | 4 | 0 | 0.000 | `corner-top-right--dark@1.png` | - | – |
| cpu-power-critical | 4 | 4 | 0 | 0.000 | `cpu-power-critical--dark@1.png` | - | – |
| cpu-power-integer | 4 | 4 | 0 | 0.000 | `cpu-power-integer--dark@1.png` | - | – |
| cpu-power-low | 4 | 4 | 0 | 0.000 | `cpu-power-low--dark@1.png` | - | – |
| cpu-power-medium | 4 | 4 | 0 | 0.000 | `cpu-power-medium--dark@1.png` | - | – |
| fullscreen-slide | 48 | 48 | 0 | 0.000 | `fullscreen-slide--hide-t0000--dark@1.png` | - | – |
| island-expanded-host | 4 | 4 | 0 | 0.000 | `island-expanded-host--dark@1.png` | - | – |
| media-paused | 4 | 4 | 0 | 0.000 | `media-paused--dark@1.png` | - | – |
| media-playing | 4 | 4 | 0 | 0.000 | `media-playing--dark@1.png` | - | – |
| metrics | 4 | 4 | 0 | 0.000 | `metrics--dark@1.png` | - | – |
| narrow-output | 4 | 4 | 0 | 0.000 | `narrow-output--dark@1.png` | - | – |
| network-ethernet | 4 | 4 | 0 | 0.000 | `network-ethernet--dark@1.png` | - | – |
| network-offline | 4 | 4 | 0 | 0.000 | `network-offline--dark@1.png` | - | – |
| network-wifi-0-open | 4 | 4 | 0 | 0.000 | `network-wifi-0-open--dark@1.png` | - | – |
| network-wifi-0-secure | 4 | 4 | 0 | 0.000 | `network-wifi-0-secure--dark@1.png` | - | – |
| network-wifi-100-open | 4 | 4 | 0 | 0.000 | `network-wifi-100-open--dark@1.png` | - | – |
| network-wifi-100-secure | 4 | 4 | 0 | 0.000 | `network-wifi-100-secure--dark@1.png` | - | – |
| network-wifi-25-open | 4 | 4 | 0 | 0.000 | `network-wifi-25-open--dark@1.png` | - | – |
| network-wifi-25-secure | 4 | 4 | 0 | 0.000 | `network-wifi-25-secure--dark@1.png` | - | – |
| network-wifi-50-open | 4 | 4 | 0 | 0.000 | `network-wifi-50-open--dark@1.png` | - | – |
| network-wifi-50-secure | 4 | 4 | 0 | 0.000 | `network-wifi-50-secure--dark@1.png` | - | – |
| network-wifi-75-open | 4 | 4 | 0 | 0.000 | `network-wifi-75-open--dark@1.png` | - | – |
| network-wifi-75-secure | 4 | 4 | 0 | 0.000 | `network-wifi-75-secure--dark@1.png` | - | – |
| network-wifi-disconnected | 4 | 4 | 0 | 0.000 | `network-wifi-disconnected--dark@1.png` | - | – |
| network-wifi-hover | 4 | 4 | 0 | 0.000 | `network-wifi-hover--dark@1.png` | - | – |
| network-wifi-off | 4 | 4 | 0 | 0.000 | `network-wifi-off--dark@1.png` | - | – |
| notifications-bell | 4 | 4 | 0 | 0.000 | `notifications-bell--dark@1.png` | - | – |
| notifications-dnd | 4 | 4 | 0 | 0.000 | `notifications-dnd--dark@1.png` | - | – |
| notifications-dot | 4 | 4 | 0 | 0.000 | `notifications-dot--dark@1.png` | - | – |
| temperature-critical | 4 | 4 | 0 | 0.000 | `temperature-critical--dark@1.png` | - | – |
| title-long | 4 | 4 | 0 | 0.000 | `title-long--dark@1.png` | - | – |
| title-none | 4 | 4 | 0 | 0.000 | `title-none--dark@1.png` | - | – |
| tray-fallback-letter | 4 | 4 | 0 | 0.000 | `tray-fallback-letter--dark@1.png` | - | – |
| tray-menu-anim | 48 | 48 | 0 | 0.000 | `tray-menu-anim--close-t0000--dark@1.png` | - | – |
| tray-menu-hover | 4 | 4 | 0 | 0.000 | `tray-menu-hover--dark@1.png` | - | – |
| tray-menu-root | 4 | 4 | 0 | 0.000 | `tray-menu-root--dark@1.png` | - | – |
| tray-menu-submenu | 4 | 4 | 0 | 0.000 | `tray-menu-submenu--dark@1.png` | - | – |
| tray-overflow-2 | 4 | 4 | 0 | 0.000 | `tray-overflow-2--dark@1.png` | - | – |
| tray-overflow-6 | 4 | 4 | 0 | 0.000 | `tray-overflow-6--dark@1.png` | - | – |
| tray-overflow-7 | 4 | 4 | 0 | 0.000 | `tray-overflow-7--dark@1.png` | - | – |
| viewflow-hover | 4 | 4 | 0 | 0.000 | `viewflow-hover--dark@1.png` | - | – |
| volume-20 | 4 | 4 | 0 | 0.000 | `volume-20--dark@1.png` | - | – |
| volume-50 | 4 | 4 | 0 | 0.000 | `volume-50--dark@1.png` | - | – |
| volume-80 | 4 | 4 | 0 | 0.000 | `volume-80--dark@1.png` | - | – |
| volume-muted | 4 | 4 | 0 | 0.000 | `volume-muted--dark@1.png` | - | – |
| workspace-hover | 4 | 4 | 0 | 0.000 | `workspace-hover--dark@1.png` | - | – |
| workspace-named | 4 | 4 | 0 | 0.000 | `workspace-named--dark@1.png` | - | – |
| workspace-urgent | 4 | 4 | 0 | 0.000 | `workspace-urgent--dark@1.png` | - | – |

## panels (gl)

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| calendar-error | 4 | 4 | 3 | 0.007 | `calendar-error--light@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/calendar-error--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/calendar-error--light@1.png) |
| calendar-morph | 40 | 40 | 3 | 0.009 | `calendar-morph--open-t0100--dark@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/calendar-morph--open-t0100--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/calendar-morph--open-t0100--dark@1.png) |
| calendar-tooltip | 4 | 4 | 3 | 0.009 | `calendar-tooltip--light@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/calendar-tooltip--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/calendar-tooltip--light@1.png) |
| calendar-week | 4 | 4 | 3 | 0.007 | `calendar-week--light@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/calendar-week--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/calendar-week--light@1.png) |
| cp-brightness-0 | 4 | 4 | 0 | 0.000 | `cp-brightness-0--dark@1.png` | - | – |
| cp-brightness-100 | 4 | 4 | 0 | 0.000 | `cp-brightness-100--dark@1.png` | - | – |
| cp-brightness-37 | 4 | 4 | 0 | 0.000 | `cp-brightness-37--dark@1.png` | - | – |
| cp-brightness-drag | 4 | 4 | 0 | 0.000 | `cp-brightness-drag--held--dark@1.png` | - | – |
| cp-bt-many | 4 | 4 | 0 | 0.000 | `cp-bt-many--dark@1.png` | - | – |
| cp-bt-off | 4 | 4 | 0 | 0.000 | `cp-bt-off--dark@1.png` | - | – |
| cp-bt-scanning-empty | 4 | 4 | 0 | 0.000 | `cp-bt-scanning-empty--dark@1.png` | - | – |
| cp-bt-three | 4 | 4 | 0 | 0.000 | `cp-bt-three--dark@1.png` | - | – |
| cp-hover-toggle | 8 | 8 | 0 | 0.000 | `cp-hover-toggle--dnd--dark@1.png` | - | – |
| cp-main-idle | 4 | 4 | 0 | 0.000 | `cp-main-idle--dark@1.png` | - | – |
| cp-main-off | 4 | 4 | 0 | 0.000 | `cp-main-off--dark@1.png` | - | – |
| cp-mihomo-offline | 4 | 4 | 3 | 0.019 | `cp-mihomo-offline--dark@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/cp-mihomo-offline--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/cp-mihomo-offline--dark@1.png) |
| cp-mihomo-online | 4 | 4 | 3 | 0.030 | `cp-mihomo-online--dark@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/cp-mihomo-online--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/cp-mihomo-online--dark@1.png) |
| cp-open-close | 44 | 44 | 0 | 0.000 | `cp-open-close--close-t0000--dark@1.png` | - | – |
| cp-page-switch | 24 | 24 | 0 | 0.000 | `cp-page-switch--switch-t0000--dark@1.png` | - | – |
| cp-press-splittile | 12 | 12 | 0 | 0.000 | `cp-press-splittile--press-t0000--dark@1.png` | - | – |
| cp-press-toggle | 12 | 12 | 0 | 0.000 | `cp-press-toggle--press-t0000--dark@1.png` | - | – |
| cp-vm-running | 4 | 4 | 0 | 0.000 | `cp-vm-running--dark@1.png` | - | – |
| qa-brightness-0 | 4 | 4 | 0 | 0.000 | `qa-brightness-0--dark@1.png` | - | – |
| qa-brightness-100 | 4 | 4 | 0 | 0.000 | `qa-brightness-100--dark@1.png` | - | – |
| qa-brightness-50 | 4 | 4 | 0 | 0.000 | `qa-brightness-50--dark@1.png` | - | – |
| qa-show-hide | 28 | 28 | 0 | 0.000 | `qa-show-hide--later-t0000--dark@1.png` | - | – |
| qa-volume-0-muted | 4 | 4 | 0 | 0.000 | `qa-volume-0-muted--dark@1.png` | - | – |
| qa-volume-100 | 4 | 4 | 0 | 0.000 | `qa-volume-100--dark@1.png` | - | – |
| qa-volume-30 | 4 | 4 | 0 | 0.000 | `qa-volume-30--dark@1.png` | - | – |
| startup-confirm | 4 | 4 | 1 | 0.015 | `startup-confirm--dark@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/startup-confirm--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/startup-confirm--dark@1.png) |
| startup-confirm-hover | 4 | 4 | 1 | 0.015 | `startup-confirm-hover--dark@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/startup-confirm-hover--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/startup-confirm-hover--dark@1.png) |
| startup-confirm-press | 4 | 4 | 1 | 0.015 | `startup-confirm-press--light@1.png` | - | [heat](../../parity/verify/out/panels-gl/heat/startup-confirm-press--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels-gl/tri/startup-confirm-press--light@1.png) |
| stats-0 | 4 | 4 | 0 | 0.000 | `stats-0--dark@1.png` | - | – |
| stats-1 | 4 | 4 | 0 | 0.000 | `stats-1--dark@1.png` | - | – |
| stats-120 | 4 | 4 | 0 | 0.000 | `stats-120--dark@1.png` | - | – |
| stats-offline | 4 | 4 | 0 | 0.000 | `stats-offline--dark@1.png` | - | – |
| wifi-connected-4 | 4 | 4 | 0 | 0.000 | `wifi-connected-4--dark@1.png` | - | – |
| wifi-expanded-secure | 4 | 4 | 0 | 0.000 | `wifi-expanded-secure--dark@1.png` | - | – |
| wifi-many | 4 | 4 | 0 | 0.000 | `wifi-many--dark@1.png` | - | – |
| wifi-off | 4 | 4 | 0 | 0.000 | `wifi-off--dark@1.png` | - | – |

Per-image metrics (max, mean, pixel counts, IoU, bbox shift, attribution shares): `parity/verify/out/results.json`.
