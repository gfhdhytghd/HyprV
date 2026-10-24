# Production tray provider sizing

Production `assets::themed_icon` now receives an already-scaled device-pixel
request (`round(18 × DPR)`), with theme selection fixed at scale 1. It uses
`hyprv_widgets::icon_theme::tray_raster` for raster entries and the same request
for SVG/custom paths. At DPR 2 a large raster/SVG is rendered to 36 px, replacing
the erroneous 72 px request; smaller theme rasters remain unscaled. Supplied SNI
pixmaps retain their existing `ceil(18 × DPR)` selection rule.

Both production async delivery and deterministic fixture rendering call this
shared helper. The worker limits, bounded reads, generation checks and metadata
cache identity remain in place. Cache keys retain rounded provider request and
ceil-based supplied-pixmap target, including fractional scales.

Validation: focused assets tests13 passed,0 failed,1 diagnostic ignored; shell
library48 passed,0 failed,1 diagnostic ignored; scoped all-targets Clippy passed
with `-D warnings`. Tests cover async DPR 2 completion at36×36, fractional cache
reuse versus fresh decoding, raster downscaling, metadata identity and obsolete
worker replies. Logs: `target/reveal-idle/tray-tests.log`,
`shell-tests-final.log`, `clippy-final.log`. No live shell was restarted.

The aggregate widgets run also reports an existing controls-owned reference
probe mismatch (`tray_fallback_letter_matches_bar`, four theme/scale crops).
The probe assumes letter Z for a nonempty unknown icon; its owner was notified
in `docs/requests/widgets.md`. This is not counted as a passing aggregate run.

The corrected pinned production release also passes204/204 private-compositor
native-input cases. Six fresh-process battery cycles pass285/285 measured render
frames under16.7ms (max6.346ms); these are functional/performance checks, not a
new all-reference visual parity gate. Raw manifest:
`target/reveal-idle/functional-current/manifest.json`.
