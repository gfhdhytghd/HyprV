# Qt text regression fixtures

`scene.json` and the three PNGs are the original unmapped Qt 6.12 RHI/OpenGL
samples from experiments/text. `tests/text.rs` checks each sample independently:
39 NativeRendering comparisons require zero alpha differences; 18 QtRendering
comparisons require maximum alpha error <=1 and ink-weighted error <0.2%.
These tolerances were tightened after porting the complete Qt glyph cache.

`qt-cache-{jb,cjk}.json` are independent Qt QDistanceField glyph cache dumps,
covering 14 Latin/CJK glyphs. `src/text.rs` private tests compare the actual
standalone distance-field output against their bytes, maximum difference <=1.
To generate fresh dumps using the development-only Qt oracle (example text):

```
experiments/text/qt-ref/textref --dumpdf 'JetBrainsMono Nerd Font' 700 'AegR0→3' /tmp/jb.json
experiments/text/qt-ref/textref --dumpdf 'HarmonyOS Sans SC' 400 '中文测试字体' /tmp/cjk.json
cargo test -p hyprv-ui --test text -- --nocapture
cargo test -p hyprv-ui --lib qt_distance_field_cache_bytes
```

The oracle uses Qt and is never linked by the production shell. The Rust/standard
C++ port preserves Qt's fixed-point path simplifier and distance kernels plus the
FreeType conic-outline walk. These fixed-font, single-line samples do not prove
arbitrary fonts, multiline text, emoji, transforms or whole-component parity.
