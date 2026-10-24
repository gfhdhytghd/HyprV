Qt6.12 RHI/OpenGL reference PNGs, independently captured by the unmodified parity
host from `hyprv-ui-bar/tools/native_alpha.qml`. Each image has16 cells: color
alpha1/.72/.14/.98 crossed with inherited opacity1/.82/.45/.13. DPR1/2 and
transparent/translucent backgrounds make64 exact cases. Text is native-rendered
JetBrainsMono Nerd Font17 bold. Source/report/provenance:
`hyprv-ui-bar/artifacts/alpha-qrgb/{qt,metrics.json}`.

QQuickText::setColor stores QColor::rgba() (QRgb8), then the material decodes and
premultiplies by inherited opacity. Runtime has no Qt dependency. Goldens were
captured before enabling translucent direct GL in production, not copied from
Rust output.
