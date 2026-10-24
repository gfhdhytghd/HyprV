//! Wi-Fi / network popup (`WifiFallback`, ui-panels-wifi.md). One instance per bar
//! Wi-Fi icon; the host opens it from the bar icon or from the control-panel Wi-Fi tile.
use crate::{
    control_panel::{empty_card, message_card, status_card, StatusCard},
    icons,
    model::*,
    popup::{self, GlassIds, GlassStyle, Machine, PopupEvent},
    qtext::{self, Font, TextBox, BOLD, NORMAL},
    ui::*,
    widgets::{self, Chip, CHIP_HEIGHT},
};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Node, NodeId, Scene},
};
use hyprv_widgets::{containers::{Reveal, RevealTiming, Transition}, flickable::{Flickable, Settings}};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Z {
    Card,
    Close,
    Radio,
    Rescan,
    Advanced,
    List,
    Connect(String),
    Field(String),
    Join(String),
    Cancel(String),
}

const WIDTH: f32 = 384.;
const MAX_VISIBLE: usize = 5;
const ROW_H: f32 = 39.; // max(20 + 4 + 15, 24)
const PASSWORD_H: f32 = 82.; // 38 + 10 + 34
/// QTextInput cursor flash period (cursorFlashTime 1000 ms → 500 ms on/off).
pub const CURSOR_HALF_PERIOD_MS: u64 = 500;
/// The 60 ms `passwordFocusTimer` re-focus after expanding.
const FOCUS_DELAY_MS: u64 = 60;

fn timing() -> RevealTiming {
    RevealTiming { open_pause: 85, open_reveal: 280, content_delay: 60, open_fade: 180, open_slide: 220, open_offset: -10., close_pause: 35, close_reveal: 210, close_fade: 110, close_slide: 170, close_offset: -8. }
}
fn section_timing() -> RevealTiming {
    RevealTiming { open_pause: 0, open_reveal: 180, content_delay: 20, open_fade: 130, open_slide: 180, open_offset: -6., close_pause: 0, close_reveal: 150, close_fade: 90, close_slide: 130, close_offset: -4. }
}

struct Ids {
    root: NodeId,
    glass: GlassIds,
    key: String,
}

pub struct WifiPanel {
    pub m: Machine,
    pub popup_visible: bool,
    anchor: Anchor,
    output: String,
    pub inter: Interaction<Z>,
    zones: Vec<Zone<Z>>,
    displayed: Vec<WifiNetwork>,
    displayed_at: Duration,
    last_live: Option<Vec<WifiNetwork>>,
    pub expanded: String,
    /// The typed PSK: plaintext lives only here (zeroizing, never reallocated in place).
    password: SecretBuf,
    focused: bool,
    focus_at: Option<Duration>,
    cursor_epoch: Duration,
    sections: BTreeMap<String, Reveal>,
    scroll: f32,
    scrolling: Flickable,
    ids: Option<Ids>,
    last_card: Rect,
}

impl Default for WifiPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiPanel {
    pub fn new() -> Self {
        Self {
            m: Machine::new(timing()),
            popup_visible: false,
            anchor: Anchor::default(),
            output: String::new(),
            inter: Interaction::default(),
            zones: Vec::new(),
            displayed: Vec::new(),
            displayed_at: Duration::ZERO,
            last_live: None,
            expanded: String::new(),
            password: SecretBuf::new(),
            focused: false,
            focus_at: None,
            cursor_epoch: Duration::ZERO,
            sections: BTreeMap::new(),
            scroll: 0.,
            scrolling: Flickable::new(Settings::list_view()),
            ids: None,
            last_card: Rect::default(),
        }
    }

    /// Mapped but the content height is not known yet (no build since mapping); the
    /// QML binding has it immediately, so [`crate::Panels::tick`] measures first.
    pub fn needs_measure(&self) -> bool {
        self.m.mapped && self.m.reveal.full_height <= 0.
    }
    pub fn is_mapped(&self) -> bool {
        self.m.mapped
    }

    /// The bar icon moved / was (re)assigned (`onXChanged`/`onYChanged` reposition).
    pub fn set_anchor(&mut self, anchor: Anchor, output: &str) {
        self.anchor = anchor;
        self.output = output.to_string();
    }

    /// `openPopup()`
    pub fn open(&mut self, model: &Model) -> Vec<Action> {
        let now = model.now;
        let mut out = Vec::new();
        if model.network.networks.is_empty() {
            out.push(Action::WifiRescan);
        }
        let was_visible = self.popup_visible;
        self.popup_visible = true;
        if !was_visible {
            self.on_popup_visible(model, &mut out);
        }
        if self.m.closing {
            // cancel closing and animate open from the current partial state
            self.m.closing = false;
            self.m.requested = true;
            self.m.reveal.play_open(now);
            return out;
        }
        self.m.requested = true;
        if !self.m.mapped {
            self.m.map(now);
            out.insert(0, Action::Host(HostAction::Mapped(PanelKind::Wifi)));
        }
        out
    }
    /// `closePopup()`
    pub fn close(&mut self, model: &Model) -> Vec<Action> {
        let mut out = Vec::new();
        if !self.m.mapped || self.m.closing {
            if self.popup_visible {
                self.popup_visible = false;
                self.on_popup_visible(model, &mut out);
            }
            return out;
        }
        self.popup_visible = false;
        self.on_popup_visible(model, &mut out);
        self.m.requested = false;
        self.m.closing = true;
        self.m.reveal.play_close(model.now);
        out
    }
    /// `togglePopup()` (bar icon left click).
    pub fn toggle(&mut self, model: &Model) -> Vec<Action> {
        if self.popup_visible && !self.m.closing {
            self.close(model)
        } else {
            self.open(model)
        }
    }

    fn on_popup_visible(&mut self, model: &Model, out: &mut Vec<Action>) {
        out.push(Action::Host(HostAction::WifiPanelVisible(self.popup_visible)));
        if !self.popup_visible {
            self.collapse();
            self.focused = false;
        }
        self.sync_networks(model, true);
    }

    fn collapse(&mut self) {
        if !self.expanded.is_empty() {
            let now = Duration::ZERO;
            if let Some(r) = self.sections.get_mut(&self.expanded) {
                r.play_close(now);
            }
        }
        self.expanded.clear();
        self.password.clear();
    }

    /// `syncNetworkList(force)` (ui-panels-wifi.md §4.2).
    pub fn sync_networks(&mut self, model: &Model, force: bool) {
        let live = &model.network.networks;
        let now = model.now;
        let age = now.saturating_sub(self.displayed_at);
        let hold = self.popup_visible && model.network.wifi_radio_enabled && model.network.wifi_hardware_enabled;
        if !force && !self.expanded.is_empty() && live.iter().any(|n| n.ssid == self.expanded) {
            return;
        }
        if !live.is_empty() {
            let merge = hold && !self.displayed.is_empty() && live.len() < self.displayed.len() && age < ms(30_000);
            self.displayed = if merge {
                let mut seen = std::collections::BTreeSet::new();
                let mut m = Vec::new();
                for n in live.iter().chain(self.displayed.iter()) {
                    if !n.ssid.is_empty() && seen.insert(n.ssid.clone()) {
                        m.push(n.clone());
                    }
                }
                m
            } else {
                live.clone()
            };
            self.displayed_at = now;
        } else if !(hold && !self.displayed.is_empty() && age < ms(30_000)) {
            self.displayed.clear();
            self.displayed_at = now;
        }
        if !self.expanded.is_empty() && !self.displayed.iter().any(|n| n.ssid == self.expanded && n.secure && !n.known) {
            self.expanded.clear();
            self.password.clear();
        }
    }

    pub fn displayed(&self) -> &[WifiNetwork] {
        &self.displayed
    }

    pub fn tick(&mut self, model: &Model) -> Vec<Action> {
        let now = model.now;
        let mut out = Vec::new();
        for ev in self.m.tick(now) {
            if ev == PopupEvent::CloseFinished && self.m.finish_close() {
                self.scrolling.reset(0.);
                self.scroll = 0.;
                self.inter = Interaction::default();
                out.push(Action::Host(HostAction::Unmapped(PanelKind::Wifi)));
            }
        }
        for r in self.sections.values_mut() {
            r.advance(now);
        }
        self.sections.retain(|_, r| r.is_running() || r.values().height > 0.);
        if let Some(t) = self.focus_at {
            if now >= t {
                self.focus_at = None;
                if !self.expanded.is_empty() && self.m.mapped && !self.m.closing {
                    self.focused = true;
                    self.cursor_epoch = now;
                }
            }
        }
        self.sync_scroll();
        if self.m.mapped {
            self.scrolling.frame(now);
            self.scroll = self.scrolling.content_y() as f32;
        }
        out
    }

    pub fn animating(&self, now: Duration) -> bool {
        self.m.mapped && (self.m.animating() || self.scrolling.animating() || self.sections.values().any(|r| r.is_running()) || self.focus_at.is_some_and(|t| now < t))
    }
    /// Next timer: open timer, focus timer, or the cursor blink while the field has focus.
    pub fn next_deadline(&self, now: Duration) -> Option<Duration> {
        let blink = (self.focused && self.m.mapped).then(|| {
            let half = ms(CURSOR_HALF_PERIOD_MS);
            let k = now.saturating_sub(self.cursor_epoch).as_nanos() / half.as_nanos() + 1;
            self.cursor_epoch + half * k as u32
        });
        [self.m.next_deadline(), self.focus_at, blink].into_iter().flatten().min()
    }

    pub fn surface(&self, model: &Model) -> Option<SurfaceSpec> {
        self.m.mapped.then(|| SurfaceSpec {
            namespace: "shell:hyprv-wifi",
            layer: Layer::Overlay,
            anchors: [true; 4],
            exclusive_zone: -1,
            keyboard: KeyboardMode::OnDemand,
            size: None,
            input_region: None,
            output: if self.output.is_empty() { model.output.name.clone() } else { self.output.clone() },
        })
    }

    fn card_rect(&self, model: &Model, h: f32) -> Rect {
        let (w_out, h_out) = (model.output.width, model.output.height);
        let x = 8f32.max((w_out - WIDTH - 8.).min(w_out - WIDTH - 10.));
        let rel_y = self.anchor.bottom();
        let y = popup::below_or_above(rel_y, h, h_out);
        Rect::new(x, y, WIDTH, h)
    }

    fn style(pal: &Palette) -> GlassStyle {
        GlassStyle { radius: 19., fill: pal.glass_fill, stroke: pal.glass_stroke, surface_opacity: 0.82, shadow: Color::default() }
    }

    fn section_height(&self, ssid: &str) -> f32 {
        self.sections.get(ssid).map(|r| r.values().height).unwrap_or(0.)
    }
    fn card_height(&self, n: &WifiNetwork) -> f32 {
        let sh = self.section_height(&n.ssid);
        ROW_H + if sh > 0. { 10. + sh } else { 0. } + 20.
    }
    /// (content height, viewport height) of the list.
    fn list_extent(&self) -> (f32, f32) {
        let n = self.displayed.len();
        if n == 0 {
            return (0., 0.);
        }
        let heights: Vec<f32> = self.displayed.iter().map(|d| self.card_height(d)).collect();
        let content = heights.iter().sum::<f32>() + (n - 1) as f32 * 10.;
        let vis = n.min(MAX_VISIBLE);
        let capped = heights[..vis].iter().sum::<f32>() + (vis.saturating_sub(1)) as f32 * 10.;
        (content, content.min(capped))
    }

    fn content(&mut self, model: &Model, card_abs: (f32, f32)) -> (Vec<Node>, f32) {
        let pal = Palette::new(model.theme.dark);
        let mut zones = Vec::new();
        let inter = self.inter.clone();
        let mut c = Ctx { dpr: model.output.scale, pal: &pal, zones: &mut zones };
        let n = &model.network;
        let w = 364.;
        let cabs = (card_abs.0 + 10., card_abs.1 + 10.);
        let mut col = Vec::new();
        let mut y = 0.;
        // 1. header
        {
            let hf = Rect::new(0., 0., w, 36.);
            let ha = at(cabs, hf);
            let mut h = group(hf);
            h.children.push(text_vc(&widgets::label("Network", 17., BOLD, pal.primary_text), 0., 36., ha, c.dpr));
            let close = Chip::new(&pal, "Close").min(76.).r(9.).colors(with_alpha(pal.primary_text, if pal.dark { 0.08 } else { 0.12 }), pal.primary_text, pal.card_stroke);
            let cw = close.width();
            let (cn, _) = widgets::chip(&mut c, w - cw, center(36., CHIP_HEIGHT), ha, &close, Z::Close, &inter);
            h.children.push(cn);
            col.push(h);
            y += 36. + 10.;
        }
        // 2. status card
        {
            let connected = n.network_connected();
            let summary = match n.active_type {
                NetType::Wired => "Ethernet connected".to_string(),
                NetType::Other => format!("Connected via {}", if n.default_interface.is_empty() { "network" } else { &n.default_interface }),
                _ if !n.wifi_controls_available() => "No wireless device detected".into(),
                _ if !n.wifi_hardware_enabled => "Hardware blocked".into(),
                _ if !n.wifi_radio_enabled => "Wi-Fi disabled".into(),
                _ if n.active_type == NetType::Wifi || n.wifi_connected => {
                    format!("{}  {}%", if n.wifi_ssid.is_empty() { "Wi-Fi connected" } else { &n.wifi_ssid }, (n.wifi_signal_strength * 100.).round() as i32)
                }
                _ => "Not connected".into(),
            };
            let detail = if connected {
                format!("Interface: {}", if n.default_interface.is_empty() { "network" } else { &n.default_interface })
            } else if n.wifi_device_present {
                format!("Wireless interface: {}", if n.wifi_interface.is_empty() { "wifi" } else { &n.wifi_interface })
            } else {
                "No wireless device detected".into()
            };
            let status = match n.active_type {
                NetType::Wired => "Wired",
                NetType::Other => "Online",
                _ if !n.wifi_controls_available() => "No Wi-Fi",
                _ if !n.wifi_hardware_enabled => "Blocked",
                _ if n.wifi_radio_enabled => {
                    if n.active_type == NetType::Wifi || n.wifi_connected {
                        "Online"
                    } else {
                        "Ready"
                    }
                }
                _ => "Off",
            };
            let icon_src = icons::network_tray_icon(n, pal.dark);
            // status_card positions the icon at body-local (0, centred); give its abs origin
            let body_abs = (cabs.0 + 10., cabs.1 + y + 10.);
            let probe_h = {
                let st = widgets::label(&summary, 14., BOLD, pal.primary_text);
                let dt = widgets::label_n(&detail, 12., pal.muted_text);
                text_size(&st).1 + 4. + text_size(&dt).1
            };
            let icon_abs = (body_abs.0, body_abs.1 + center(probe_h.max(24.), 24.));
            let icon = widgets::icon_with_fallback(&pal, Rect::new(0., 0., 24., 24.), (icon_abs.0, icon_abs.1), &icon_src, icons::network_tray_glyph(n), &model.assets, c.dpr);
            let (node, h) = status_card(&mut c, Rect::new(0., y, w, 0.), cabs, StatusCard { glyph: "", summary: &summary, detail: &detail, status, on: connected }, Some(icon));
            col.push(node);
            y += h + 10.;
        }
        // 3. action row
        {
            let ctl = n.wifi_controls_available();
            let radio = Chip::new(&pal, if n.wifi_radio_enabled { "Turn Off" } else { "Turn On" }).min(96.).r(9.).colors(pal.card_strong_fill, pal.accent, with_alpha(pal.accent, 0.18)).disabled(!ctl || !n.wifi_hardware_enabled || n.action_busy);
            let rescan = Chip::new(&pal, "Rescan").min(88.).r(9.).colors(pal.card_fill, pal.primary_text, pal.card_stroke).disabled(!ctl || !n.wifi_radio_enabled || n.action_busy);
            let adv = Chip::new(&pal, "Advanced").min(98.).r(9.).colors(pal.card_fill, pal.primary_text, pal.card_stroke);
            let mut x = 0.;
            let mut row = group(Rect::new(0., y, w, CHIP_HEIGHT));
            let ra = at(cabs, row.frame);
            for (chip, id) in [(radio, Z::Radio), (rescan, Z::Rescan), (adv, Z::Advanced)] {
                let (cn, cw) = widgets::chip(&mut c, x, 0., ra, &chip, id, &inter);
                row.children.push(cn);
                x += cw + 10.;
            }
            col.push(row);
            y += CHIP_HEIGHT + 10.;
        }
        // 4. message
        if !n.action_message.is_empty() {
            let (mn, mh) = message_card(&mut c, Rect::new(0., y, w, 0.), cabs, &n.action_message);
            col.push(mn);
            y += mh + 10.;
        }
        // 5. empty / 6. list
        if self.displayed.is_empty() {
            let net = n.network_connected();
            let txt = if !n.wifi_controls_available() {
                if net { "Ethernet is active. No wireless device detected." } else { "No wireless device detected." }
            } else if n.wifi_radio_enabled {
                if net { "Ethernet is active. No visible Wi-Fi networks right now." } else { "No visible networks right now." }
            } else if net {
                "Ethernet is active. Turn Wi-Fi on to scan for wireless networks."
            } else {
                "Turn Wi-Fi on to scan for networks."
            };
            let (en, eh) = empty_card(&mut c, Rect::new(0., y, w, 0.), cabs, txt);
            col.push(en);
            y += eh + 10.;
        } else {
            let (_, view) = self.list_extent();
            self.sync_scroll();
            let lf = Rect::new(0., y, w, view);
            let la = at(cabs, lf);
            c.zone(Z::List, abs(cabs, lf), Cursor::Default);
            let mut items = Vec::new();
            let mut iy = -self.scroll;
            let mut list_zones = Vec::new();
            let displayed = self.displayed.clone();
            for net in &displayed {
                let ch = self.card_height(net);
                let cf = Rect::new(0., iy, w, ch);
                let ca = at(la, cf);
                let mut lc = Ctx { dpr: c.dpr, pal: &pal, zones: &mut list_zones };
                items.push(self.network_card(&mut lc, net, cf, ca, model, &inter));
                iy += ch + 10.;
            }
            // clip zones to the viewport
            for mut z in list_zones {
                let top = la.1.max(z.rect.y);
                let bottom = (la.1 + view).min(z.rect.y + z.rect.height);
                if bottom > top {
                    z.rect = Rect::new(z.rect.x, top, z.rect.width, bottom - top);
                    c.zones.push(z);
                }
            }
            col.push(clip(lf, items));
            y += view + 10.;
        }
        let content_h = y - 10.;
        self.zones = zones;
        (vec![group_with(Rect::new(10., 10., w, content_h), col)], content_h + 20.)
    }

    fn network_card(&self, c: &mut Ctx<Z>, net: &WifiNetwork, cf: Rect, ca: (f32, f32), model: &Model, inter: &Interaction<Z>) -> Node {
        let pal = c.pal;
        let n = &model.network;
        let mut card = rect(cf, 9., panel_color(if net.active { pal.accent_fill } else { pal.card_fill }), Some((1., if net.active { pal.accent_stroke } else { pal.card_stroke })));
        let bw = cf.width - 20.;
        let row = Rect::new(10., 10., bw, ROW_H);
        let ra = at(ca, row);
        let mut rn = group(row);
        let expanded = self.expanded == net.ssid;
        let icon = widgets::icon_with_fallback(pal, Rect::new(0., center(ROW_H, 24.), 24., 24.), ra, &icons::wifi_list_icon(net.signal as i32, net.secure, pal.dark), icons::wifi_signal_glyph(net.signal as i32), &model.assets, c.dpr);
        rn.children.push(icon);
        let glyph = if net.active {
            "\u{F08B}"
        } else if net.enterprise && !net.known {
            "\u{F013}"
        } else if net.secure && !net.known && !expanded {
            "\u{F084}"
        } else {
            "\u{F090}"
        };
        let chip = Chip::icon(pal, glyph)
            .min(50.)
            .r(9.)
            .colors(if net.active { pal.card_strong_fill } else { pal.card_fill }, if net.active { pal.critical } else { pal.accent }, if net.active { with_alpha(pal.critical, 0.2) } else { with_alpha(pal.accent, 0.18) })
            .disabled(n.action_busy || (!n.wifi_radio_enabled && !net.active) || (expanded && net.secure && !net.known && self.password.is_empty()));
        let chw = chip.width();
        let (cn, _) = widgets::chip(c, bw - chw, center(ROW_H, CHIP_HEIGHT), ra, &chip, Z::Connect(net.ssid.clone()), inter);
        rn.children.push(cn);
        // info column
        let ix = 24. + 10.;
        let iw = (bw - chw - 10. - ix).max(0.);
        let badge_w = if net.secure { 22. + 6. } else { 0. };
        let title_natural = qtext::measure(&net.ssid, &Font::native(14., BOLD)).width;
        let tw = (title_natural.min(iw - badge_w)).max(0.);
        let tt = widgets::label(&net.ssid, 14., BOLD, pal.primary_text).width(tw).elide();
        let th = text_size(&tt).1;
        let line_h = th.max(20.);
        let meta = network_meta(net);
        let mt = widgets::label_n(&meta, 11., pal.muted_text).width(iw).elide();
        let mh = text_size(&mt).1;
        let info_h = line_h + 4. + mh;
        let iframe = Rect::new(ix, center(ROW_H, info_h), iw, info_h);
        let ia = at(ra, iframe);
        let mut info = group(iframe);
        let lf = Rect::new(0., 0., iw, line_h);
        let la = at(ia, lf);
        let mut line = clip(lf, vec![]);
        let row_h = th.max(if net.secure { 20. } else { 0. });
        let ry = center(line_h, row_h);
        line.children.push(text(&tt, Rect::new(0., ry + center(row_h, th), tw, th), la, c.dpr));
        if net.secure {
            line.children.push(widgets::badge(pal, Rect::new(tw + 6., ry + center(row_h, 20.), 22., 20.), la, 9., c.dpr));
        }
        info.children.push(line);
        info.children.push(text(&mt, Rect::new(0., line_h + 4., iw, mh), ia, c.dpr));
        rn.children.push(info);
        card.children.push(rn);
        // password section (ExpandableSection: layout height follows reveal, content slide applied)
        let sh = self.section_height(&net.ssid);
        if sh > 0. {
            let sec = self.sections.get(&net.ssid).unwrap();
            let v = sec.values();
            let sf = Rect::new(10., 10. + ROW_H + 10., bw, sh);
            let sa = at(ca, sf);
            let mut content = group(Rect::new(0., v.offset, bw, PASSWORD_H));
            content.opacity = v.opacity;
            let coa = (sa.0, sa.1 + v.offset);
            // field
            let ff = Rect::new(0., 0., bw, 38.);
            let fa = at(coa, ff);
            let focused = self.focused && expanded;
            let mut field = rect(ff, 9., panel_color(pal.input_fill), Some((1., if focused { pal.accent_stroke } else { pal.card_stroke })));
            let iwid = bw - 20.;
            // echoMode Password: only the mask reaches layout/glyph/scene caches
            let pw = if expanded { MASK.to_string().repeat(self.password.char_count()) } else { String::new() };
            let input = TextBox::new(pw.clone(), Font::native(13., NORMAL).spacing(3.), pal.primary_text);
            let (tw_in, thi) = (qtext::layout(&input).implicit_width as f32, text_size(&input).1);
            let scroll_x = (tw_in + 1. - iwid).max(0.);
            let ty = center(38., thi);
            if !pw.is_empty() {
                field.children.push(text(&input, Rect::new(10. - scroll_x, ty, tw_in, thi), fa, c.dpr));
            } else {
                let ph = widgets::label_n("Password", 13., pal.soft_text);
                field.children.push(text_vc(&ph, 10., 38., fa, c.dpr));
            }
            if focused && cursor_visible(self.cursor_epoch, model.now) {
                // QQuickTextInput cursor: 1 px wide, line height, text colour, after the text
                let adv = qtext::layout(&input).advance as f32;
                field.children.push(rect(Rect::new(10. - scroll_x + adv, ty, 1., thi), 0., pal.primary_text, None));
            }
            content.children.push(field);
            c.zone(Z::Field(net.ssid.clone()), abs(sa, Rect::new(0., v.offset, bw, 38.)).intersect_clip(abs(sa, Rect::new(0., 0., bw, sh))), Cursor::Default);
            let join = Chip::new(pal, "Join").min(82.).r(9.).colors(pal.card_strong_fill, pal.accent, with_alpha(pal.accent, 0.18)).disabled(self.password.is_empty() || n.action_busy);
            let cancel = Chip::new(pal, "Cancel").min(82.).r(9.).colors(pal.card_fill, pal.primary_text, pal.card_stroke);
            let (jn, jw) = widgets::chip(c, 0., 48., coa, &join, Z::Join(net.ssid.clone()), inter);
            let (kn, _) = widgets::chip(c, jw + 10., 48., coa, &cancel, Z::Cancel(net.ssid.clone()), inter);
            content.children.push(jn);
            content.children.push(kn);
            card.children.push(clip(sf, vec![content]));
        }
        card
    }

    fn key_string(&self, model: &Model) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{}|{:?}|{}|{:?}",
            model.theme,
            model.network,
            self.displayed,
            self.inter.hovered,
            self.inter.pressed,
            model.output,
            self.expanded,
            self.password.char_count(),
            self.focused && cursor_visible(self.cursor_epoch, model.now),
            self.sections.iter().map(|(k, r)| (k.clone(), r.values().height.to_bits(), r.values().opacity.to_bits(), r.values().offset.to_bits())).collect::<Vec<_>>(),
            self.scroll,
            self.anchor,
        )
    }

    pub fn build(&mut self, model: &Model) -> Node {
        let mut n = self.build_raw(model);
        crate::qrect::convert(&mut n, (0., 0.), model.output.scale);
        n
    }
    fn build_raw(&mut self, model: &Model) -> Node {
        self.observe(model);
        let mut root = group(Rect::new(0., 0., model.output.width, model.output.height));
        let rid = root.id();
        let probe = self.card_rect(model, 0.);
        let (nodes, h) = self.content(model, (probe.x, probe.y));
        self.m.set_full_height(h);
        let card = self.card_rect(model, h);
        // re-lay out if the vertical placement depends on the height (above-fallback)
        let (nodes, h) = if (card.y - probe.y).abs() > 1e-3 { self.content(model, (card.x, card.y)) } else { (nodes, h) };
        self.last_card = card;
        let pal = Palette::new(model.theme.dark);
        let v = self.m.reveal.values();
        let mut zones = vec![Zone { id: Z::Card, rect: card, cursor: Cursor::Default, enabled: true }];
        zones.append(&mut self.zones);
        self.zones = zones;
        let (cn, gids) = popup::glass(card, h, v.height, v.opacity, v.offset, &Self::style(&pal), nodes, model.output.scale);
        root.children.push(cn);
        self.ids = Some(Ids { root: rid, glass: gids, key: self.key_string(model) });
        root
    }

    fn observe(&mut self, model: &Model) {
        if self.last_live.as_ref() != Some(&model.network.networks) {
            let first = self.last_live.is_none();
            self.last_live = Some(model.network.networks.clone());
            self.sync_networks(model, first);
        }
    }

    pub fn update(&mut self, scene: &mut Scene, model: &Model) -> bool {
        let changed = self.update_raw(scene, model);
        crate::qrect::finalize(scene, model.output.scale);
        changed
    }
    fn update_raw(&mut self, scene: &mut Scene, model: &Model) -> bool {
        self.observe(model);
        let Some(ids) = &self.ids else { return false };
        if scene.root().find(ids.root).is_none() {
            return false;
        }
        let rev = scene.revision();
        let key = self.key_string(model);
        if key != ids.key {
            let rid = ids.root;
            let node = self.build(model);
            let children = node.children;
            let gids = self.ids.as_ref().unwrap().glass;
            scene.edit(rid, |n| n.children = children);
            self.ids = Some(Ids { root: rid, glass: gids, key });
        } else {
            let card = self.card_rect(model, self.m.reveal.full_height);
            self.last_card = card;
            if let Some(z) = self.zones.first_mut() {
                z.rect = card;
            }
            let v = self.m.reveal.values();
            popup::glass_update(scene, &ids.glass, (card.x, card.y), v.height, v.opacity);
        }
        scene.revision() != rev
    }

    // ------------------------------------------------------------------ input

    fn expand(&mut self, ssid: &str, now: Duration) {
        if !self.expanded.is_empty() && self.expanded != ssid {
            if let Some(r) = self.sections.get_mut(&self.expanded) {
                r.play_close(now);
            }
        }
        self.expanded = ssid.to_string();
        self.password.clear();
        let r = self.sections.entry(ssid.to_string()).or_insert_with(|| {
            let mut r = Reveal::new(PASSWORD_H);
            r.line_height = 0.;
            r.layout_height_follows_reveal = true;
            r.timing = section_timing();
            r.collapse_immediately();
            r
        });
        r.prepare_open();
        r.play_open(now);
        self.focused = true;
        self.cursor_epoch = now;
        self.focus_at = Some(now + ms(FOCUS_DELAY_MS));
    }
    fn collapse_at(&mut self, now: Duration) {
        if let Some(r) = self.sections.get_mut(&self.expanded) {
            if r.values().height > 0. || r.is_running() {
                r.play_close(now);
            }
        }
        self.expanded.clear();
        self.password.clear();
        self.focused = false;
    }

    /// `activateNetwork(n)` (ui-panels-wifi.md §4.1).
    fn activate(&mut self, ssid: &str, model: &Model) -> Vec<Action> {
        let now = model.now;
        let n = &model.network;
        if n.action_busy {
            return vec![];
        }
        let Some(net) = self.displayed.iter().find(|d| d.ssid == ssid).cloned() else { return vec![] };
        if net.active {
            self.collapse_at(now);
            return vec![Action::WifiDisconnect];
        }
        if net.enterprise && !net.known {
            return vec![Action::Host(HostAction::OpenWifiManager)];
        }
        if net.secure && !net.known && self.expanded == net.ssid && self.password.is_empty() {
            return vec![];
        }
        if net.secure && !net.known && self.expanded != net.ssid {
            self.expand(&net.ssid, now);
            return vec![];
        }
        // single exact-capacity copy; the host moves it into hyprv-svc-net (zeroized there)
        let password = if net.secure && !net.known { self.password.to_owned_exact() } else { String::new() };
        self.collapse_at(now);
        vec![Action::WifiConnect { ssid: net.ssid, password, security: net.security }]
    }

    pub fn key(&mut self, ev: &KeyEvent, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if !ev.pressed || !self.m.mapped {
            return r;
        }
        r.redraw = true;
        match &ev.key {
            Key::Escape => r.actions = self.close(model),
            Key::Return | Key::Enter if self.focused && !self.password.is_empty() => {
                let s = self.expanded.clone();
                r.actions = self.activate(&s, model);
            }
            Key::Backspace if self.focused => {
                self.password.pop();
                self.cursor_epoch = model.now;
            }
            // keypad Enter arrives as "\r" text from some keymaps: treat like Return
            Key::Text(t) if self.focused && (t == "\r" || t == "\n") => {
                if !self.password.is_empty() {
                    let s = self.expanded.clone();
                    r.actions = self.activate(&s, model);
                }
            }
            Key::Text(t) if self.focused => {
                // QInputControl::isAcceptableInput: control characters are rejected
                self.password.push_text(t);
                self.cursor_epoch = model.now;
            }
            _ => r.redraw = false,
        }
        r
    }

    fn sync_scroll(&mut self) {
        let (content, view) = self.list_extent();
        self.scrolling.set_extents(view as f64, content as f64);
        if content <= view {
            self.scrolling.cancel_interaction();
        }
        self.scroll = self.scrolling.content_y() as f32;
    }

    pub fn pointer(&mut self, ev: Pointer, model: &Model) -> Reply<Action> {
        let mut r = Reply::default();
        if !self.m.mapped {
            return r;
        }
        self.sync_scroll();
        if let Pointer::Wheel { x, y, angle_y, pixel_y, .. } = ev {
            let (content, view) = self.list_extent();
            let over = Interaction::top(&self.zones, x, y).is_some_and(|z| !matches!(z.id, Z::Card | Z::Close | Z::Radio | Z::Rescan | Z::Advanced));
            if over && content > view {
                // WifiFallback.qml:956-970: pixelDelta.y, else angleDelta.y / 2
                let step = if pixel_y != 0. { pixel_y } else { angle_y / 2. };
                self.scrolling.set_content_y((self.scrolling.content_y() - step as f64).clamp(0., (content - view) as f64));
                self.scroll = self.scrolling.content_y() as f32;
                r.redraw = true;
            }
            return r;
        }
        match ev {
            Pointer::Press { x, y, button: Button::Left } => {
                let (content, view) = self.list_extent();
                if content > view && self.zones.iter().any(|z| z.id == Z::List && z.rect.contains(x, y)) {
                    self.scrolling.press(y as f64, model.now);
                }
            }
            Pointer::Motion { y, .. } if self.scrolling.drag_to(y as f64, None, model.now) => {
                self.scroll = self.scrolling.content_y() as f32;
                self.inter.pressed = None;
                self.inter.hovered = None;
                r.redraw = true;
                return r;
            }
            Pointer::Release { y, button: Button::Left, .. } if self.scrolling.release(y as f64, model.now) => {
                self.inter.pressed = None;
                self.inter.hovered = None;
                r.redraw = true;
                return r;
            }
            // lost grab: Flickable::mouseUngrabEvent → cancelInteraction (fixup to bounds)
            Pointer::Leave | Pointer::Cancel => {
                self.scrolling.cancel_interaction();
                self.scroll = self.scrolling.content_y() as f32;
            }
            _ => {}
        }
        let zones = self.zones.clone();
        match self.inter.feed(ev, &zones) {
            Hit::Hover => r.redraw = true,
            Hit::Pressed(id, ..) => {
                r.redraw = true;
                if let Z::Field(s) = &id {
                    if *s == self.expanded {
                        self.focused = true;
                        self.cursor_epoch = model.now;
                    }
                } else {
                    // focus leaves the input when clicking elsewhere inside the card? Qt keeps
                    // activeFocus unless another item takes it; chips do not take focus.
                }
            }
            Hit::Released { id, button: Button::Left, inside: true, .. } => {
                r.redraw = true;
                match id {
                    Z::Close => r.actions = self.close(model),
                    Z::Radio => r.actions = vec![Action::WifiSetRadio(!model.network.wifi_radio_enabled)],
                    Z::Rescan => r.actions = vec![Action::WifiRescan],
                    Z::Advanced => r.actions = vec![Action::Host(HostAction::OpenWifiManager)],
                    Z::Connect(s) | Z::Join(s) => r.actions = self.activate(&s, model),
                    Z::Cancel(_) => {
                        self.collapse_at(model.now);
                        self.sync_networks(model, true);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if let Pointer::Press { x, y, .. } = ev {
            if !self.last_card.contains(x, y) {
                r.actions.extend(self.close(model));
                r.redraw = true;
            }
        }
        r.cursor = Some(self.inter.cursor(&self.zones));
        r
    }

    /// Content only (no glass chrome) at `card_abs`, for hosts that draw the chrome.
    pub fn content_node(&mut self, model: &Model, card_abs: (f32, f32)) -> (Node, f32, f32) {
        let mut r = self.content_node_raw(model, card_abs);
        crate::qrect::convert(&mut r.0, card_abs, model.output.scale);
        r
    }
    fn content_node_raw(&mut self, model: &Model, card_abs: (f32, f32)) -> (Node, f32, f32) {
        self.observe(model);
        let (nodes, h) = self.content(model, card_abs);
        let card = Rect::new(card_abs.0, card_abs.1, WIDTH, h);
        self.last_card = card;
        let mut zones = vec![Zone { id: Z::Card, rect: card, cursor: Cursor::Default, enabled: true }];
        zones.append(&mut self.zones);
        self.zones = zones;
        (group_with(Rect::new(0., 0., WIDTH, h), nodes), WIDTH, h)
    }

    /// Card at origin, fully revealed (offscreen tests).
    pub fn preview(&mut self, model: &Model) -> (Node, f32, f32) {
        let mut r = self.preview_raw(model);
        crate::qrect::convert(&mut r.0, (0., 0.), model.output.scale);
        r
    }
    fn preview_raw(&mut self, model: &Model) -> (Node, f32, f32) {
        self.observe(model);
        self.popup_visible = true;
        self.sync_networks(model, true);
        let (nodes, h) = self.content(model, (0., 0.));
        let pal = Palette::new(model.theme.dark);
        let (n, _) = popup::glass(Rect::new(0., 0., WIDTH, h), h, h, 1., 0., &Self::style(&pal), nodes, model.output.scale);
        (n, WIDTH, h)
    }
    /// Test hook: expand a secure network's password row immediately.
    pub fn expand_now(&mut self, ssid: &str, password: &str, focused: bool) {
        self.expand(ssid, Duration::ZERO);
        if let Some(r) = self.sections.get_mut(ssid) {
            r.reset();
        }
        self.password.clear();
        self.password.push_text(password);
        self.focused = focused;
        self.focus_at = None;
    }
}

fn cursor_visible(epoch: Duration, now: Duration) -> bool {
    (now.saturating_sub(epoch).as_millis() / CURSOR_HALF_PERIOD_MS as u128).is_multiple_of(2)
}

/// `networkMeta(n)`
pub fn network_meta(n: &WifiNetwork) -> String {
    let mut p = Vec::new();
    if n.active {
        p.push("Connected".to_string());
    } else if n.known {
        p.push("Saved".into());
    }
    if n.enterprise {
        p.push("802.1X".into());
    } else if !n.security.is_empty() {
        p.push(n.security.clone());
    } else {
        p.push("Open".into());
    }
    p.push(format!("{}%", n.signal));
    p.join("  •  ")
}

trait RectExt {
    fn intersect_clip(self, o: Rect) -> Rect;
}
impl RectExt for Rect {
    fn intersect_clip(self, o: Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.width).min(o.x + o.width);
        let y1 = (self.y + self.height).min(o.y + o.height);
        Rect::new(x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
    }
}

#[allow(dead_code)]
fn _unused(_: Transition) {}

#[cfg(test)]
#[path = "scroll_tests/wifi.rs"]
mod scroll_tests;

/// QQuickTextInput password mask character (U+25CF).
const MASK: char = '\u{25CF}';

/// Zeroizing PSK buffer. Capacity is reserved up front so typing never reallocates
/// (which would leave unwiped copies); bytes removed by `pop` and the whole buffer on
/// `clear`/drop are overwritten.
struct SecretBuf(zeroize::Zeroizing<String>);

impl SecretBuf {
    const CAPACITY: usize = 512;
    fn new() -> Self {
        Self(zeroize::Zeroizing::new(String::with_capacity(Self::CAPACITY)))
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn char_count(&self) -> usize {
        self.0.chars().count()
    }
    /// Append typed text, dropping control characters (Qt rejects them).
    fn push_text(&mut self, t: &str) {
        for c in t.chars().filter(|c| !c.is_control()) {
            if self.0.len() + c.len_utf8() > self.0.capacity() {
                // grow into a fresh zeroizing buffer; the old one is wiped on drop
                let mut bigger = String::with_capacity(self.0.capacity() * 2);
                bigger.push_str(&self.0);
                self.0 = zeroize::Zeroizing::new(bigger);
            }
            self.0.push(c);
        }
    }
    fn pop(&mut self) {
        let Some(c) = self.0.chars().next_back() else { return };
        let new_len = self.0.len() - c.len_utf8();
        // SAFETY: zeroing the bytes of a whole trailing char, then truncating at its
        // start (a char boundary), leaves valid UTF-8.
        unsafe {
            let v = self.0.as_mut_vec();
            for b in &mut v[new_len..] {
                std::ptr::write_volatile(b, 0);
            }
            v.truncate(new_len);
        }
    }
    fn clear(&mut self) {
        use zeroize::Zeroize;
        // String::zeroize wipes the full capacity and clears; keep the reservation
        self.0.zeroize();
        self.0.reserve(Self::CAPACITY);
    }
    /// A copy for `Action::WifiConnect` with exactly the needed capacity.
    fn to_owned_exact(&self) -> String {
        let mut s = String::with_capacity(self.0.len());
        s.push_str(&self.0);
        s
    }
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    use crate::ui::{Key, KeyEvent};

    const SECRET: &str = "hunter2-secret";

    fn typed(p: &mut WifiPanel, m: &Model, t: &str) {
        p.key(&KeyEvent { key: Key::Text(t.into()), pressed: true, autorepeat: false }, m);
    }

    /// PANEL-1: typing, rendering and submitting/escaping never put the plaintext into the
    /// text caches, the scene diff key or an action's Debug output.
    #[test]
    fn password_never_reaches_caches_or_keys() {
        let m = crate::fixtures::control_idle(false);
        let mut p = WifiPanel::new();
        p.open(&m);
        p.expand_now("Neighbour", "", true);
        let mut keys = Vec::new();
        for c in SECRET.chars() {
            typed(&mut p, &m, &c.to_string());
            let _ = p.build(&m);
            keys.push(p.key_string(&m));
        }
        assert_eq!(p.password.char_count(), SECRET.chars().count());
        let leaks = |what: &str, v: &[String]| {
            for k in v {
                for n in 4..=SECRET.len() {
                    assert!(!k.contains(&SECRET[..n]), "{what} contains a password prefix: {k:?}");
                }
            }
        };
        leaks("scene key", &keys);
        leaks("text cache", &qtext::debug_cache_keys());
        // Return submits a copy; its Debug is redacted and the panel buffer is wiped
        let r = p.key(&KeyEvent { key: Key::Return, pressed: true, autorepeat: false }, &m);
        let sent: Vec<String> = r.actions.iter().map(|a| format!("{a:?}")).collect();
        assert!(r.actions.iter().any(|a| matches!(a, Action::WifiConnect { password, .. } if password == SECRET)));
        leaks("action debug", &sent);
        assert!(p.password.is_empty());
        leaks("text cache after submit", &qtext::debug_cache_keys());
    }

    #[test]
    fn secret_buffer_wipes_and_never_reallocates_while_typing() {
        let mut b = SecretBuf::new();
        let ptr = b.0.as_ptr();
        b.push_text(SECRET);
        assert_eq!(b.0.as_ptr(), ptr, "typing must not reallocate");
        b.pop();
        // the popped byte is zeroed in the spare capacity
        let spare = unsafe { *b.0.as_ptr().add(SECRET.len() - 1) };
        assert_eq!(spare, 0);
        b.clear();
        let wiped = unsafe { std::slice::from_raw_parts(b.0.as_ptr(), SECRET.len()) };
        assert!(wiped.iter().all(|&x| x == 0));
    }

    /// PANEL-3: control characters are dropped (QInputControl::isAcceptableInput) and a
    /// "\r" text event submits like Return.
    #[test]
    fn control_characters_are_rejected() {
        let m = crate::fixtures::control_idle(false);
        let mut p = WifiPanel::new();
        p.open(&m);
        p.expand_now("Neighbour", "", true);
        typed(&mut p, &m, "\u{15}\u{1b}\u{8}");
        assert!(p.password.is_empty());
        typed(&mut p, &m, "ab\u{7f}c");
        assert_eq!(&*p.password.0, "abc");
        let r = p.key(&KeyEvent { key: Key::Text("\r".into()), pressed: true, autorepeat: false }, &m);
        assert!(r.actions.iter().any(|a| matches!(a, Action::WifiConnect { password, .. } if password == "abc")));
    }
}
