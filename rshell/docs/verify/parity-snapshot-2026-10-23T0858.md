# Parity matrix — source snapshot 2026-10-23 08:58:10 EDT

Run by svc-media on an rsync snapshot of rshell (sources only, taken 08:58:10–08:58:18 EDT, fingerprint `a6485fe97be1566f`; `target/` shared, sibling `quickshell`/`hypr` linked read-only), so concurrent edits by other agents cannot leak into the run. `python3 parity/verify/verify.py` against `parity/reference`, 08:58:44–09:05:37. Sources unchanged during the run (the only new source-pattern file is the bar harness's generated `.build/check/Cargo.toml`). Raw: [raw/parity-snapshot-2026-10-23T0858.json](raw/parity-snapshot-2026-10-23T0858.json).

| component | backend | crate (owner) | render | images | pass | fail | missing | excluded | gate v1 pass | causes of failures |
|---|---|---|---|---|---|---|---|---|---|---|
| island | cpu **(gated)** | `hyprv-ui-island` (svc-media) | ok | 380 | 0 | 380 | 0 | 0 | 127 | canvas 136, fill/color 120, animation 52, text 42, rect AA 26, svg/icon 4 |
| island | gl | `hyprv-ui-island` (svc-media) | GPU hook not wired in the crate harness | 0 | 0 | 0 | 0 | 0 | 0 | - |
| bar | cpu | `hyprv-ui-bar` (svc-power) | ok | 312 | 224 | 68 | 8 | 12 | 303 | fill/color 38, rect AA 29, text 1 |
| bar | gl **(gated)** | `hyprv-ui-bar` (svc-power) | ok | 312 | 230 | 62 | 8 | 12 | 303 | rect AA 32, fill/color 29, text 1 |
| panels | cpu **(gated)** | `hyprv-ui-panels` (spec-bar) | ok | 300 | 64 | 184 | 52 | 0 | 248 | fill/color 97, rect AA 87 |
| panels | gl | `hyprv-ui-panels` (spec-bar) | GPU hook not wired in the crate harness | 0 | 0 | 0 | 0 | 0 | 0 | - |

**Gate status:** island FAIL (provisional: cpu), bar FAIL, panels FAIL (provisional: cpu). `excluded` = live-unreachable states (decision: match live behaviour, `QUIRK_BATTERY_POPUP_NEVER_OPENS`); their references are kept for a later fix-forward.

Causes: `layout` = painted footprint moved > 1 logical px or mask IoU < 0.98; `text` / `svg/icon` / `canvas` = most failing pixels inside reference Text / Image / Canvas items (regions sidecars); `rect AA` = failing pixels outside those, ≥ 80 % on coverage edges; `fill/color` = interior shape pixels, or only small (≤ 8) differences spread over fills; `animation` = a mid-sequence frame ≥ 3× worse (mean) than the settled last frame of its sequence (timing/easing, or layout not following the animated geometry).


## Production GPU path vs CPU harness

Same fixtures, same crate code; the only difference is the rasteriser. `differing` = images whose CPU and GPU renders are not byte-identical (premultiplied).

| component | GPU hook | cpu pass | gl pass | images compared | differing | max delta | worst |
|---|---|---|---|---|---|---|---|
| island | GPU hook not wired in the crate harness | 0 | – | – | – | – | – |
| bar | wired | 224 | 230 | 304 | 272 | 3 | `workspace-urgent--light@2.png` (3), `workspace-urgent--light@1.png` (3), `workspace-urgent--dark@2.png` (3) |
| panels | GPU hook not wired in the crate harness | 64 | – | – | – | – | – |

## island (cpu, provisional)

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| agent-appear | 20 | 0 | 105 | 2.331 | `agent-appear--appear-t0000--dark@1.png` | animation, fill/color, text | [heat](../../parity/verify/out/island/heat/agent-appear--appear-t0000--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-appear--appear-t0000--dark@1.png) |
| agent-compact-approval | 4 | 0 | 4 | 0.175 | `agent-compact-approval--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-approval--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-approval--dark@1.png) |
| agent-compact-claude | 4 | 0 | 3 | 0.175 | `agent-compact-claude--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-claude--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-claude--dark@1.png) |
| agent-compact-completed | 4 | 0 | 4 | 0.175 | `agent-compact-completed--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-completed--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-completed--dark@1.png) |
| agent-compact-progress-zero | 4 | 0 | 4 | 0.197 | `agent-compact-progress-zero--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-progress-zero--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-progress-zero--dark@1.png) |
| agent-compact-running | 4 | 0 | 4 | 0.181 | `agent-compact-running--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-running--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-running--dark@1.png) |
| agent-compact-unknown-source | 4 | 0 | 3 | 0.095 | `agent-compact-unknown-source--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-compact-unknown-source--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-compact-unknown-source--dark@1.png) |
| agent-expanded-1 | 4 | 0 | 4 | 0.333 | `agent-expanded-1--dark@2.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-expanded-1--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-expanded-1--dark@2.png) |
| agent-expanded-3 | 4 | 0 | 4 | 0.709 | `agent-expanded-3--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/agent-expanded-3--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-expanded-3--dark@1.png) |
| agent-expanded-9-scrolled | 8 | 0 | 166 | 0.918 | `agent-expanded-9-scrolled--scrolled--dark@2.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/agent-expanded-9-scrolled--scrolled--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-expanded-9-scrolled--scrolled--dark@2.png) |
| agent-expanded-done-row | 4 | 0 | 173 | 0.651 | `agent-expanded-done-row--dark@2.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/agent-expanded-done-row--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/agent-expanded-done-row--dark@2.png) |
| calendar-host | 40 | 0 | 118 | 0.705 | `calendar-host--open-t0016--light@2.png` | canvas, fill/color, rect AA | [heat](../../parity/verify/out/island/heat/calendar-host--open-t0016--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/calendar-host--open-t0016--light@2.png) |
| dual-agent-expand | 20 | 0 | 13 | 0.398 | `dual-agent-expand--expand-t0070--dark@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/island/heat/dual-agent-expand--expand-t0070--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-agent-expand--expand-t0070--dark@2.png) |
| dual-collapsed-approval | 4 | 0 | 3 | 0.131 | `dual-collapsed-approval--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/dual-collapsed-approval--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-collapsed-approval--dark@1.png) |
| dual-collapsed-done | 4 | 0 | 3 | 0.127 | `dual-collapsed-done--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/dual-collapsed-done--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-collapsed-done--dark@1.png) |
| dual-collapsed-question | 4 | 0 | 3 | 0.133 | `dual-collapsed-question--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/dual-collapsed-question--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-collapsed-question--dark@1.png) |
| dual-collapsed-running | 4 | 0 | 3 | 0.134 | `dual-collapsed-running--dark@1.png` | fill/color | [heat](../../parity/verify/out/island/heat/dual-collapsed-running--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-collapsed-running--dark@1.png) |
| dual-music-expand | 60 | 0 | 198 | 2.073 | `dual-music-expand--expand-t0150--dark@1.png` | animation, canvas, fill/color, rect AA, svg/icon, text | [heat](../../parity/verify/out/island/heat/dual-music-expand--expand-t0150--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/dual-music-expand--expand-t0150--dark@1.png) |
| idle | 4 | 0 | 105 | 0.977 | `idle--dark@1.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/idle--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/idle--dark@1.png) |
| idle-hover-lock | 4 | 0 | 105 | 0.977 | `idle-hover-lock--dark@1.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/idle-hover-lock--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/idle-hover-lock--dark@1.png) |
| idle-hover-power | 4 | 0 | 105 | 0.977 | `idle-hover-power--dark@1.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/idle-hover-power--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/idle-hover-power--dark@1.png) |
| idle-press-lock | 16 | 0 | 189 | 0.984 | `idle-press-lock--press-t0060--dark@2.png` | fill/color, text | [heat](../../parity/verify/out/island/heat/idle-press-lock--press-t0060--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/idle-press-lock--press-t0060--dark@2.png) |
| music-appear | 20 | 0 | 105 | 0.985 | `music-appear--appear-t0000--dark@1.png` | canvas, fill/color, text | [heat](../../parity/verify/out/island/heat/music-appear--appear-t0000--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-appear--appear-t0000--dark@1.png) |
| music-compact-noart | 4 | 0 | 188 | 1.082 | `music-compact-noart--dark@2.png` | canvas, text | [heat](../../parity/verify/out/island/heat/music-compact-noart--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-compact-noart--dark@2.png) |
| music-compact-paused | 12 | 0 | 127 | 0.386 | `music-compact-paused--wave-t0132--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-compact-paused--wave-t0132--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-compact-paused--wave-t0132--dark@2.png) |
| music-compact-playing | 4 | 0 | 49 | 0.221 | `music-compact-playing--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-compact-playing--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-compact-playing--dark@2.png) |
| music-compact-playing-nodata | 8 | 0 | 161 | 0.501 | `music-compact-playing-nodata--wave-t0066--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-compact-playing-nodata--wave-t0066--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-compact-playing-nodata--wave-t0066--dark@2.png) |
| music-compact-wideart | 4 | 0 | 49 | 0.388 | `music-compact-wideart--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-compact-wideart--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-compact-wideart--dark@2.png) |
| music-expand-collapse | 56 | 0 | 219 | 23.756 | `music-expand-collapse--collapse-t0140--dark@2.png` | animation, canvas, rect AA | [heat](../../parity/verify/out/island/heat/music-expand-collapse--collapse-t0140--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-expand-collapse--collapse-t0140--dark@2.png) |
| music-expanded-nolength-hover-next | 4 | 0 | 185 | 0.234 | `music-expanded-nolength-hover-next--dark@2.png` | canvas, text | [heat](../../parity/verify/out/island/heat/music-expanded-nolength-hover-next--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-expanded-nolength-hover-next--dark@2.png) |
| music-expanded-paused-cjk | 4 | 0 | 159 | 0.120 | `music-expanded-paused-cjk--light@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-expanded-paused-cjk--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-expanded-paused-cjk--light@2.png) |
| music-expanded-playing | 4 | 0 | 159 | 0.293 | `music-expanded-playing--dark@2.png` | canvas, rect AA | [heat](../../parity/verify/out/island/heat/music-expanded-playing--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-expanded-playing--dark@2.png) |
| music-expanded-seeking | 8 | 0 | 154 | 0.141 | `music-expanded-seeking--held--light@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-expanded-seeking--held--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-expanded-seeking--held--light@2.png) |
| music-swipe-from-side | 12 | 0 | 57 | 0.131 | `music-swipe-from-side--after-t0000--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-swipe-from-side--after-t0000--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-swipe-from-side--after-t0000--dark@2.png) |
| music-swipe-hide | 12 | 0 | 49 | 0.221 | `music-swipe-hide--after-t0000--dark@2.png` | canvas | [heat](../../parity/verify/out/island/heat/music-swipe-hide--after-t0000--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/island/tri/music-swipe-hide--after-t0000--dark@2.png) |

## bar (gl)

The bar export leaves the island slot empty; that rectangle (`foreign_island_local_rect`) is cleared in both images. Island contents are verified in the island section. `calendar-host` and `island-expanded-host` are foreign-content views (not exported).

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| baseline | 4 | 4 | 2 | 0.004 | `baseline--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/baseline--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/baseline--dark@1.png) |
| battery-charging | 4 | – | – | – | – | excluded (live-unreachable) | – |
| battery-critical | 4 | – | – | – | – | excluded (live-unreachable) | – |
| battery-full | 4 | – | – | – | – | excluded (live-unreachable) | – |
| calendar-host | 4 | 0 | – | – | – | missing | – |
| corner-bottom-left | 4 | 4 | 0 | 0.000 | `corner-bottom-left--dark@1.png` | - | – |
| corner-bottom-right | 4 | 4 | 0 | 0.000 | `corner-bottom-right--dark@1.png` | - | – |
| corner-top-left | 4 | 0 | 1 | 0.004 | `corner-top-left--light@2.png` | fill/color | [heat](../../parity/verify/out/bar-gl/heat/corner-top-left--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/corner-top-left--light@2.png) |
| corner-top-right | 4 | 1 | 2 | 0.006 | `corner-top-right--light@2.png` | fill/color | [heat](../../parity/verify/out/bar-gl/heat/corner-top-right--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/corner-top-right--light@2.png) |
| cpu-power-critical | 4 | 4 | 2 | 0.004 | `cpu-power-critical--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/cpu-power-critical--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/cpu-power-critical--dark@1.png) |
| cpu-power-integer | 4 | 4 | 2 | 0.004 | `cpu-power-integer--light@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/cpu-power-integer--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/cpu-power-integer--light@1.png) |
| cpu-power-low | 4 | 4 | 2 | 0.004 | `cpu-power-low--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/cpu-power-low--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/cpu-power-low--dark@1.png) |
| cpu-power-medium | 4 | 4 | 2 | 0.004 | `cpu-power-medium--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/cpu-power-medium--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/cpu-power-medium--dark@1.png) |
| fullscreen-slide | 48 | 48 | 2 | 0.005 | `fullscreen-slide--show-t0100--light@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/fullscreen-slide--show-t0100--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/fullscreen-slide--show-t0100--light@1.png) |
| island-expanded-host | 4 | 0 | – | – | – | missing | – |
| media-paused | 4 | 4 | 2 | 0.004 | `media-paused--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/media-paused--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/media-paused--dark@1.png) |
| media-playing | 4 | 4 | 2 | 0.004 | `media-playing--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/media-playing--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/media-playing--dark@1.png) |
| metrics | 4 | 4 | 2 | 0.004 | `metrics--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/metrics--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/metrics--dark@1.png) |
| narrow-output | 4 | 4 | 2 | 0.007 | `narrow-output--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/narrow-output--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/narrow-output--dark@1.png) |
| network-ethernet | 4 | 4 | 2 | 0.004 | `network-ethernet--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-ethernet--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-ethernet--dark@1.png) |
| network-offline | 4 | 4 | 2 | 0.004 | `network-offline--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-offline--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-offline--dark@1.png) |
| network-wifi-0-open | 4 | 4 | 2 | 0.004 | `network-wifi-0-open--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-0-open--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-0-open--dark@1.png) |
| network-wifi-0-secure | 4 | 4 | 2 | 0.004 | `network-wifi-0-secure--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-0-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-0-secure--dark@1.png) |
| network-wifi-100-open | 4 | 4 | 2 | 0.004 | `network-wifi-100-open--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-100-open--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-100-open--dark@1.png) |
| network-wifi-100-secure | 4 | 4 | 2 | 0.004 | `network-wifi-100-secure--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-100-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-100-secure--dark@1.png) |
| network-wifi-25-open | 4 | 4 | 2 | 0.004 | `network-wifi-25-open--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-25-open--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-25-open--dark@1.png) |
| network-wifi-25-secure | 4 | 4 | 2 | 0.004 | `network-wifi-25-secure--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-25-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-25-secure--dark@1.png) |
| network-wifi-50-open | 4 | 4 | 2 | 0.004 | `network-wifi-50-open--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-50-open--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-50-open--dark@1.png) |
| network-wifi-50-secure | 4 | 4 | 2 | 0.004 | `network-wifi-50-secure--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-50-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-50-secure--dark@1.png) |
| network-wifi-75-open | 4 | 4 | 2 | 0.004 | `network-wifi-75-open--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-75-open--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-75-open--dark@1.png) |
| network-wifi-75-secure | 4 | 4 | 2 | 0.004 | `network-wifi-75-secure--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-75-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-75-secure--dark@1.png) |
| network-wifi-disconnected | 4 | 4 | 2 | 0.004 | `network-wifi-disconnected--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-disconnected--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-disconnected--dark@1.png) |
| network-wifi-hover | 4 | 4 | 2 | 0.004 | `network-wifi-hover--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-hover--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-hover--dark@1.png) |
| network-wifi-off | 4 | 4 | 2 | 0.004 | `network-wifi-off--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/network-wifi-off--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/network-wifi-off--dark@1.png) |
| notifications-bell | 4 | 4 | 2 | 0.004 | `notifications-bell--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/notifications-bell--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/notifications-bell--dark@1.png) |
| notifications-dnd | 4 | 4 | 2 | 0.004 | `notifications-dnd--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/notifications-dnd--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/notifications-dnd--dark@1.png) |
| notifications-dot | 4 | 4 | 2 | 0.004 | `notifications-dot--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/notifications-dot--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/notifications-dot--dark@1.png) |
| temperature-critical | 4 | 4 | 2 | 0.004 | `temperature-critical--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/temperature-critical--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/temperature-critical--dark@1.png) |
| title-long | 4 | 4 | 2 | 0.002 | `title-long--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/title-long--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/title-long--dark@1.png) |
| title-none | 4 | 4 | 2 | 0.005 | `title-none--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/title-none--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/title-none--dark@1.png) |
| tray-fallback-letter | 4 | 4 | 2 | 0.003 | `tray-fallback-letter--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/tray-fallback-letter--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-fallback-letter--dark@1.png) |
| tray-menu-anim | 48 | 5 | 1 | 0.004 | `tray-menu-anim--close-t0400--light@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/bar-gl/heat/tray-menu-anim--close-t0400--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-menu-anim--close-t0400--light@2.png) |
| tray-menu-hover | 4 | 0 | 1 | 0.002 | `tray-menu-hover--dark@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/bar-gl/heat/tray-menu-hover--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-menu-hover--dark@2.png) |
| tray-menu-root | 4 | 0 | 1 | 0.002 | `tray-menu-root--dark@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/bar-gl/heat/tray-menu-root--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-menu-root--dark@2.png) |
| tray-menu-submenu | 4 | 0 | 23 | 4.159 | `tray-menu-submenu--dark@2.png` | fill/color, rect AA, text | [heat](../../parity/verify/out/bar-gl/heat/tray-menu-submenu--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-menu-submenu--dark@2.png) |
| tray-overflow-2 | 4 | 4 | 3 | 0.045 | `tray-overflow-2--light@2.png` | - | [heat](../../parity/verify/out/bar-gl/heat/tray-overflow-2--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-overflow-2--light@2.png) |
| tray-overflow-6 | 4 | 4 | 3 | 0.037 | `tray-overflow-6--light@2.png` | - | [heat](../../parity/verify/out/bar-gl/heat/tray-overflow-6--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-overflow-6--light@2.png) |
| tray-overflow-7 | 4 | 4 | 3 | 0.030 | `tray-overflow-7--dark@2.png` | - | [heat](../../parity/verify/out/bar-gl/heat/tray-overflow-7--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/tray-overflow-7--dark@2.png) |
| viewflow-hover | 4 | 4 | 2 | 0.004 | `viewflow-hover--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/viewflow-hover--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/viewflow-hover--dark@1.png) |
| volume-20 | 4 | 4 | 2 | 0.004 | `volume-20--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/volume-20--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/volume-20--dark@1.png) |
| volume-50 | 4 | 4 | 2 | 0.004 | `volume-50--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/volume-50--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/volume-50--dark@1.png) |
| volume-80 | 4 | 4 | 2 | 0.004 | `volume-80--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/volume-80--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/volume-80--dark@1.png) |
| volume-muted | 4 | 4 | 2 | 0.004 | `volume-muted--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/volume-muted--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/volume-muted--dark@1.png) |
| workspace-hover | 4 | 4 | 2 | 0.004 | `workspace-hover--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/workspace-hover--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/workspace-hover--dark@1.png) |
| workspace-named | 4 | 4 | 2 | 0.004 | `workspace-named--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/workspace-named--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/workspace-named--dark@1.png) |
| workspace-urgent | 4 | 4 | 2 | 0.004 | `workspace-urgent--dark@1.png` | - | [heat](../../parity/verify/out/bar-gl/heat/workspace-urgent--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/bar-gl/tri/workspace-urgent--dark@1.png) |

## panels (cpu, provisional)

| state | images | pass | worst max | worst mean | worst image | cause | heatmap |
|---|---|---|---|---|---|---|---|
| calendar-error | 4 | 0 | – | – | – | missing | – |
| calendar-morph | 40 | 0 | – | – | – | missing | – |
| calendar-tooltip | 4 | 0 | – | – | – | missing | – |
| calendar-week | 4 | 0 | – | – | – | missing | – |
| cp-brightness-0 | 4 | 0 | 2 | 0.044 | `cp-brightness-0--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-brightness-0--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-brightness-0--light@1.png) |
| cp-brightness-100 | 4 | 0 | 2 | 0.045 | `cp-brightness-100--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-brightness-100--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-brightness-100--light@1.png) |
| cp-brightness-37 | 4 | 0 | 2 | 0.044 | `cp-brightness-37--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-brightness-37--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-brightness-37--light@1.png) |
| cp-brightness-drag | 4 | 0 | 2 | 0.044 | `cp-brightness-drag--held--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-brightness-drag--held--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-brightness-drag--held--light@1.png) |
| cp-bt-many | 4 | 0 | 3 | 0.314 | `cp-bt-many--light@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-bt-many--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-bt-many--light@2.png) |
| cp-bt-off | 4 | 0 | 2 | 0.275 | `cp-bt-off--dark@1.png` | fill/color | [heat](../../parity/verify/out/panels/heat/cp-bt-off--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-bt-off--dark@1.png) |
| cp-bt-scanning-empty | 4 | 0 | 3 | 0.268 | `cp-bt-scanning-empty--light@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/cp-bt-scanning-empty--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-bt-scanning-empty--light@2.png) |
| cp-bt-three | 4 | 0 | 3 | 0.310 | `cp-bt-three--light@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-bt-three--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-bt-three--light@2.png) |
| cp-hover-toggle | 8 | 0 | 2 | 0.044 | `cp-hover-toggle--dnd--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-hover-toggle--dnd--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-hover-toggle--dnd--light@1.png) |
| cp-main-idle | 4 | 0 | 2 | 0.064 | `cp-main-idle--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-main-idle--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-main-idle--light@1.png) |
| cp-main-off | 4 | 0 | 3 | 0.107 | `cp-main-off--light@2.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-main-off--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-main-off--light@2.png) |
| cp-mihomo-offline | 4 | 4 | 4 | 0.205 | `cp-mihomo-offline--dark@1.png` | - | [heat](../../parity/verify/out/panels/heat/cp-mihomo-offline--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-mihomo-offline--dark@1.png) |
| cp-mihomo-online | 4 | 4 | 4 | 0.217 | `cp-mihomo-online--dark@1.png` | - | [heat](../../parity/verify/out/panels/heat/cp-mihomo-online--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-mihomo-online--dark@1.png) |
| cp-open-close | 44 | 20 | 3 | 0.247 | `cp-open-close--close-t0060--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-open-close--close-t0060--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-open-close--close-t0060--light@1.png) |
| cp-page-switch | 24 | 8 | 3 | 0.310 | `cp-page-switch--switch-t0060--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-page-switch--switch-t0060--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-page-switch--switch-t0060--light@1.png) |
| cp-press-splittile | 12 | 0 | 2 | 0.062 | `cp-press-splittile--press-t0000--light@1.png` | rect AA | [heat](../../parity/verify/out/panels/heat/cp-press-splittile--press-t0000--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-press-splittile--press-t0000--light@1.png) |
| cp-press-toggle | 12 | 0 | 2 | 0.044 | `cp-press-toggle--press-t0000--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-press-toggle--press-t0000--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-press-toggle--press-t0000--light@1.png) |
| cp-vm-running | 4 | 0 | 2 | 0.044 | `cp-vm-running--light@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/cp-vm-running--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/cp-vm-running--light@1.png) |
| qa-brightness-0 | 4 | 0 | 2 | 0.019 | `qa-brightness-0--light@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/qa-brightness-0--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-brightness-0--light@2.png) |
| qa-brightness-100 | 4 | 0 | 2 | 0.028 | `qa-brightness-100--light@1.png` | rect AA | [heat](../../parity/verify/out/panels/heat/qa-brightness-100--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-brightness-100--light@1.png) |
| qa-brightness-50 | 4 | 0 | 2 | 0.022 | `qa-brightness-50--light@1.png` | rect AA | [heat](../../parity/verify/out/panels/heat/qa-brightness-50--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-brightness-50--light@1.png) |
| qa-show-hide | 28 | 16 | 7 | 0.460 | `qa-show-hide--show-t0060--dark@2.png` | rect AA | [heat](../../parity/verify/out/panels/heat/qa-show-hide--show-t0060--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-show-hide--show-t0060--dark@2.png) |
| qa-volume-0-muted | 4 | 0 | 2 | 0.018 | `qa-volume-0-muted--light@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/qa-volume-0-muted--light@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-volume-0-muted--light@2.png) |
| qa-volume-100 | 4 | 0 | 2 | 0.031 | `qa-volume-100--light@1.png` | rect AA | [heat](../../parity/verify/out/panels/heat/qa-volume-100--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-volume-100--light@1.png) |
| qa-volume-30 | 4 | 0 | 2 | 0.020 | `qa-volume-30--light@1.png` | rect AA | [heat](../../parity/verify/out/panels/heat/qa-volume-30--light@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/qa-volume-30--light@1.png) |
| startup-confirm | 4 | 4 | 2 | 0.031 | `startup-confirm--dark@1.png` | - | [heat](../../parity/verify/out/panels/heat/startup-confirm--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/startup-confirm--dark@1.png) |
| startup-confirm-hover | 4 | 4 | 2 | 0.032 | `startup-confirm-hover--dark@1.png` | - | [heat](../../parity/verify/out/panels/heat/startup-confirm-hover--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/startup-confirm-hover--dark@1.png) |
| startup-confirm-press | 4 | 4 | 2 | 0.031 | `startup-confirm-press--dark@1.png` | - | [heat](../../parity/verify/out/panels/heat/startup-confirm-press--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/startup-confirm-press--dark@1.png) |
| stats-0 | 4 | 0 | 2 | 0.471 | `stats-0--dark@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/stats-0--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/stats-0--dark@2.png) |
| stats-1 | 4 | 0 | 2 | 0.449 | `stats-1--dark@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/stats-1--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/stats-1--dark@2.png) |
| stats-120 | 4 | 0 | 3 | 0.413 | `stats-120--dark@1.png` | fill/color | [heat](../../parity/verify/out/panels/heat/stats-120--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/stats-120--dark@1.png) |
| stats-offline | 4 | 0 | 2 | 0.438 | `stats-offline--dark@2.png` | fill/color | [heat](../../parity/verify/out/panels/heat/stats-offline--dark@2.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/stats-offline--dark@2.png) |
| wifi-connected-4 | 4 | 0 | 2 | 0.313 | `wifi-connected-4--dark@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/wifi-connected-4--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/wifi-connected-4--dark@1.png) |
| wifi-expanded-secure | 4 | 0 | 2 | 0.322 | `wifi-expanded-secure--dark@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/wifi-expanded-secure--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/wifi-expanded-secure--dark@1.png) |
| wifi-many | 4 | 0 | 2 | 0.316 | `wifi-many--dark@1.png` | fill/color, rect AA | [heat](../../parity/verify/out/panels/heat/wifi-many--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/wifi-many--dark@1.png) |
| wifi-off | 4 | 0 | 2 | 0.279 | `wifi-off--dark@1.png` | fill/color | [heat](../../parity/verify/out/panels/heat/wifi-off--dark@1.png) · [ref\|ours\|heat](../../parity/verify/out/panels/tri/wifi-off--dark@1.png) |

Per-image metrics (max, mean, pixel counts, IoU, bbox shift, attribution shares): `parity/verify/out/results.json`.
