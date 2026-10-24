//! WeekCalendarPanel (ui-panels-calendar.md §2), embedded in the bar window's centre
//! section (the bar crate owns the morph/host). Text uses Qt's default distance-field
//! rendering except the header buttons (NativeRendering), exactly like the QML.
use crate::{
    model::*,
    qtext::{Align, Font, TextBox, BOLD, DEMIBOLD, NORMAL},
    ui::*,
};
use chrono::{Datelike, Duration as CDuration, NaiveDate, NaiveDateTime, Timelike};
use hyprv_ui::{
    animation::{Color, Easing},
    geometry::Rect,
    scene::{Node, NodeId, Scene},
    skia::Matrix,
};
use std::time::Duration;
use hyprv_widgets::flickable::{Flickable, Settings, Wheel, ScrollPhase};

/// The scroll indicator is declared inside the Flickable, so it is reparented into the
/// content item: it scrolls with the content and is only visible at the far right.
pub const QUIRK_SCROLL_INDICATOR_IN_CONTENT: bool = true;
/// `onWeekStartChanged` re-runs `positionNearNow()`, so paging a week also jumps the
/// vertical scroll back to "now − 2 h".
pub const QUIRK_WEEK_CHANGE_RESETS_VERTICAL: bool = true;

pub const HOUR: f32 = 44.;
pub const DAYS: usize = 21;
pub const LEADING: i64 = 7;
pub const TOOLBAR_H: f32 = 58.;
pub const DAY_HEADER_H: f32 = 48.;
pub const TIME_COL_W: f32 = 58.;
const PRESS_DELAY: Duration = Duration::from_millis(80);

#[derive(Clone, Debug, PartialEq)]
pub enum Z {
    Panel,
    Create,
    Prev,
    Range,
    Next,
    Expand,
    DayHeader,
    AllDayArea,
    Timeline,
    AllDayEvent(usize),
    TimedEvent(usize),
}

#[derive(Clone, Debug)]
struct TimedItem {
    event: usize,
    day: i64,
    start_minute: f64,
    end_minute: f64,
    lane: usize,
    lanes: usize,
}
#[derive(Clone, Debug)]
struct AllDayItem {
    event: usize,
    start_day: i64,
    span: i64,
    lane: usize,
}

/// JS `new Date(iso)` → local wall time. Date-only strings are UTC midnight (ES spec);
/// strings without an offset are local; with `Z`/offset → converted to local.
pub fn js_date_local(s: &str) -> Option<NaiveDateTime> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let utc = d.and_hms_opt(0, 0, 0)?.and_utc();
        return Some(utc.with_timezone(&chrono::Local).naive_local());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Local).naive_local());
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, f) {
            return Some(dt);
        }
    }
    None
}
/// `allDayDate(value)`: the UTC calendar date of the instant.
pub fn all_day_date(s: &str) -> Option<NaiveDate> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_utc().date());
    }
    let local = js_date_local(s)?;
    local.and_local_timezone(chrono::Local).single().map(|d| d.naive_utc().date())
}

fn parse_color(s: &str) -> Option<Color> {
    hyprv_widgets::foundations::parse_color(s)
}

pub struct CalendarPanel {
    pub width: f32,
    pub height: f32,
    content_x: f32,
    content_y: f32,
    h_init: bool,
    observed_x: f64,
    /// Week start applied locally at a rebase until the bridge publishes it.
    pending_week: Option<NaiveDate>,
    last_week: Option<NaiveDate>,
    position_near_now: bool,
    pub inter: Interaction<Z>,
    zones: Vec<Zone<Z>>,
    hovered_event: Option<(usize, Rect)>,
    pointer_inside: bool,
    btn_color: [Anim<Color>; 5],
    btn_scale: [Anim<f64>; 5],
    scroll_x: Flickable,
    scroll_y: Flickable,
    delayed_press: Option<(Pointer, Duration)>,
    wheel_active: [Option<Duration>; 3],
    root: Option<NodeId>,
    key: String,
}

impl CalendarPanel {
    pub fn new(width: f32, height: f32) -> Self {
        let lin = || Anim::new(Color::default(), 120, Easing::Linear);
        let sc = || Anim::new(1., 90, Easing::Linear);
        Self {
            width,
            height,
            content_x: 0.,
            content_y: 0.,
            h_init: false,
            observed_x: 0.,
            pending_week: None,
            last_week: None,
            position_near_now: true,
            inter: Interaction::default(),
            zones: Vec::new(),
            hovered_event: None,
            pointer_inside: false,
            btn_color: [lin(), lin(), lin(), lin(), lin()],
            btn_scale: [sc(), sc(), sc(), sc(), sc()],
            scroll_x: Flickable::new(Settings { flick_deceleration: 2600., ..Settings::list_view() }),
            scroll_y: Flickable::new(Settings { flick_deceleration: 2600., ..Settings::list_view() }),
            delayed_press: None,
            wheel_active: [None; 3],
            root: None,
            key: String::new(),
        }
    }

    /// Host size: `min(520, contentRoot.width − 48)` × `min(500, barWindow.height − 78)`.
    pub fn host_size(content_width: f32, bar_height: f32) -> (f32, f32) {
        (520f32.min(content_width - 48.), 500f32.min(bar_height - 78.))
    }

    pub fn day_width(&self) -> f32 {
        (self.width - TIME_COL_W - 12.) / 7.
    }
    fn view_w(&self) -> f32 {
        self.width - TIME_COL_W - 12.
    }
    fn content_w(&self) -> f32 {
        DAYS as f32 * self.day_width()
    }
    fn content_h(&self) -> f32 {
        24. * HOUR
    }

    pub fn week_start(&self, model: &Model) -> NaiveDate {
        if let Some(p) = self.pending_week {
            return p;
        }
        NaiveDate::parse_from_str(&model.calendar.week_start, "%Y-%m-%d").unwrap_or(model.wall.date())
    }
    fn buffer_start(&self, model: &Model) -> NaiveDate {
        self.week_start(model) - CDuration::days(LEADING)
    }

    fn lane_count_and_layout(&self, model: &Model) -> (Vec<AllDayItem>, Vec<TimedItem>, usize) {
        let cal = &model.calendar;
        let week = self.buffer_start(model);
        let find = |id: &str| cal.calendars.iter().find(|c| c.id == id);
        let mut all_day = Vec::new();
        let mut timed = Vec::new();
        for (i, e) in cal.events.iter().enumerate() {
            if let Some(c) = find(&e.calendar_id) {
                if c.hidden || c.holds_tasks {
                    continue;
                }
            }
            if e.all_day {
                let (Some(s), Some(en)) = (all_day_date(&e.start), all_day_date(&e.end)) else { continue };
                let start_idx = (s - week).num_days().max(0);
                let end_idx = (DAYS as i64).min((start_idx + 1).max((en - week).num_days()));
                if start_idx < DAYS as i64 && end_idx > 0 {
                    all_day.push(AllDayItem { event: i, start_day: start_idx, span: (end_idx - start_idx).max(1), lane: 0 });
                }
                continue;
            }
            let (Some(es), Some(ee)) = (js_date_local(&e.start), js_date_local(&e.end)) else { continue };
            for day in 0..DAYS as i64 {
                let ds = (week + CDuration::days(day)).and_hms_opt(0, 0, 0).unwrap();
                let de = (week + CDuration::days(day + 1)).and_hms_opt(0, 0, 0).unwrap();
                if ee <= ds || es >= de {
                    continue;
                }
                let ss = es.max(ds);
                let se = ee.min(de);
                let sm = (ss - ds).num_milliseconds() as f64 / 60000.;
                let em = (sm + 15.).max((se - ds).num_milliseconds() as f64 / 60000.);
                timed.push(TimedItem { event: i, day, start_minute: sm, end_minute: em, lane: 0, lanes: 1 });
            }
        }
        all_day.sort_by(|a, b| a.start_day.cmp(&b.start_day).then(b.span.cmp(&a.span)));
        let mut ends: Vec<i64> = Vec::new();
        for it in all_day.iter_mut() {
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
        for day in 0..DAYS as i64 {
            let mut idx: Vec<usize> = (0..timed.len()).filter(|&k| timed[k].day == day).collect();
            idx.sort_by(|&a, &b| timed[a].start_minute.partial_cmp(&timed[b].start_minute).unwrap().then(timed[a].end_minute.partial_cmp(&timed[b].end_minute).unwrap()));
            let mut lane_ends: Vec<f64> = Vec::new();
            for &k in &idx {
                let mut lane = 0;
                while lane < lane_ends.len() && lane_ends[lane] > timed[k].start_minute {
                    lane += 1;
                }
                if lane == lane_ends.len() {
                    lane_ends.push(0.);
                }
                timed[k].lane = lane;
                lane_ends[lane] = timed[k].end_minute;
            }
            let lanes = lane_ends.len().max(1);
            for &k in &idx {
                timed[k].lanes = lanes;
            }
        }
        (all_day, timed, ends.len())
    }

    fn all_day_height(lanes: usize) -> f32 {
        (12. + lanes as f32 * 24.).clamp(34., 106.)
    }
    fn timeline_rect(&self, lanes: usize) -> Rect {
        let y = TOOLBAR_H + DAY_HEADER_H + Self::all_day_height(lanes);
        Rect::new(TIME_COL_W, y, self.view_w(), self.height - y)
    }

    /// `initializeHorizontalScroll` + `positionNearNow` when the panel becomes visible.
    fn ensure_init(&mut self, model: &Model) {
        let week = self.week_start(model);
        if self.last_week != Some(week) {
            let first = self.last_week.is_none();
            self.last_week = Some(week);
            if !first && QUIRK_WEEK_CHANGE_RESETS_VERTICAL {
                self.position_near_now = true;
            }
        }
        if !self.h_init && self.day_width() > 0. {
            self.content_x = LEADING as f32 * self.day_width();
            self.scroll_x.set_content_y(self.content_x as f64);
            self.observed_x = self.scroll_x.content_y();
            self.h_init = true;
        }
        if self.position_near_now {
            self.position_near_now = false;
            let (_, _, lanes) = self.lane_count_and_layout(model);
            let tl = self.timeline_rect(lanes);
            let d = (model.wall.date() - self.buffer_start(model)).num_days();
            let hour = if (0..DAYS as i64).contains(&d) { model.wall.hour() as f32 } else { 8. };
            self.content_y = ((hour - 2.) * HOUR).min(self.content_h() - tl.height).max(0.);
            self.scroll_y.set_content_y(self.content_y as f64);
        }
        if let Some(p) = self.pending_week {
            if NaiveDate::parse_from_str(&model.calendar.week_start, "%Y-%m-%d").ok() == Some(p) {
                self.pending_week = None;
            }
        }
    }

    /// `rebaseHorizontalScroll` after every contentX change; returns the week action.
    fn rebase(&mut self, model: &Model) -> Option<Action> {
        if !self.h_init || !model.calendar.bridge || self.day_width() <= 0. {
            return None;
        }
        let dw = self.day_width();
        if self.content_x >= 3. * dw && self.content_x <= 11. * dw {
            return None;
        }
        let week = self.week_start(model);
        if self.content_x < 3. * dw {
            self.content_x += 7. * dw;
            self.pending_week = Some(week - CDuration::days(7));
            Some(Action::CalendarPreviousWeek)
        } else {
            self.content_x -= 7. * dw;
            self.pending_week = Some(week + CDuration::days(7));
            Some(Action::CalendarNextWeek)
        }
    }

    fn set_content_x(&mut self, x: f32, model: &Model) -> Option<Action> {
        self.content_x = x.clamp(0., (self.content_w() - self.view_w()).max(0.));
        self.scroll_x.set_content_y(self.content_x as f64);
        self.rebase_scroll(model)
    }

    pub fn range_label(&self, model: &Model) -> String {
        let dw = self.day_width();
        let first = ((self.content_x / dw + 1e-4).floor() as i64).clamp(0, DAYS as i64 - 1);
        let last = (((self.content_x + self.view_w()) / dw - 1e-4).ceil() as i64 - 1).clamp(first, DAYS as i64 - 1);
        let b = self.buffer_start(model);
        let (s, e) = (b + CDuration::days(first), b + CDuration::days(last));
        let ymd = |d: NaiveDate| format!("{}/{}/{}", d.year(), d.month(), d.day());
        if s.year() != e.year() {
            format!("{} – {}", ymd(s), ymd(e))
        } else if s.month() != e.month() {
            format!("{} – {}/{}", ymd(s), e.month(), e.day())
        } else {
            format!("{} – {}", ymd(s), e.day())
        }
    }

    fn tokens(pal: &Palette) -> (Color, Color, Color, Color, Color) {
        let text = pal.primary_text;
        let muted = with_alpha(text, if pal.dark { 0.62 } else { 0.58 });
        let line = with_alpha(text, if pal.dark { 0.11 } else { 0.09 });
        let elevated = with_alpha(rgb("#ffffff"), if pal.dark { 0.055 } else { 0.30 });
        (text, muted, pal.accent, line, elevated)
    }

    fn header_labels(&self, model: &Model) -> [String; 5] {
        let hhmm = format!("{:02}:{:02}", model.wall.hour(), model.wall.minute());
        ["＋ 创建日程".into(), "‹".into(), format!("{}  ·  {}", self.range_label(model), hhmm), "›".into(), "↗".into()]
    }

    fn sync_buttons(&mut self, model: &Model) {
        let pal = Palette::new(model.theme.dark);
        let (_, _, accent, _, elevated) = Self::tokens(&pal);
        let ids = [Z::Create, Z::Prev, Z::Range, Z::Next, Z::Expand];
        for (i, id) in ids.iter().enumerate() {
            let target = if i == 0 { accent } else if self.inter.is_hovered(id) { elevated } else { with_alpha(elevated, 0.) };
            // ColorAnimation interpolates RGBA; "transparent" is #00000000
            let target = if i != 0 && !self.inter.is_hovered(id) { Color::default() } else { target };
            if self.btn_color[i].target() == &Color::default() && !self.btn_color[i].running(model.now) && self.key.is_empty() {
                self.btn_color[i].snap(target);
            }
            self.btn_color[i].set(target, model.now);
            self.btn_scale[i].set(if self.inter.is_pressed(id) { 0.96 } else { 1. }, model.now);
        }
    }

    pub fn animating(&self, now: Duration) -> bool {
        self.btn_color.iter().any(|a| a.running(now)) || self.btn_scale.iter().any(|a| a.running(now)) || self.scroll_x.animating() || self.scroll_y.animating()
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.delayed_press.map(|(_, deadline)| deadline)
    }

    fn replay_press(&mut self) {
        if let Some((event, _)) = self.delayed_press.take() {
            self.inter.feed(event, &self.zones);
        }
    }

    fn sync_scroll(&mut self, model: &Model) {
        let (_, _, lanes) = self.lane_count_and_layout(model);
        let tl = self.timeline_rect(lanes);
        self.scroll_x.set_extents(self.view_w() as f64, self.content_w() as f64);
        self.scroll_y.set_extents(tl.height as f64, self.content_h() as f64);
        self.content_x = self.scroll_x.content_y() as f32;
        self.content_y = self.scroll_y.content_y() as f32;
    }

    fn rebase_scroll(&mut self, model: &Model) -> Option<Action> {
        if self.observed_x == self.scroll_x.content_y() { return None; }
        self.observed_x = self.scroll_x.content_y();
        let action = self.rebase(model);
        if action.is_some() {
            // QML assigns contentX in onContentXChanged: Qt resets that axis's
            // timeline and adjusts its held drag anchor (not the vertical axis).
            self.scroll_x.set_content_y(self.content_x as f64);
            self.observed_x = self.scroll_x.content_y();
        }
        action
    }

    /// Advance only from the host's presentation clock; no idle polling.
    pub fn tick(&mut self, model: &Model) -> Vec<Action> {
        self.sync_scroll(model);
        self.scroll_x.frame(model.now);
        self.scroll_y.frame(model.now);
        self.content_x = self.scroll_x.content_y() as f32;
        self.content_y = self.scroll_y.content_y() as f32;
        let action = self.rebase_scroll(model);
        if self.delayed_press.is_some_and(|(_, deadline)| model.now >= deadline) {
            self.replay_press();
        }
        action.into_iter().collect()
    }

    // ------------------------------------------------------------------ build

    /// The 520×500 panel at `abs` (absolute logical origin on the bar surface).
    pub fn build(&mut self, model: &Model, abs: (f32, f32)) -> Node {
        let mut n = self.build_raw(model, abs);
        crate::qrect::convert(&mut n, abs, model.output.scale);
        n
    }
    fn build_raw(&mut self, model: &Model, abs: (f32, f32)) -> Node {
        self.ensure_init(model);
        self.sync_scroll(model);
        self.sync_buttons(model);
        let pal = Palette::new(model.theme.dark);
        let (text_c, muted, accent, line, elevated) = Self::tokens(&pal);
        let dpr = model.output.scale;
        let (w, h) = (self.width, self.height);
        let dw = self.day_width();
        let vw = self.view_w();
        let cal = &model.calendar;
        let (all_day, timed, lanes) = self.lane_count_and_layout(model);
        let adh = Self::all_day_height(lanes);
        let buffer = self.buffer_start(model);
        let today = model.wall.date();
        let mut zones = vec![Zone { id: Z::Panel, rect: Rect::new(abs.0, abs.1, w, h), cursor: Cursor::Default, enabled: true }];
        let mut root = group(Rect::new(0., 0., w, h));
        root.clip = Some([0.; 4]);
        let qt = |s: &str, px: f32, weight: i32, color: Color| TextBox::new(s, Font::qt(px, weight), color);
        let ev_color = |e: &CalendarEvent| cal.calendars.iter().find(|c| c.id == e.calendar_id).and_then(|c| c.color.as_deref()).and_then(parse_color).unwrap_or(accent);

        // 1. toolbar
        let labels = self.header_labels(model);
        let ids = [Z::Create, Z::Prev, Z::Range, Z::Next, Z::Expand];
        let widths: Vec<f32> = labels.iter().map(|l| 36f32.max(crate::qtext::measure(l, &Font::native(13., DEMIBOLD)).width + 22.)).collect();
        let nav_w = widths[1] + widths[2] + widths[3] + 14.;
        let mid_w = w - 28. - 178.;
        let spacer = ((mid_w - nav_w) / 2.).max(0.);
        let nav_x = 14. + 102. + if spacer > 0. { spacer + 7. } else { 0. };
        let xs = [14., nav_x, nav_x + widths[1] + 7., nav_x + widths[1] + 7. + widths[2] + 7., 14. + 102. + mid_w + 76. - widths[4]];
        for i in 0..5 {
            let f = Rect::new(xs[i], center(TOOLBAR_H, 34.), widths[i], 34.);
            let emph = i == 0;
            let color = self.btn_color[i].get(model.now);
            let mut b = rect(f, 17., color, if emph { None } else { Some((1., line)) });
            let s = self.btn_scale[i].get(model.now) as f32;
            if s != 1. {
                let (cx, cy) = (f.width / 2., f.height / 2.);
                let mut m = Matrix::translate((cx, cy));
                m.pre_scale((s, s), None);
                m.pre_translate((-cx, -cy));
                b.transform = m;
            }
            let lt = TextBox::new(labels[i].clone(), Font::native(13., DEMIBOLD), if emph { rgb("#ffffff") } else { text_c });
            b.children.push(text_center(&lt, f.width, f.height, at(abs, f), dpr));
            root.children.push(b);
            zones.push(Zone { id: ids[i].clone(), rect: abs_rect(abs, f), cursor: Cursor::Pointer, enabled: true });
        }
        root.children.push(rect(Rect::new(0., TOOLBAR_H - 1., w, 1.), 0., line, None));

        // 3. day headers
        {
            let vf = Rect::new(TIME_COL_W, TOOLBAR_H, vw, DAY_HEADER_H);
            let va = at(abs, vf);
            let mut items = Vec::new();
            for i in 0..DAYS {
                let cx = i as f32 * dw - self.content_x;
                if cx + dw < 0. || cx > vw {
                    continue;
                }
                let day = buffer + CDuration::days(i as i64);
                let cell = Rect::new(cx, 0., dw, DAY_HEADER_H);
                let ca = at(va, cell);
                let wd = qt(["一", "二", "三", "四", "五", "六", "日"][i % 7], 11., DEMIBOLD, muted);
                let (ww, wh) = text_size(&wd);
                let col_w = ww.max(28.);
                let col_h = wh + 1. + 24.;
                let colf = Rect::new(center(dw, col_w), center(DAY_HEADER_H, col_h), col_w, col_h);
                let coa = at(ca, colf);
                let mut col = group(colf);
                col.children.push(text(&wd, Rect::new(center(col_w, ww), 0., ww, wh), coa, dpr));
                let is_today = day == today;
                let pf = Rect::new(center(col_w, 28.), wh + 1., 28., 24.);
                let mut pill = rect(pf, 12., if is_today { accent } else { Color::default() }, None);
                let num = qt(&day.day().to_string(), 14., BOLD, if is_today { rgb("#ffffff") } else { text_c });
                pill.children.push(text_center(&num, 28., 24., at(coa, pf), dpr));
                col.children.push(pill);
                items.push(group_with(cell, vec![col]));
            }
            root.children.push(clip(vf, items));
            zones.push(Zone { id: Z::DayHeader, rect: abs_rect(abs, vf), cursor: Cursor::Default, enabled: true });
        }

        // 4. all-day area
        let af = Rect::new(TIME_COL_W, TOOLBAR_H + DAY_HEADER_H, vw, adh);
        let aa = at(abs, af);
        {
            zones.push(Zone { id: Z::AllDayArea, rect: abs_rect(abs, af), cursor: Cursor::Default, enabled: true });
            let mut items = Vec::new();
            for (k, it) in all_day.iter().enumerate() {
                let e = &cal.events[it.event];
                let f = Rect::new(it.start_day as f32 * dw + 3. - self.content_x, 6. + it.lane as f32 * 24., it.span as f32 * dw - 6., 20.);
                let mut chip = rect(f, 6., with_alpha(ev_color(e), 0.80), Some((1., with_alpha(rgb("#ffffff"), 0.22))));
                let tw = (f.width - 12.).max(0.);
                let t = qt(if e.summary.is_empty() { "(无标题)" } else { &e.summary }, 10., DEMIBOLD, rgb("#ffffff")).width(tw).elide();
                let th = text_size(&t).1;
                // anchors.fill + AlignVCenter: the line box sits centred in the 20 px chip
                chip.children.push(text(&t, Rect::new(7., (20. - th) / 2., tw, th), at(aa, f), dpr));
                items.push(chip);
                let z = clip_rect(abs_rect(aa, f), abs_rect(abs, af));
                zones.push(Zone { id: Z::AllDayEvent(k), rect: z, cursor: Cursor::Pointer, enabled: true });
            }
            root.children.push(clip(af, items));
            let lbl = qt("全天", 10., NORMAL, muted).width(TIME_COL_W - 14.).align(Align::Right);
            let (lw, lh) = (TIME_COL_W - 14., text_size(&lbl).1);
            root.children.push(text(&lbl, Rect::new(8., af.y + 8., lw, lh), abs, dpr));
            root.children.push(rect(Rect::new(0., af.y + adh - 1., w, 1.), 0., line, None));
        }

        // 5. timeline
        let tl = self.timeline_rect(lanes);
        let ta = at(abs, tl);
        {
            zones.push(Zone { id: Z::Timeline, rect: abs_rect(abs, tl), cursor: Cursor::Default, enabled: true });
            let content = Rect::new(-self.content_x, -self.content_y, self.content_w(), self.content_h());
            let ca = at(ta, content);
            let mut c = group(content);
            for i in 0..25 {
                c.children.push(rect(Rect::new(0., i as f32 * HOUR, self.content_w(), 1.), 0., line, None));
            }
            for i in 0..=DAYS {
                c.children.push(rect(Rect::new(i as f32 * dw, 0., 1., self.content_h()), 0., line, None));
            }
            for (k, it) in timed.iter().enumerate() {
                let e = &cal.events[it.event];
                let lane_w = (dw - 6.) / it.lanes as f32;
                let f = Rect::new(it.day as f32 * dw + 3. + it.lane as f32 * lane_w, it.start_minute as f32 / 60. * HOUR + 1., 18f32.max(lane_w - 2.), 18f32.max(((it.end_minute - it.start_minute) / 60.) as f32 * HOUR - 2.));
                let fa = at(ca, f);
                let mut card = rect(f, 7., with_alpha(ev_color(e), 0.84), Some((1., with_alpha(rgb("#ffffff"), 0.24))));
                card.clip = Some([0.; 4]);
                let iw = f.width - 10.;
                let st = qt(if e.summary.is_empty() { "(无标题)" } else { &e.summary }, 10., BOLD, rgb("#ffffff")).width(iw).elide();
                let sh = text_size(&st).1;
                card.children.push(text(&st, Rect::new(5., 5., iw, sh), fa, dpr));
                if f.height >= 34. {
                    if let Some(s) = js_date_local(&e.start) {
                        let tt = qt(&format!("{:02}:{:02}", s.hour(), s.minute()), 9., NORMAL, rgb("#f7f7fb")).width(iw);
                        let th = text_size(&tt).1;
                        card.children.push(faded(text(&tt, Rect::new(5., 5. + sh + 1., iw, th), fa, dpr), 0.84));
                    }
                }
                c.children.push(card);
                let z = clip_rect(abs_rect(ca, f), abs_rect(abs, tl));
                zones.push(Zone { id: Z::TimedEvent(k), rect: z, cursor: Cursor::Pointer, enabled: true });
            }
            // now line
            let d = (today - buffer).num_days();
            if cal.bridge && (0..DAYS as i64).contains(&d) {
                let y = (model.wall.hour() as f32 * 60. + model.wall.minute() as f32 + model.wall.second() as f32 / 60.) / 60. * HOUR;
                let mut nl = rect(Rect::new(d as f32 * dw, y, dw, 2.), 0., accent, None);
                nl.children.push(rect(Rect::new(0., center(2., 7.), 7., 7.), 4., accent, None));
                c.children.push(nl);
            }
            if QUIRK_SCROLL_INDICATOR_IN_CONTENT {
                let vis_ratio = tl.height / self.content_h();
                let ypos = self.content_y / self.content_h();
                let ih = 28f32.max(vis_ratio * tl.height);
                let moving = self.scroll_x.moving() || self.scroll_y.moving();
                let mut ind = rect(Rect::new(self.content_w() - 4. - 3., ypos * tl.height, 3., ih), 2., muted, None);
                ind.opacity = if moving { 0.65 } else { 0.25 };
                c.children.push(ind);
            }
            root.children.push(clip(tl, vec![c]));
        }

        // 6. time column
        {
            let cf = Rect::new(0., tl.y, TIME_COL_W, tl.height);
            let ca = at(abs, cf);
            let inner = Rect::new(0., -self.content_y, TIME_COL_W, self.content_h());
            let ia = at(ca, inner);
            let mut g = group(inner);
            for i in 0..24 {
                let y = i as f32 * HOUR - 7.;
                if y - self.content_y > tl.height || y + 14. - self.content_y < 0. {
                    continue;
                }
                let t = qt(&format!("{:02}:00", i), 9., NORMAL, muted).width(TIME_COL_W - 14.).align(Align::Right);
                let th = text_size(&t).1;
                g.children.push(text(&t, Rect::new(7., y, TIME_COL_W - 14., th), ia, dpr));
            }
            root.children.push(clip(cf, vec![g]));
        }

        // 7. status pill
        if cal.bridge && (!cal.connected || !cal.last_error.is_empty()) {
            let t = qt(if cal.last_error.is_empty() { "正在连接日历…" } else { &cal.last_error }, 12., NORMAL, text_c);
            let (tw, th) = text_size(&t);
            let f = Rect::new(center(w, tw + 28.), center(h, 34.), tw + 28., 34.);
            let mut p = rect(f, 17., elevated, None);
            p.children.push(text(&t, Rect::new(center(f.width, tw), center(34., th), tw, th), at(abs, f), dpr));
            root.children.push(p);
        }

        // 8. tooltip
        if let Some((k, item)) = self.hovered_event {
            if let Some(e) = cal.events.get(k) {
                let tw = 270. - 22.;
                let summary = qt(if e.summary.is_empty() { "(无标题)" } else { &e.summary }, 13., BOLD, text_c).width(tw).elide();
                let time = qt(&event_time_text(e), 11., NORMAL, muted).width(tw);
                let calname = cal.calendars.iter().find(|c| c.id == e.calendar_id).map(|c| c.name.clone()).unwrap_or_default();
                let loc_s = if e.location.is_empty() { calname } else { e.location.clone() };
                let loc = qt(&loc_s, 11., NORMAL, muted).width(tw).elide();
                let desc = qt(&e.description, 11., NORMAL, muted).width(tw).wrap().lines(3).elide();
                let mut rows: Vec<(TextBox, f32)> = vec![(summary.clone(), text_size(&summary).1), (time.clone(), text_size(&time).1)];
                if !loc_s.is_empty() {
                    rows.push((loc.clone(), text_size(&loc).1));
                }
                if !e.description.is_empty() {
                    rows.push((desc.clone(), text_size(&desc).1));
                }
                let ch: f32 = rows.iter().map(|r| r.1).sum::<f32>() + (rows.len() as f32 - 1.) * 4.;
                let th = ch + 22.;
                let anchor_x = item.x + item.width / 2.;
                let below = item.y + item.height + 7.;
                let above = item.y - 7.;
                let x = 8f32.max((w - 270. - 8.).min(anchor_x - 135.));
                let y = if below + th > h - 8. { 8f32.max(above - th) } else { below };
                let f = Rect::new(x, y, 270., th);
                let fa = at(abs, f);
                let mut tip = rect(f, 12., pal.module_background, Some((1., line)));
                let mut yy = 11.;
                for (tb, rh) in rows {
                    tip.children.push(text(&tb, Rect::new(11., yy, tw, rh), fa, dpr));
                    yy += rh + 4.;
                }
                root.children.push(tip);
            }
        }
        self.zones = zones;
        self.key = self.key_of(model);
        root
    }

    fn key_of(&self, model: &Model) -> String {
        format!(
            "{:?}|{:?}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            model.theme,
            model.calendar,
            model.wall,
            self.content_x,
            self.content_y,
            self.pending_week,
            self.hovered_event,
            self.inter.hovered,
            self.inter.pressed,
            self.btn_color.iter().map(|a| a.get(model.now)).collect::<Vec<_>>(),
            self.btn_scale.iter().map(|a| a.get(model.now).to_bits()).collect::<Vec<_>>(),
            (self.width, self.height, model.output.scale),
            (self.scroll_x.grabbed(), self.scroll_y.grabbed()),
        )
    }

    /// Build into a host node (`slot`): replace its children when anything changed.
    pub fn update(&mut self, scene: &mut Scene, slot: NodeId, model: &Model, abs: (f32, f32)) -> bool {
        self.ensure_init(model);
        self.sync_scroll(model);
        self.sync_buttons(model);
        if self.key_of(model) == self.key && self.root.is_some() {
            return false;
        }
        let n = self.build(model, abs);
        self.root = Some(n.id());
        scene.edit(slot, |s| s.children = vec![n])
    }

    // ------------------------------------------------------------------ input

    /// Panel-local events are given in absolute surface coordinates; `abs` = panel origin.
    pub fn pointer(&mut self, ev: Pointer, model: &Model, abs: (f32, f32)) -> Reply<Action> {
        let mut r = Reply::default();
        let now = model.now;
        self.sync_scroll(model);
        // QML items exist (and hit-test) as soon as the panel is presented; the hit zones
        // come from `build`, so build them if no frame was built since presenting.
        if self.zones.is_empty() {
            let _ = self.build(model, abs);
        }
        if self.delayed_press.is_some_and(|(_, deadline)| now >= deadline) { self.replay_press(); }
        // pointerInside (HoverHandler)
        let inside = match ev {
            Pointer::Motion { x, y } | Pointer::Press { x, y, .. } | Pointer::Release { x, y, .. } | Pointer::Wheel { x, y, .. } => Rect::new(abs.0, abs.1, self.width, self.height).contains(x, y),
            Pointer::Leave | Pointer::Cancel => false,
        };
        if matches!(ev, Pointer::Leave | Pointer::Cancel) {
            // lost grab: Flickable::mouseUngrabEvent → cancelInteraction; no delayed press
            self.scroll_x.cancel_interaction();
            self.scroll_y.cancel_interaction();
            self.delayed_press = None;
            self.content_x = self.scroll_x.content_y() as f32;
            self.content_y = self.scroll_y.content_y() as f32;
        }
        if inside != self.pointer_inside {
            self.pointer_inside = inside;
            r.actions.push(Action::Host(HostAction::CalendarPointerInside(inside)));
        }
        if let Pointer::Wheel { x, y, angle_x, angle_y, pixel_x, pixel_y } = ev {
            let wheel = self.wheel(x, y, Wheel::deltas(angle_x as f64, pixel_x as f64), Wheel::deltas(angle_y as f64, pixel_y as f64), model);
            r.actions.extend(wheel.actions);
            r.redraw |= wheel.redraw;
            return r;
        }
        match ev {
            Pointer::Press { x, y, button: Button::Left } if Interaction::top(&self.zones, x, y).is_some_and(|z| matches!(z.id, Z::Timeline | Z::TimedEvent(_))) => {
                self.scroll_x.press(x as f64, now);
                self.scroll_y.press(y as f64, now);
                if Interaction::top(&self.zones, x, y).is_some_and(|z| matches!(z.id, Z::TimedEvent(_))) {
                    // No pressed bindings/actions on event cards: replay once after
                    // 80 ms, or immediately before an early click release, as Qt does.
                    self.delayed_press = Some((ev, now + PRESS_DELAY));
                    return r;
                }
            }
            Pointer::Motion { x, y } if Flickable::drag_both(&mut self.scroll_x, &mut self.scroll_y, (x as f64, y as f64), None, now) => {
                self.delayed_press = None;
                self.inter.pressed = None;
                self.inter.hovered = None;
                self.hovered_event = None;
                self.content_x = self.scroll_x.content_y() as f32;
                self.content_y = self.scroll_y.content_y() as f32;
                r.actions.extend(self.rebase_scroll(model));
                r.redraw = true;
                return r;
            }
            Pointer::Release { x, y, button: Button::Left } => {
                let gx = self.scroll_x.release(x as f64, now);
                let gy = self.scroll_y.release(y as f64, now);
                if gx || gy {
                    self.delayed_press = None;
                    self.inter.pressed = None;
                    self.inter.hovered = None;
                    self.hovered_event = None;
                    r.redraw = true;
                    return r;
                }
                self.replay_press();
            }
            _ => {}
        }
        let zones = self.zones.clone();
        let before = self.inter.hovered.clone();
        match self.inter.feed(ev, &zones) {
            Hit::Hover | Hit::Pressed(..) => r.redraw = true,
            Hit::Released { id, button: Button::Left, inside, .. } => {
                r.redraw = true;
                if inside { r.actions.extend(self.click(&id, model)); }
            }
            _ => {}
        }
        // event hover tooltips (MouseArea entered/exited)
        if self.inter.hovered != before {
            if let Some(Z::AllDayEvent(k) | Z::TimedEvent(k)) = &before {
                let ev_idx = self.event_index(&before.clone().unwrap(), *k, model);
                if self.hovered_event.map(|h| h.0) == ev_idx {
                    self.hovered_event = None;
                }
            }
            if let Some(id @ (Z::AllDayEvent(_) | Z::TimedEvent(_))) = &self.inter.hovered {
                let k = match id {
                    Z::AllDayEvent(k) | Z::TimedEvent(k) => *k,
                    _ => unreachable!(),
                };
                if let Some(e) = self.event_index(id, k, model) {
                    let rect = self.event_item_rect(id, k, model);
                    self.hovered_event = Some((e, rect));
                }
            }
            r.redraw = true;
        }
        self.sync_buttons(model);
        r.cursor = Some(self.inter.cursor(&self.zones));
        let _ = abs;
        r
    }

    /// Native two-axis wheel input. The three QML WheelHandlers default to Vertical:
    /// a pure horizontal event initially falls through to the timeline Flickable.
    pub fn wheel(&mut self, x: f32, y: f32, horizontal: Wheel, vertical: Wheel, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        self.sync_scroll(model);
        let Some(z) = Interaction::top(&self.zones, x, y) else { return r; };
        let area = match z.id {
            Z::DayHeader => 0,
            Z::AllDayArea | Z::AllDayEvent(_) => 1,
            Z::Timeline | Z::TimedEvent(_) => 2,
            _ => return r,
        };
        let now = model.now;
        let active = self.wheel_active[area].is_some_and(|until| now < until);
        if vertical.angle_delta != 0. || vertical.pixel_delta != 0. || active {
            // active has no QML consumers. Evaluate its timeout lazily at the next
            // input, avoiding a wakeup solely for an unobserved handler property.
            self.wheel_active[area] = match vertical.phase {
                ScrollPhase::End => None,
                ScrollPhase::None => Some(now + Duration::from_millis(100)),
                _ => Some(Duration::MAX),
            };
            let dx = if horizontal.pixel_delta != 0. { horizontal.pixel_delta } else { horizontal.angle_delta };
            let dy = if vertical.pixel_delta != 0. { vertical.pixel_delta } else { vertical.angle_delta };
            if dx != 0. || dy != 0. {
                if dx.abs() > dy.abs() {
                    let step = if horizontal.pixel_delta != 0. { horizontal.pixel_delta } else { dx / 120. * self.day_width() as f64 };
                    r.actions.extend(self.set_content_x((self.scroll_x.content_y() - step) as f32, model));
                } else {
                    let step = if vertical.pixel_delta != 0. { vertical.pixel_delta } else { dy / 120. * HOUR as f64 * 3. };
                    self.scroll_y.set_content_y((self.scroll_y.content_y() - step).clamp(0., self.scroll_y.maximum_y()));
                    self.content_y = self.scroll_y.content_y() as f32;
                }
                r.redraw = true;
            }
        } else if area == 2 {
            // Header/all-day WheelHandlers have no Flickable underneath them.
            r.redraw = self.scroll_x.wheel(horizontal, now);
            r.redraw |= self.scroll_y.wheel(vertical, now);
            self.content_x = self.scroll_x.content_y() as f32;
            self.content_y = self.scroll_y.content_y() as f32;
            r.actions.extend(self.rebase_scroll(model));
        }
        r
    }

    fn event_index(&self, id: &Z, k: usize, model: &Model) -> Option<usize> {
        let (all_day, timed, _) = self.lane_count_and_layout(model);
        match id {
            Z::AllDayEvent(_) => all_day.get(k).map(|i| i.event),
            Z::TimedEvent(_) => timed.get(k).map(|i| i.event),
            _ => None,
        }
    }
    /// Unclipped item rect in panel coordinates (`mapToItem(root, …)`).
    fn event_item_rect(&self, id: &Z, k: usize, model: &Model) -> Rect {
        let (all_day, timed, lanes) = self.lane_count_and_layout(model);
        let dw = self.day_width();
        match id {
            Z::AllDayEvent(_) => {
                let it = &all_day[k];
                Rect::new(TIME_COL_W + it.start_day as f32 * dw + 3. - self.content_x, TOOLBAR_H + DAY_HEADER_H + 6. + it.lane as f32 * 24., it.span as f32 * dw - 6., 20.)
            }
            _ => {
                let it = &timed[k];
                let tl = self.timeline_rect(lanes);
                let lane_w = (dw - 6.) / it.lanes as f32;
                Rect::new(
                    tl.x - self.content_x + it.day as f32 * dw + 3. + it.lane as f32 * lane_w,
                    tl.y - self.content_y + it.start_minute as f32 / 60. * HOUR + 1.,
                    18f32.max(lane_w - 2.),
                    18f32.max(((it.end_minute - it.start_minute) / 60.) as f32 * HOUR - 2.),
                )
            }
        }
    }

    fn click(&mut self, id: &Z, model: &Model) -> Vec<Action> {
        match id {
            Z::Create => {
                if model.calendar.bridge {
                    vec![Action::CalendarNewEvent]
                } else {
                    vec![]
                }
            }
            Z::Prev | Z::Next => {
                let dir = if *id == Z::Prev { -1. } else { 1. };
                if !self.h_init && self.day_width() > 0. {
                    self.content_x = LEADING as f32 * self.day_width();
                    self.h_init = true;
                }
                self.set_content_x(self.content_x + dir * 7. * self.day_width(), model).into_iter().collect()
            }
            Z::Range => vec![Action::Host(HostAction::CloseCalendar { force: false })],
            Z::Expand => vec![Action::Host(HostAction::ExpandCalendar)],
            Z::AllDayEvent(k) | Z::TimedEvent(k) => {
                if !model.calendar.bridge {
                    return vec![];
                }
                match self.event_index(id, *k, model).and_then(|i| model.calendar.events.get(i)) {
                    Some(e) => vec![Action::CalendarOpenEvent { uid: e.uid.clone(), start: e.start.clone() }],
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// Becomes visible again (each presentation creates a new panel in the QML; call this
    /// to reuse an instance).
    pub fn reset(&mut self) {
        *self = Self::new(self.width, self.height);
    }
}

fn event_time_text(e: &CalendarEvent) -> String {
    if e.all_day {
        return "全天".into();
    }
    let (Some(s), Some(en)) = (js_date_local(&e.start), js_date_local(&e.end)) else { return String::new() };
    format!("{}月{}日 {:02}:{:02} – {:02}:{:02}", s.month(), s.day(), s.hour(), s.minute(), en.hour(), en.minute())
}

fn abs_rect(abs: (f32, f32), r: Rect) -> Rect {
    Rect::new(abs.0 + r.x, abs.1 + r.y, r.width, r.height)
}
fn clip_rect(r: Rect, c: Rect) -> Rect {
    let x0 = r.x.max(c.x);
    let y0 = r.y.max(c.y);
    let x1 = (r.x + r.width).min(c.x + c.width);
    let y1 = (r.y + r.height).min(c.y + c.height);
    Rect::new(x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
}

#[cfg(test)]
#[path = "scroll_tests/calendar.rs"]
mod scroll_tests;
