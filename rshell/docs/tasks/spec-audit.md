# Task: spec-audit
All UI specs now exist: docs/spec/ui-bar*.md, ui-panels*.md, ui-island.md (written by three different agents) plus services*.md.
Audit them against the QML source (/home/wilf/.config/HyprV/quickshell, read-only) with fresh eyes:
1. Build docs/spec/COVERAGE.md: for every .qml file (and line ranges of shell.qml) that is actually loaded at runtime, which spec section covers it. Flag gaps.
2. Check cross-spec consistency: shared components (AnimatedGlassPanel, theme tokens, fonts, Behavior semantics, easing, surface namespaces, popup positioning, how panels/island/calendar morph between each other) must be described identically; resolve contradictions by re-reading QML and fix the spec text in place (mark each fix with `<!-- audit: ... -->`).
3. Spot-check 30 random numeric values (sizes, durations, colors) in other agents' specs against QML; report error rate in COVERAGE.md.
4. Write docs/spec/components.md: the deduplicated list of reusable visual primitives/widgets the Rust UI layer should provide (e.g. GlassPanel, Pill, Slider, Toggle, IconButton, ProgressRing, Chart…) with their parameters, referencing spec sections. This becomes the widget-kit work list.
Rules: only edit docs/spec/. No sudo, no git commit.
When done, reply with a short summary.
