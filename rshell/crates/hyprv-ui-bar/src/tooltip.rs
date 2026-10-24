//! Laptop TextModule tooltip (laptop-bar.md §4, laptop shell.qml:576-649): the CPU-power
//! module's delayed glass popup with the CPU temperature. Pure builder; the host maps it
//! as an input-less xdg_popup of the bar surface.
use crate::{format, layout, model::*, popups, view};
use hyprv_ui::{
    geometry::{qt_center_half as half, Rect},
    scene::{Content, Node, RenderType},
};
use hyprv_widgets::foundations::{rgb, with_alpha, Palette};
use std::time::Duration;

/// `Timer tooltipDelay { interval: 350; running: mouseArea.containsMouse }`.
pub const DELAY: Duration = Duration::from_millis(350);
const TITLE: &str = "CPU 温度";

fn temperature_text(m: &Model) -> String {
    format!("{} °C", format::js_round(m.stats.temperature_c.unwrap_or(0.)))
}

/// Popup rectangle in bar-surface coordinates, or None when not shown.
pub fn rect(m: &Model) -> Option<Rect> {
    if !m.laptop() || !m.tooltip_visible || m.hovered != Some(Target::CpuPower) {
        return None;
    }
    let module = layout::compute(m, &m.theme).target(&Target::CpuPower)?;
    let (tw, th) = layout::measure(TITLE, 11., 400);
    let (vw, vh) = layout::measure(&temperature_text(m), 18., 600);
    let row_w = 36. + 5. + tw.max(vw);
    let row_h = (th + vh).max(36.);
    let (w, h) = (row_w + 20., row_h + 24.);
    Some(Rect::new(
        module.x + format::js_round(((module.width - w) / 2.) as f64) as f32,
        module.y + module.height + 9.,
        w,
        h,
    ))
}

/// Full-output root holding the positioned popup, like the other bar popups.
pub fn build(m: &Model, theme: &Theme) -> Option<Node> {
    let frame = rect(m)?;
    let p = Palette::new(theme.dark);
    let t = m.stats.temperature_c.unwrap_or(0.);
    let (tw, th) = layout::measure(TITLE, 11., 400);
    let value = temperature_text(m);
    let (vw, vh) = layout::measure(&value, 18., 600);
    let row_h = (th + vh).max(36.);
    let mut row = view::group(Rect::new(8., 12., frame.width - 20., row_h));
    let mut icon_box = view::group(Rect::new(0., half(row_h) - half(36.), 36., 36.));
    let icon_color = rgb(if t < 70. {
        "#2eaa45"
    } else if t <= 95. {
        "#e5b629"
    } else {
        "#e5484d"
    });
    let (iw, ih) = layout::measure("\u{f2c9}", 21., 400);
    let mut icon = view::text(
        "\u{f2c9}",
        Rect::new(half(36.) - half(iw), half(36.) - half(ih), iw, ih),
        21.,
        400,
        icon_color,
        false,
    );
    if QUIRK_TOOLTIP_ICON_DF_TEXT {
        if let Content::Text(text) = &mut icon.content {
            text.render_type = RenderType::QtRendering;
        }
        icon.native_text_mode = Default::default();
    }
    icon_box.add(icon);
    row.add(icon_box);
    let mut column = view::group(Rect::new(41., 0., tw.max(vw), th + vh));
    let native = |mut n: Node| {
        if let Content::Text(text) = &mut n.content {
            text.render_type = RenderType::NativeRendering;
        }
        n
    };
    column.add(native(view::text(
        TITLE,
        Rect::new(0., 0., tw, th),
        11.,
        400,
        with_alpha(p.primary_text, 0.65),
        false,
    )));
    column.add(native(view::text(
        &value,
        Rect::new(0., th, vw, vh),
        18.,
        600,
        p.primary_text,
        false,
    )));
    row.add(column);
    let mut root = view::group(Rect::new(0., 0., m.output.width, m.output.height));
    root.native_materials = Some(true);
    root.add(popups::glass(frame, theme, vec![row], m.output.scale));
    Some(root)
}
