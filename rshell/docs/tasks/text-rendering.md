# Task: text-rendering parity research
Write /home/wilf/.config/HyprV/rshell/docs/spec/text-rendering.md (you may also add small experiments under rshell/experiments/text/).
Pixel parity depends on reproducing Qt Quick text exactly with skia-safe (textlayout). Determine, from the live environment (read-only) and Qt sources/docs:
- Which renderType the QML Text items use (QtRendering distance-field vs NativeRendering/CurveRendering), QT_* env vars set by quickshell/scripts/launch.sh, fontconfig settings (hinting, hintstyle, antialias, rgba/subpixel, lcdfilter, embeddedbitmap) actually in effect for each family used (fc-match -v).
- How Qt computes baseline, line height, ascent/descent, glyph advance rounding (design metrics vs hinted), letterSpacing, elide, and how distance-field AA looks vs FreeType gray AA at fractional scale (wp_fractional_scale) — and what QSG settings change it.
- The exact skia-safe configuration (FontMgr, Font edging/hinting/subpixel/linear-metrics/baseline-snap, paragraph style, strut) that matches; if distance-field text is used, propose how to emulate (skia SDF? or switch QML reference to NativeRendering is NOT allowed — we must match what the user sees).
- Build a tiny experiment: render the same strings (bar clock, Chinese battery strings, icon glyphs) via the parity harness (rshell/parity, being built by agent spec-island — coordinate by reading its README; don't edit its files) or a standalone qml6 offscreen script vs a skia-safe test, diff them, and iterate until the diff is minimal. Report the residual diff numbers.
Rules: no sudo, no git commit, don't touch ../quickshell or the running session.
When done, reply with a short summary.
