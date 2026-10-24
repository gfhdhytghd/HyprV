//! Theme/color/layout conventions from ui-panels.md §1 and components.md §0.
use hyprv_ui::{animation::Color, geometry::Rect};
pub const BASE_FONT: &str = "JetBrainsMono Nerd Font";
pub fn parse_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    let (a, r, g, b) = match hex.len() {
        6 => (255, v >> 16, (v >> 8) & 255, v & 255),
        8 => (v >> 24, (v >> 16) & 255, (v >> 8) & 255, v & 255),
        _ => return None,
    };
    Some(Color::rgba(
        r as f32 / 255.,
        g as f32 / 255.,
        b as f32 / 255.,
        a as f32 / 255.,
    ))
}
pub fn rgb(value: &str) -> Color {
    parse_color(value).expect("literal Qt color")
}
pub fn with_alpha(mut color: Color, alpha: f32) -> Color {
    color.a = alpha.clamp(0., 1.);
    color
}
pub fn panel_color(mut color: Color) -> Color {
    color.a *= 0.82;
    color
}
/// Legacy component alignment. Some ports combine this rule with independently
/// snapped native text; migrate those call sites against their Qt references.
/// New Qt 6 anchors use `hyprv_ui::geometry::qt_center_half` / `qt_centered`.
pub fn hcenter(value:f32)->f32{if (value as i64)&1==1{(value+1.)/2.}else{value/2.}}
pub use hyprv_ui::geometry::{qt_center_half,qt_centered};
pub fn centered(parent: Rect, size: (f32, f32)) -> Rect {
    Rect::new(
        parent.x + hcenter(parent.width) - hcenter(size.0),
        parent.y + hcenter(parent.height) - hcenter(size.1),
        size.0,
        size.1,
    )
}
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub island_soft_fill: Color,
    pub island_stroke: Color,
    pub island_hover_fill: Color,
    pub island_muted_text: Color,
    pub accent_soft: Color,
    pub input_fill: Color,
    pub accent_fill: Color,
    pub accent_stroke: Color,
    pub chip_fill: Color,
    pub dark: bool,
    pub primary_text: Color,
    pub module_background: Color,
    pub accent: Color,
    pub glass_fill: Color,
    pub glass_stroke: Color,
    pub card_fill: Color,
    pub card_strong_fill: Color,
    pub card_stroke: Color,
    pub muted_text: Color,
    pub soft_text: Color,
    pub active_fill: Color,
    pub active_stroke: Color,
    pub brightness: Color,
    pub battery: Color,
    pub microphone: Color,
    pub critical: Color,
    pub usage_low: Color,
    pub usage_medium: Color,
    pub chart_accent: Color,
    pub workspace_hover: Color,
    pub active_workspace: Color,
    pub active_workspace_text: Color,
    pub muted_workspace_text: Color,
}
impl Default for Palette {
    fn default() -> Self {
        Self::new(false)
    }
}
impl Palette {
    pub fn new(dark: bool) -> Self {
        let primary_text = rgb(if dark { "#cdd6f4" } else { "#2b2b2c" });
        let accent = rgb(if dark { "#89b4fa" } else { "#407cdd" });
        let active_workspace = rgb(if dark { "#8789c1" } else { "#8eb6ec" });
        Self {
            island_soft_fill: with_alpha(rgb("#ffffff"), if dark { 0.07 } else { 0.23 }),
            island_stroke: with_alpha(primary_text, if dark { 0.13 } else { 0.10 }),
            island_hover_fill: with_alpha(primary_text, if dark { 0.10 } else { 0.12 }),
            island_muted_text: with_alpha(primary_text, if dark { 0.68 } else { 0.62 }),
            accent_soft: with_alpha(accent, if dark { 0.72 } else { 0.78 }),
            input_fill: with_alpha(
                rgb(if dark { "#000000" } else { "#ffffff" }),
                if dark { 0.20 } else { 0.34 },
            ),
            accent_fill: with_alpha(primary_text, if dark { 0.10 } else { 0.08 }),
            accent_stroke: with_alpha(primary_text, if dark { 0.18 } else { 0.14 }),
            chip_fill: with_alpha(primary_text, 0.08),
            dark,
            primary_text,
            accent,
            module_background: with_alpha(rgb(if dark { "#1e1e2e" } else { "#e7e7ec" }), 0.8),
            glass_fill: with_alpha(
                rgb(if dark { "#101214" } else { "#ffffff" }),
                if dark { 0.42 } else { 0.28 },
            ),
            glass_stroke: with_alpha(primary_text, if dark { 0.14 } else { 0.10 }),
            card_fill: with_alpha(rgb("#ffffff"), if dark { 0.07 } else { 0.22 }),
            card_strong_fill: with_alpha(rgb("#ffffff"), if dark { 0.11 } else { 0.30 }),
            card_stroke: with_alpha(primary_text, if dark { 0.12 } else { 0.08 }),
            muted_text: with_alpha(primary_text, 0.68),
            soft_text: with_alpha(primary_text, 0.44),
            active_fill: with_alpha(accent, if dark { 0.28 } else { 0.22 }),
            active_stroke: with_alpha(accent, if dark { 0.42 } else { 0.28 }),
            brightness: rgb(if dark { "#f3b35c" } else { "#d47b1f" }),
            battery: rgb(if dark { "#a6e3a1" } else { "#1d7715" }),
            microphone: rgb(if dark { "#cba6f7" } else { "#ad6bfd" }),
            critical: rgb("#e92d4d"),
            usage_low: rgb(if dark { "#7ad48b" } else { "#2f9e44" }),
            usage_medium: rgb(if dark { "#f2d36b" } else { "#c99700" }),
            chart_accent: rgb(if dark { "#d7a26a" } else { "#b9782f" }),
            workspace_hover: if dark {
                rgb("#000000")
            } else {
                active_workspace
            },
            active_workspace,
            active_workspace_text: rgb(if dark { "#0c0d14" } else { "#1b1b1b" }),
            muted_workspace_text: rgb(if dark { "#575b6a" } else { "#859abf" }),
        }
    }
    pub fn icon_well_fill(&self, active: bool) -> Color {
        with_alpha(
            rgb("#ffffff"),
            match (self.dark, active) {
                (true, true) => 0.14,
                (true, false) => 0.11,
                (false, true) => 0.26,
                (false, false) => 0.32,
            },
        )
    }
    pub fn severity(&self, value: f64) -> Color {
        if !value.is_finite() {
            self.primary_text
        } else if value >= 90. {
            self.critical
        } else if value >= 60. {
            self.usage_medium
        } else {
            self.usage_low
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutItem {
    pub width: f32,
    pub height: f32,
    pub visible: bool,
}
/// Qt positioners skip hidden and zero-area children without inserting spacing.
pub fn row(items: &[LayoutItem], spacing: f32) -> Vec<Option<Rect>> {
    let mut x = 0.;
    let mut first = true;
    items
        .iter()
        .map(|i| {
            if !i.visible || i.width <= 0. || i.height <= 0. {
                return None;
            }
            if !first {
                x += spacing;
            }
            first = false;
            let r = Rect::new(x, 0., i.width, i.height);
            x += i.width;
            Some(r)
        })
        .collect()
}
pub fn column(items: &[LayoutItem], spacing: f32) -> Vec<Option<Rect>> {
    row(
        &items
            .iter()
            .map(|i| LayoutItem {
                width: i.height,
                height: i.width,
                visible: i.visible,
            })
            .collect::<Vec<_>>(),
        spacing,
    )
    .into_iter()
    .map(|r| r.map(|r| Rect::new(0., r.x, r.height, r.width)))
    .collect()
}

pub use crate::color_math::{darker, lighter};
