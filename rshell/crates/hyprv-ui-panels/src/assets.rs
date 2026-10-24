//! Asset loading helpers for the host (the UI never reads files itself).
use hyprv_ui::skia::{svg, FontMgr};

/// Parse an SVG the way QtSvg renders these icon themes. Skia's SVG module ignores
/// `<style>` rules; Fluent/Breeze icons colour paths with `fill:currentColor` plus
/// `class="ColorScheme-…"` whose `color` comes from a `<style>` block. Resolve those
/// class colours into `color="…"` attributes before parsing.
pub fn load_svg(bytes: &[u8], fonts: FontMgr) -> Option<svg::Dom> {
    let text = String::from_utf8_lossy(bytes);
    let fixed = resolve_class_colors(&text);
    svg::Dom::from_bytes(fixed.as_bytes(), fonts).ok()
}

pub fn resolve_class_colors(text: &str) -> String {
    let mut rules: Vec<(String, String)> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<style") {
        let Some(j) = rest[i..].find("</style>") else { break };
        let css = &rest[i..i + j];
        let mut c = css;
        while let Some(dot) = c.find('.') {
            let after = &c[dot + 1..];
            let Some(open) = after.find('{') else { break };
            let Some(close) = after.find('}') else { break };
            let name = after[..open].trim().to_string();
            let body = &after[open + 1..close];
            if let Some(cp) = body.find("color") {
                let v = body[cp + 5..].trim_start_matches([' ', ':']).split(';').next().unwrap_or("").trim().to_string();
                if !name.is_empty() && !v.is_empty() && !name.contains(char::is_whitespace) {
                    rules.push((name, v));
                }
            }
            c = &after[close + 1..];
        }
        rest = &rest[i + j..];
    }
    if rules.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for (name, color) in rules {
        let needle = format!("class=\"{name}\"");
        out = out.replace(&needle, &format!("{needle} color=\"{color}\""));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn class_colors() {
        let s = r#"<svg><style>.ColorScheme-NegativeText{ color:#e74c3c; }</style><rect class="ColorScheme-NegativeText" style="fill:currentColor"/></svg>"#;
        assert!(super::resolve_class_colors(s).contains(r##"class="ColorScheme-NegativeText" color="#e74c3c""##));
    }
}
