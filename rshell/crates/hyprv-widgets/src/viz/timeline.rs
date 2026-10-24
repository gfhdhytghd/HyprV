//! V6 TimelineGrid — `WeekCalendarPanel.qml` body below the toolbar (ui-panels-calendar.md
//! §2.3–2.5): day headers, all-day lanes, hour/day grid with lane-packed events, now line,
//! the (content-attached) scroll indicator and the scroll-synced time column.
//!
//! Dates are converted by the caller into **buffer-relative** numbers (buffer start =
//! `weekStart − 7 days`, 21 days): all-day events as day indices, timed events as minutes
//! since buffer-start midnight. `layout_all_day` / `layout_timed` reproduce
//! `rebuildLayouts()` exactly on those numbers.
use crate::controls::{
    kit::{self, Font, Tok},
    Control, PointerEvent, Response,
};
use crate::foundations::{rgb, with_alpha};
use hyprv_ui::{animation::Color, geometry::Rect, scene::Node};
use std::time::Duration;

pub const BUFFER_DAYS: usize = 21;
pub const HOUR_HEIGHT: f32 = 44.;
const TOOLBAR_H: f32 = 58.;
const DAY_HEADER_H: f32 = 48.;
const TIME_COL_W: f32 = 58.;

#[derive(Clone, Debug)]
pub struct AllDayInput {
    /// Day index of the (UTC-date) start relative to the buffer start (may be < 0).
    pub start_day: i32,
    /// Exclusive end day index (UTC date of `end`).
    pub end_day: i32,
    pub title: String,
    /// `None` = accent.
    pub color: Option<Color>,
}

#[derive(Clone, Debug)]
pub struct TimedInput {
    /// Minutes since buffer-start local midnight.
    pub start_min: f64,
    pub end_min: f64,
    pub title: String,
    /// `Qt.formatTime(start, "HH:mm")`.
    pub start_label: String,
    pub color: Option<Color>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllDayItem {
    pub input: usize,
    pub start_day: usize,
    pub span: usize,
    pub lane: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedItem {
    pub input: usize,
    pub day: usize,
    pub start_min: f64,
    pub end_min: f64,
    pub lane: usize,
    pub lanes: usize,
}

/// All-day layout (`WeekCalendarPanel.qml:152-165, 192-202`). Returns items + lane count.
pub fn layout_all_day(inputs: &[AllDayInput]) -> (Vec<AllDayItem>, usize) {
    let n = BUFFER_DAYS as i32;
    let mut items: Vec<AllDayItem> = inputs
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let start = e.start_day.max(0);
            let end = n.min((start + 1).max(e.end_day));
            (start < n && end > 0).then(|| AllDayItem {
                input: i,
                start_day: start as usize,
                span: (end - start).max(1) as usize,
                lane: 0,
            })
        })
        .collect();
    // JS Array.sort is stable: startDay asc, span desc.
    items.sort_by(|a, b| a.start_day.cmp(&b.start_day).then(b.span.cmp(&a.span)));
    let mut ends: Vec<usize> = Vec::new();
    for it in &mut items {
        // QML: first lane whose end is not > startDay, scanning from 0.
        let mut lane = 0;
        while lane < ends.len() && ends[lane] > it.start_day {
            lane += 1;
        }
        if lane == ends.len() {
            ends.push(0);
        }
        it.lane = lane;
        ends[lane] = it.start_day + it.span;
    }
    (items, ends.len())
}

/// Timed layout (`WeekCalendarPanel.qml:168-190, 204-219`): split per day, ≥ 15 min, lanes.
pub fn layout_timed(inputs: &[TimedInput]) -> Vec<TimedItem> {
    let mut out = Vec::new();
    for (i, e) in inputs.iter().enumerate() {
        for day in 0..BUFFER_DAYS {
            let ds = day as f64 * 1440.;
            let de = ds + 1440.;
            if e.end_min <= ds || e.start_min >= de {
                continue;
            }
            let s = e.start_min.max(ds) - ds;
            let en = (e.end_min.min(de) - ds).max(s + 15.);
            out.push(TimedItem {
                input: i,
                day,
                start_min: s,
                end_min: en,
                lane: 0,
                lanes: 1,
            });
        }
    }
    for day in 0..BUFFER_DAYS {
        let mut idx: Vec<usize> = (0..out.len()).filter(|&k| out[k].day == day).collect();
        idx.sort_by(|&a, &b| {
            out[a]
                .start_min
                .total_cmp(&out[b].start_min)
                .then(out[a].end_min.total_cmp(&out[b].end_min))
        });
        let mut ends: Vec<f64> = Vec::new();
        for &k in &idx {
            let mut lane = 0;
            while lane < ends.len() && ends[lane] > out[k].start_min {
                lane += 1;
            }
            if lane == ends.len() {
                ends.push(0.);
            }
            out[k].lane = lane;
            ends[lane] = out[k].end_min;
        }
        let lanes = ends.len().max(1);
        for &k in &idx {
            out[k].lanes = lanes;
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct DayLabel {
    /// `["一","二","三","四","五","六","日"][index % 7]`.
    pub weekday: String,
    pub date: u32,
    pub today: bool,
}

#[derive(Clone, Debug)]
pub struct TimelineGrid {
    /// The whole panel rect (520×500 in the bar); the toolbar row (58 px) is not drawn here.
    pub frame: Rect,
    pub days: Vec<DayLabel>,
    pub all_day: Vec<AllDayInput>,
    pub timed: Vec<TimedInput>,
    pub content_x: f32,
    pub content_y: f32,
    /// (buffer day index, minutes since that midnight incl. seconds/60).
    pub now: Option<(i32, f64)>,
    /// `timeline.moving` (scroll indicator opacity 0.65 vs 0.25).
    pub moving: bool,
}

impl TimelineGrid {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            frame: Rect::new(0., 0., width, height),
            days: Vec::new(),
            all_day: Vec::new(),
            timed: Vec::new(),
            content_x: 0.,
            content_y: 0.,
            now: None,
            moving: false,
        }
    }
    pub fn day_width(&self) -> f32 {
        (self.frame.width - TIME_COL_W - 12.) / 7.
    }
    pub fn all_day_height(&self) -> f32 {
        let (_, lanes) = layout_all_day(&self.all_day);
        (12. + lanes as f32 * 24.).clamp(34., 106.)
    }
    /// Timeline viewport (y, height).
    pub fn timeline_rect(&self) -> Rect {
        let y = TOOLBAR_H + DAY_HEADER_H + self.all_day_height();
        Rect::new(
            TIME_COL_W,
            y,
            self.frame.width - TIME_COL_W - 12.,
            self.frame.height - y,
        )
    }
    pub fn content_size(&self) -> (f32, f32) {
        (BUFFER_DAYS as f32 * self.day_width(), 24. * HOUR_HEIGHT)
    }
    /// `contentX` that shows the week of `weekStart` (initializeHorizontalScroll).
    pub fn initial_content_x(&self) -> f32 {
        7. * self.day_width()
    }
    /// `positionNearNow()`.
    pub fn content_y_near(&self, hour: u32) -> f32 {
        let (_, ch) = self.content_size();
        let vh = self.timeline_rect().height;
        ((hour as f32 - 2.) * HOUR_HEIGHT).min(ch - vh).max(0.)
    }
}

fn qt_text(s: &str, x: f32, y: f32, w: Option<f32>, font: &Font, color: Color) -> Node {
    // WeekCalendarPanel texts set no renderType → QtRendering.
    kit::qt_rendering(kit::text(s, x, y, w, font, color))
}

impl Control for TimelineGrid {
    type Action = ();

    fn implicit_size(&self, _: &Tok) -> (f32, f32) {
        (self.frame.width, self.frame.height)
    }

    fn frame(&self) -> Rect {
        self.frame
    }

    fn build(&self, tok: &Tok, _: Duration) -> Node {
        let f = self.frame;
        let d = tok.dark();
        let text = tok.pal.primary_text;
        let muted = with_alpha(text, if d { 0.62 } else { 0.58 });
        let accent = tok.pal.accent;
        let line = with_alpha(text, if d { 0.11 } else { 0.09 });
        let white = rgb("#ffffff");
        let dw = self.day_width();
        let vw = f.width - TIME_COL_W - 12.;
        let mut root = kit::group(f);

        // Toolbar divider (y = toolbarHeight − 1).
        root.add(kit::qt_rect(Rect::new(0., TOOLBAR_H - 1., f.width, 1.), 0., line, None));

        // Day headers.
        let mut hdr = crate::primitives::clip_box(Rect::new(TIME_COL_W, TOOLBAR_H, vw, DAY_HEADER_H), vec![]);
        let wd_font = Font {
            px: 11.,
            weight: kit::DEMIBOLD,
        };
        let num_font = Font {
            px: 14.,
            weight: kit::BOLD,
        };
        for (i, day) in self.days.iter().enumerate().take(BUFFER_DAYS) {
            let cx = i as f32 * dw - self.content_x;
            let wm = kit::measure(&day.weekday, 11., kit::DEMIBOLD);
            let col_w = wm.width.max(28.);
            let col_h = wm.height + 1. + 24.;
            let (ox, oy) = kit::center_in(dw, DAY_HEADER_H, col_w, col_h);
            let (x0, y0) = (cx + ox, oy);
            hdr.add(qt_text(
                &day.weekday,
                x0 + kit::hc(col_w) - kit::hc(wm.width),
                y0,
                None,
                &wd_font,
                muted,
            ));
            let px = x0 + kit::hc(col_w) - kit::hc(28.);
            let py = y0 + wm.height + 1.;
            if day.today {
                hdr.add(kit::qt_rect(Rect::new(px, py, 28., 24.), 12., accent, None));
            }
            let label = day.date.to_string();
            let nm = kit::measure(&label, 14., kit::BOLD);
            let (nx, ny) = kit::center_in(28., 24., nm.width, nm.height);
            hdr.add(qt_text(
                &label,
                px + nx,
                py + ny,
                None,
                &num_font,
                if day.today { white } else { text },
            ));
        }
        root.add(hdr);

        // All-day area.
        let (items, _) = layout_all_day(&self.all_day);
        let adh = self.all_day_height();
        let ad_y = TOOLBAR_H + DAY_HEADER_H;
        let mut ad = crate::primitives::clip_box(Rect::new(TIME_COL_W, ad_y, vw, adh), vec![]);
        let chip_font = Font {
            px: 10.,
            weight: kit::DEMIBOLD,
        };
        let exact10 = kit::line_height_exact(crate::foundations::BASE_FONT, 10., kit::DEMIBOLD);
        for it in &items {
            let e = &self.all_day[it.input];
            let x = it.start_day as f32 * dw + 3. - self.content_x;
            let y = 6. + it.lane as f32 * 24.;
            let w = it.span as f32 * dw - 6.;
            let col = e.color.unwrap_or(accent);
            let mut chip = kit::group(Rect::new(x, y, w, 20.));
            chip.add(kit::qt_rect(
                Rect::new(0., 0., w, 20.),
                6.,
                with_alpha(col, 0.80),
                Some((1., with_alpha(white, 0.22))),
            ));
            let title = if e.title.is_empty() { "(无标题)" } else { &e.title };
            chip.add(qt_text(
                title,
                7.,
                (20. - exact10) / 2.,
                Some(w - 12.),
                &chip_font,
                white,
            ));
            ad.add(chip);
        }
        root.add(ad);
        let all_m = kit::measure("全天", 10., kit::NORMAL);
        root.add(qt_text(
            "全天",
            8. + 44. - all_m.advance,
            ad_y + 8.,
            None,
            &Font {
                px: 10.,
                weight: kit::NORMAL,
            },
            muted,
        ));
        root.add(kit::qt_rect(
            Rect::new(0., ad_y + adh - 1., f.width, 1.),
            0.,
            line,
            None,
        ));

        // Timeline.
        let tl = self.timeline_rect();
        let (cw, chh) = self.content_size();
        let mut view = crate::primitives::clip_box(tl, vec![]);
        let mut content = kit::group(Rect::new(-self.content_x, -self.content_y, cw, chh));
        for i in 0..=24 {
            content.add(kit::qt_rect(
                Rect::new(0., i as f32 * HOUR_HEIGHT, cw, 1.),
                0.,
                line,
                None,
            ));
        }
        for i in 0..=BUFFER_DAYS {
            content.add(kit::qt_rect(Rect::new(i as f32 * dw, 0., 1., chh), 0., line, None));
        }
        let title_font = Font {
            px: 10.,
            weight: kit::BOLD,
        };
        let time_font = Font {
            px: 9.,
            weight: kit::NORMAL,
        };
        for it in layout_timed(&self.timed) {
            let e = &self.timed[it.input];
            let lane_w = (dw - 6.) / it.lanes as f32;
            let x = it.day as f32 * dw + 3. + it.lane as f32 * lane_w;
            let y = (it.start_min / 60.) as f32 * HOUR_HEIGHT + 1.;
            let w = (lane_w - 2.).max(18.);
            let h = (((it.end_min - it.start_min) / 60.) as f32 * HOUR_HEIGHT - 2.).max(18.);
            let col = e.color.unwrap_or(accent);
            let mut blk = crate::primitives::clip_box(Rect::new(x, y, w, h), vec![]);
            blk.add(kit::qt_rect(
                Rect::new(0., 0., w, h),
                7.,
                with_alpha(col, 0.84),
                Some((1., with_alpha(white, 0.24))),
            ));
            let title = if e.title.is_empty() { "(无标题)" } else { &e.title };
            blk.add(qt_text(title, 5., 5., Some(w - 10.), &title_font, white));
            if h >= 34. {
                let th = kit::measure(title, 10., kit::BOLD).height;
                let t = qt_text(
                    &e.start_label,
                    5.,
                    5. + th + 1.,
                    Some(w - 10.),
                    &time_font,
                    rgb("#f7f7fb"),
                );
                blk.add(kit::fade(t, 0.84));
            }
            content.add(blk);
        }
        if let Some((day, minutes)) = self.now {
            if (0..BUFFER_DAYS as i32).contains(&day) {
                let y = (minutes / 60.) as f32 * HOUR_HEIGHT;
                let mut nl = kit::group(Rect::new(day as f32 * dw, y, dw, 2.));
                nl.add(kit::qt_rect(Rect::new(0., 0., dw, 2.), 0., accent, None));
                nl.add(kit::qt_rect(
                    Rect::new(0., kit::vcenter(2., 7.), 7., 7.),
                    4.,
                    accent,
                    None,
                ));
                content.add(nl);
            }
        }
        // Scroll indicator: reparented into the Flickable's contentItem (content coordinates).
        let y_pos = self.content_y / chh;
        let ratio = tl.height / chh;
        let ind = kit::qt_rect(
            Rect::new(cw - 4. - 3., y_pos * tl.height, 3., (ratio * tl.height).max(28.)),
            2.,
            muted,
            None,
        );
        content.add(kit::fade(ind, if self.moving { 0.65 } else { 0.25 }));
        view.add(content);
        root.add(view);

        // Time column.
        let mut tc = crate::primitives::clip_box(Rect::new(0., tl.y, TIME_COL_W, tl.height), vec![]);
        for i in 0..24 {
            let label = format!("{i:02}:00");
            let m = kit::measure(&label, 9., kit::NORMAL);
            tc.add(qt_text(
                &label,
                7. + 44. - m.advance,
                i as f32 * HOUR_HEIGHT - 7. - self.content_y,
                None,
                &time_font,
                muted,
            ));
        }
        root.add(tc);
        root
    }

    fn pointer(&mut self, _: PointerEvent, _: Duration) -> Response<()> {
        // Scrolling/paging is owned by the calendar composite (P10 Flickable + rebase rules,
        // ui-panels-calendar.md §2.4); it sets content_x / content_y.
        Response::default()
    }
}
