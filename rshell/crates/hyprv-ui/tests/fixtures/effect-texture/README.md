# Independent Qt RHI layer texture sizing golden

Generated with Qt6.12.0 OpenGL/RHI using an owned parity-host copy with read-only texture diagnostics and the owned
`crates/hyprv-ui-bar/tools/rect_effect.qml` diagnostic scene, scales1/2. QML sets
layer.textureSize=round(300*DPR),round(230*DPR), and a MultiEffect shadow with
its default32px auto padding. Texture trace confirms the padded364×294 source
region is squeezed into the unpadded layer texture. Source colors, opacity and
geometry are reproduced in effect_texture.rs. Neither shared QML nor references
were edited. Reproduction: bar/tools/check_rect_effect.py; original run hashes
are retained in bar/artifacts/rect-effect/metrics.json. PNGs are straight RGBA;
the test compares premultiplied bytes. Before the fix max13/16; after max1 at
both DPRs, painted means below0.01. This oracle regression is not a full UI gate.
