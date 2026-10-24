# Independent Qt stroke coverage oracle

`../../canvas-stroke-oracle.cpp` uses system Qt6.12 QPainter and QImage
ARGB32_Premultiplied, not the engine's stroker, to generate these84 PNGs and
original double-precision path elements. Seven shapes × three caps × four DPRs
(1/1.25/1.5/2): partial arc, ellipse, round-join polyline, diagonal, arbitrary cubic,
1px horizontal grid, and track+progress drawn sequentially into one image.

Regenerate from rshell:

```sh
c++ -std=c++17 crates/hyprv-ui/tests/canvas-stroke-oracle.cpp $(pkg-config --cflags --libs Qt6Gui Qt6Core) -o /tmp/hyprv-canvas-oracle
QT_QPA_PLATFORM=offscreen /tmp/hyprv-canvas-oracle crates/hyprv-ui/tests/data/canvas-strokes
cargo test -p hyprv-ui --test canvas_strokes
```

The test decodes PNGs to premultiplied RGBA8888 and requires max channel diff0.
Set HYPRV_PARITY_BACKEND=gl and WAYLAND_DISPLAY to an **already isolated** private
compositor to exercise the production EGL/GL RGBA8 renderer instead of direct
Canvas rasterization. Both paths passed all84 cases in the engine checkpoint.
The shell never links Qt; Qt is required only to regenerate independent goldens.
