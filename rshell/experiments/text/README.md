# Text-rendering parity experiment

Findings and the implementation recipe live in `docs/spec/text-rendering.md`. This directory
only holds the tools used to derive and verify them.

| path | what |
|---|---|
| `qt-ref/` | `textref` (C++/Qt 6.12): renders a QML scene with Qt Quick's RHI/OpenGL renderer through an unmapped Wayland window (`TEXTREF_NO_SHOW=1 QT_QPA_PLATFORM=wayland`) and dumps per-Text font/layout facts. `--dumpdf` writes Qt's own `QDistanceField` bytes. |
| `skia-text/` | `textcmp` (Rust, skia-safe 0.90 + freetype-rs): standalone crate (own `[workspace]`), re-implements Qt's layout and both rasterisers and diffs against the Qt PNG. Methods: `para` (today's hyprv-ui Paragraph path), `skia` (skia glyph rasteriser with Qt placement), `ft` (FreeType, Qt-exact), `df` (distance-field emulation). |
| `scene.json` | the samples (bar clock, icons, window title, Chinese battery strings, calendar strings, letterSpacing, elide); `scene-{light,dark}.qml` are the same samples as QML |
| `bar-labels.qml` | every bar label, to read Qt's implicit sizes |
| `run.sh` | rebuild + reproduce all numbers; outputs in `out/` (gitignore-worthy) |

`out/cmp-*/` holds alpha-plane PNGs (`qt.png`, `para.png`, `skia.png`, `ft.png`, `df.png`).
