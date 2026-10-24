//! Settled host fixtures: mount the same pure component builders as hyprv-shell.
use super::*;
use std::rc::Rc;

pub fn mount(
    fixture: &Value,
    name: &str,
    m: &mut Model,
    workspace: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    match name {
        "calendar-host" => {
            use hyprv_ui_panels::{calendar::CalendarPanel, model::Model as PanelModel};
            let mut pm = PanelModel::default();
            pm.theme.dark = m.theme.dark;
            pm.output.scale = m.output.scale;
            pm.wall = chrono::NaiveDateTime::parse_from_str(
                str_value(&fixture["now"]),
                "%Y-%m-%dT%H:%M:%S",
            )?;
            pm.calendar.bridge = true;
            // Opening requests a week while the fixture has no calendar socket.
            pm.calendar.last_error = "无法连接日历服务".into();
            let (w, h) = CalendarPanel::host_size(m.output.width, 820.);
            m.calendar_open = true;
            m.calendar_presented = true;
            m.calendar_progress = 1.;
            m.calendar_width = w;
            m.calendar_height = h;
            m.calendar_radius = 30.;
            let l = layout::compute(m, &m.theme);
            let panel = std::cell::RefCell::new(CalendarPanel::new(w, h));
            m.slots.calendar = Some(Rc::new(move || {
                panel.borrow_mut().build(&pm, (l.center.x, l.center.y))
            }));
        }
        "island-expanded-host" => {
            use hyprv_ui_island::{Inputs, Island, Media, Pointer};
            let text = fixture["processes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["match"] == "media-status.sh")
                .and_then(|p| p["stdout"].as_str())
                .unwrap_or("");
            let field = |key: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(&format!("{key}=")))
                    .unwrap_or("")
            };
            let art_path = field("art_url")
                .strip_prefix("file://@ASSETS@/")
                .ok_or("host art URL")?;
            let art = skia::Image::from_encoded(skia::Data::new_copy(&std::fs::read(
                workspace.join("parity/fixtures/assets").join(art_path),
            )?));
            let spectrum = fixture["processes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["match"] == "audio-spectrum.sh")
                .and_then(|p| p["lines"].as_array()?.last()?["data"].as_str())
                .unwrap_or("")
                .split(';')
                .filter_map(|s| s.parse().ok())
                .collect();
            let mut island = Island::new(
                Inputs {
                    dark: m.theme.dark,
                    now_hh_mm: "09:41".into(),
                    art,
                    spectrum,
                    media: Media {
                        available: field("available") == "true",
                        playing: field("playing") == "true",
                        title: field("title").into(),
                        artist: field("artist").into(),
                        player: field("player").into(),
                        art_url: field("art_url").into(),
                        position_s: field("position").parse()?,
                        length_s: field("length").parse()?,
                    },
                    ..Default::default()
                },
                m.output.width,
                Duration::ZERO,
            );
            island.clip_hover_to_input_region = false;
            island.frame(Duration::from_millis(2516));
            island.pointer(
                Pointer::Motion {
                    x: m.output.width / 2. - 40.,
                    y: 29.,
                },
                Duration::from_millis(2516),
            );
            for frame in 1..=36 {
                island.frame(Duration::from_millis(2516 + frame * 1000 / 60));
            }
            let g = island.geometry();
            m.island = IslandGeometry {
                width: g.width,
                height: g.height,
                expanded: true,
                attached_center_offset: 0.,
            };
            m.slots.island = Some(Rc::new(move || {
                let mut node = island.build();
                node.frame.x = 0.;
                node.frame.y = 0.;
                node
            }));
        }
        _ => return Ok(false),
    }
    Ok(true)
}
