//! AnimatedGlassPanel.qml:13-217 and AnimatedReveal.qml:12-178.
use crate::{
    foundations::with_alpha,
    primitives::{clip_box, rounded_rect},
};
use hyprv_ui::{
    animation::{Color, Direction, Easing, Family},
    effects::MultiEffect,
    geometry::Rect,
    scene::{Border, Content, Node},
};
use std::time::Duration;
#[derive(Clone, Debug)]
pub struct RevealTiming {
    pub open_pause: u64,
    pub open_reveal: u64,
    pub content_delay: u64,
    pub open_fade: u64,
    pub open_slide: u64,
    pub open_offset: f32,
    pub close_pause: u64,
    pub close_reveal: u64,
    pub close_fade: u64,
    pub close_slide: u64,
    pub close_offset: f32,
}
impl Default for RevealTiming {
    fn default() -> Self {
        Self {
            open_pause: 0,
            open_reveal: 200,
            content_delay: 20,
            open_fade: 140,
            open_slide: 180,
            open_offset: -8.,
            close_pause: 0,
            close_reveal: 180,
            close_fade: 90,
            close_slide: 150,
            close_offset: -8.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Open,
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RevealSample {
    pub height: f32,
    pub opacity: f32,
    pub offset: f32,
}
#[derive(Clone, Debug)]
pub struct Reveal {
    pub full_height: f32,
    pub line_height: f32,
    pub layout_height_follows_reveal: bool,
    pub timing: RevealTiming,
    values: RevealSample,
    animation: Option<(Transition, Duration, RevealSample, f32)>,
}
impl Reveal {
    pub fn new(full_height: f32) -> Self {
        Self {
            full_height,
            line_height: 2.,
            layout_height_follows_reveal: false,
            timing: RevealTiming::default(),
            values: RevealSample {
                height: full_height,
                opacity: 1.,
                offset: 0.,
            },
            animation: None,
        }
    }
    pub fn reset(&mut self) {
        self.animation = None;
        self.values = RevealSample {
            height: self.full_height,
            opacity: 1.,
            offset: 0.,
        };
    }
    pub fn prepare_open(&mut self) {
        self.animation = None;
        self.values = RevealSample {
            height: 0.,
            opacity: 0.,
            offset: self.timing.open_offset,
        };
    }
    pub fn collapse_immediately(&mut self) {
        self.prepare_open();
    }
    pub fn play_open(&mut self, now: Duration) {
        self.advance(now);
        self.animation = Some((Transition::Open, now, self.values, self.full_height));
    }
    pub fn play_close(&mut self, now: Duration) {
        self.advance(now);
        self.animation = Some((Transition::Close, now, self.values, self.line_height));
    }
    pub fn is_running(&self) -> bool {
        self.animation.is_some()
    }
    pub fn values(&self) -> RevealSample {
        self.values
    }
    pub fn advance(&mut self, now: Duration) -> Option<Transition> {
        let (mode, start, from, to) = self.animation?;
        let elapsed = now.saturating_sub(start).as_millis() as u64;
        let t = &self.timing;
        let mix = |a: f32, b: f32, delay: u64, duration: u64, f: Family, d: Direction| {
            let p = if elapsed < delay {
                0.
            } else if duration == 0 {
                1.
            } else {
                (elapsed - delay) as f64 / duration as f64
            };
            a + (b - a) * Easing::new(f, d).value(p) as f32
        };
        let total = match mode {
            Transition::Open => {
                self.values = RevealSample {
                    height: mix(
                        from.height,
                        to,
                        t.open_pause,
                        t.open_reveal,
                        Family::Cubic,
                        Direction::Out,
                    ),
                    opacity: mix(
                        from.opacity,
                        1.,
                        t.open_pause + t.content_delay,
                        t.open_fade,
                        Family::Quad,
                        Direction::Out,
                    ),
                    offset: mix(
                        from.offset,
                        0.,
                        t.open_pause + t.content_delay,
                        t.open_slide,
                        Family::Cubic,
                        Direction::Out,
                    ),
                };
                t.open_pause
                    + t.open_reveal
                        .max(t.content_delay + t.open_fade.max(t.open_slide))
            }
            Transition::Close => {
                self.values = RevealSample {
                    height: mix(
                        from.height,
                        to,
                        t.close_pause,
                        t.close_reveal,
                        Family::Cubic,
                        Direction::In,
                    ),
                    opacity: mix(
                        from.opacity,
                        0.,
                        0,
                        t.close_fade,
                        Family::Quad,
                        Direction::In,
                    ),
                    offset: mix(
                        from.offset,
                        t.close_offset,
                        0,
                        t.close_slide,
                        Family::Cubic,
                        Direction::In,
                    ),
                };
                t.close_fade
                    .max(t.close_slide)
                    .max(t.close_pause + t.close_reveal)
            }
        };
        if elapsed >= total {
            self.animation = None;
            if mode == Transition::Open {
                self.values = RevealSample {
                    height: self.full_height,
                    opacity: 1.,
                    offset: 0.,
                };
            }
            Some(mode)
        } else {
            None
        }
    }
    pub fn set_full_height(&mut self, height: f32) {
        if self.full_height != height {
            self.full_height = height;
            if !self.is_running() {
                self.reset();
            }
        }
    }
    pub fn node(&self, frame: Rect, children: Vec<Node>) -> Node {
        let height = if self.layout_height_follows_reveal {
            self.values.height
        } else {
            self.full_height
        };
        let mut content = Node::new(
            Rect::new(0., self.values.offset, frame.width, self.full_height),
            Content::Group,
        );
        content.opacity = self.values.opacity;
        content.children = children;
        let viewport = clip_box(
            Rect::new(0., 0., frame.width, self.values.height.max(0.)),
            vec![content],
        );
        let mut root = Node::new(
            Rect::new(frame.x, frame.y, frame.width, height),
            Content::Group,
        );
        root.children.push(viewport);
        root
    }
}
#[derive(Clone, Debug)]
pub struct GlassStyle {
    pub radius: f32,
    pub fill: Color,
    pub stroke: Color,
    pub surface_opacity: f32,
    pub shadow: Color,
}
impl Default for GlassStyle {
    fn default() -> Self {
        Self {
            radius: 19.,
            fill: crate::foundations::rgb("#202020"),
            stroke: crate::foundations::rgb("#404040"),
            surface_opacity: 0.82,
            shadow: Color::default(),
        }
    }
}
/// Glass chrome preserves the anchored-content slide no-op and rectangular clipping.
pub fn glass_panel(frame: Rect, reveal: &Reveal, style: &GlassStyle, children: Vec<Node>) -> Node {
    let v = reveal.values();
    let full = Rect::new(0., 0., frame.width, reveal.full_height);
    let mut chrome = rounded_rect(
        full,
        style.radius,
        with_alpha(style.fill, style.fill.a * style.surface_opacity),
        Some(Border {
            width: 1.,
            color: with_alpha(style.stroke, style.stroke.a * style.surface_opacity),
        }),
    );
    if style.shadow.a > 0. && style.surface_opacity > 0. {
        chrome.effects.multi_effect = Some(MultiEffect {
            shadow_enabled: true,
            shadow_blur: 10. / 64.,
            shadow_color: style.shadow,
            ..Default::default()
        });
    }
    let mut content = Node::new(full, Content::Group);
    content.opacity = v.opacity;
    content.children = children;
    let panel_frame = clip_box(full, vec![chrome, content]);
    let viewport = clip_box(
        Rect::new(0., 0., frame.width, v.height.max(0.)),
        vec![panel_frame],
    );
    let mut root = Node::new(frame, Content::Group);
    root.children.push(viewport);
    root
}
pub fn group_pill(frame: Rect, fill: Color, children: Vec<Node>) -> Node {
    let mut n = rounded_rect(frame, 24., fill, None);
    n.children = children;
    n
}
#[derive(Clone, Copy, Debug)]
pub enum PopupPlacement {
    AnchorCentered,
    RightAligned { margin: f32 },
    FixedLeft { x: f32 },
    AnchorRight { margin: f32 },
}
/// Anchor is output-local. Vertical fallback follows shell.qml:861-882.
pub fn popup_position(
    output: (f32, f32),
    size: (f32, f32),
    anchor: Rect,
    placement: PopupPlacement,
    gap: f32,
) -> Rect {
    let (w, h) = output;
    let (cw, ch) = size;
    let x = match placement {
        PopupPlacement::AnchorCentered => (anchor.x + (anchor.width / 2.).round() - cw / 2.)
            .round()
            .clamp(8., (w - cw - 8.).max(8.)),
        PopupPlacement::RightAligned { margin } => (w - cw - margin).max(margin),
        PopupPlacement::FixedLeft { x } => x,
        PopupPlacement::AnchorRight { margin } => (anchor.x + anchor.width - cw)
            .round()
            .clamp(margin, (w - cw - margin).max(margin)),
    };
    let below = (anchor.y + anchor.height + gap).round();
    let above = (anchor.y + anchor.height - ch - gap).round();
    let y = if below + ch <= h - 8. || above < 8. {
        below.min(h - ch - 8.).max(8.)
    } else {
        above.max(8.)
    };
    Rect::new(x, y, cw, ch)
}

/// Event-driven ExpandableSection: a measured-height change releases a pending open.
pub struct ExpandableSection {
    pub reveal: Reveal,
    pub expanded: bool,
    pending: bool,
}
impl ExpandableSection {
    pub fn new(full_height: f32, expanded: bool) -> Self {
        let mut reveal = Reveal::new(full_height);
        reveal.line_height = 0.;
        reveal.layout_height_follows_reveal = true;
        if !expanded {
            reveal.collapse_immediately();
        }
        Self {
            reveal,
            expanded,
            pending: false,
        }
    }
    pub fn set_expanded(&mut self, expanded: bool, now: Duration) {
        if self.expanded == expanded {
            return;
        }
        self.expanded = expanded;
        if expanded {
            if self.reveal.full_height <= 0. {
                self.pending = true;
            } else {
                self.reveal.prepare_open();
                self.reveal.play_open(now);
            }
        } else {
            self.pending = false;
            self.reveal.play_close(now);
        }
    }
    pub fn set_full_height(&mut self, height: f32, now: Duration) {
        self.reveal.full_height = height;
        if self.expanded && self.pending && height > 0. {
            self.pending = false;
            self.reveal.prepare_open();
            self.reveal.play_open(now);
        } else if self.expanded && !self.reveal.is_running() {
            self.reveal.reset();
        }
    }
}
pub struct MorphBox {
    pub width: hyprv_ui::animation::Behavior<f64>,
    pub height: hyprv_ui::animation::Behavior<f64>,
    pub radius: hyprv_ui::animation::Behavior<f64>,
}
impl MorphBox {
    pub fn new(width: f64, height: f64, radius: f64, duration: Duration, easing: Easing) -> Self {
        Self {
            width: hyprv_ui::animation::Behavior::new(width, duration, easing.clone()),
            height: hyprv_ui::animation::Behavior::new(height, duration, easing.clone()),
            radius: hyprv_ui::animation::Behavior::new(radius, duration, easing),
        }
    }
    pub fn retarget(&mut self, size: (f64, f64), radius: f64, enabled: bool, now: Duration) {
        self.width.enabled = enabled;
        self.height.enabled = enabled;
        self.radius.enabled = enabled;
        self.width.retarget(size.0, now);
        self.height.retarget(size.1, now);
        self.radius.retarget(radius, now);
    }
    pub fn advance(&mut self, now: Duration) -> (f32, f32, f32) {
        (
            self.width.advance(now) as f32,
            self.height.advance(now) as f32,
            self.radius.advance(now) as f32,
        )
    }
    pub fn is_running(&self) -> bool {
        self.width.is_running() || self.height.is_running() || self.radius.is_running()
    }
}
/// Stable keyed identities, including moves, without resetting the container's scroll transform.
pub struct KeyedRepeater<K> {
    pub container: hyprv_ui::scene::NodeId,
    keys: Vec<K>,
}
impl<K: Clone + Eq + std::hash::Hash> KeyedRepeater<K> {
    pub fn attach(parent: &mut Node) -> Self {
        Self {
            container: parent.add(Node::new(Rect::default(), Content::Group)),
            keys: Vec::new(),
        }
    }
    pub fn reconcile(
        &mut self,
        scene: &mut hyprv_ui::scene::Scene,
        keys: &[K],
        mut build: impl FnMut(&K) -> Node,
        mut update: impl FnMut(&K, &mut Node),
    ) -> Result<(), &'static str> {
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        if unique.len() != keys.len() {
            return Err("duplicate repeater key");
        }
        let previous = std::mem::take(&mut self.keys);
        let found = scene.edit(self.container, |root| {
            let mut old: std::collections::HashMap<_, _> = previous
                .into_iter()
                .zip(std::mem::take(&mut root.children))
                .collect();
            root.children = keys
                .iter()
                .map(|key| {
                    let mut node = old.remove(key).unwrap_or_else(|| build(key));
                    update(key, &mut node);
                    node
                })
                .collect();
        });
        if !found {
            return Err("repeater container missing");
        }
        self.keys = keys.to_vec();
        Ok(())
    }
}
