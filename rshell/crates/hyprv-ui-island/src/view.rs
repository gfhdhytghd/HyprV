//! Scene construction for one evaluated [`Island`] state (ui-island.md §3).
//! The root node is island-local `(0, 0, width, height)` with `clip: true`; the host
//! positions it (and applies the calendar fade `1 − calendarProgress`).
use crate::island::{Derived, Island, AGENT_DETACHED_GAP, AGENT_MINIMAL_WIDTH, COLLAPSED_HEIGHT, MUSIC_DETACHED_WIDTH, ROW_HEIGHT, ROW_SPACING};
use crate::model::{self as m, Agent};
use crate::util::{centered_text, clip_group, group, hc, measure, qt_rect, text, vcenter, HAlign, BLACK, BOLD, DEMIBOLD};
use hyprv_ui::{
    animation::Color,
    geometry::Rect,
    scene::{Content, Node},
    skia::Image,
};
use hyprv_widgets::{
    controls::Control,
    viz::ProgressRing,
};

impl Island {
    /// Fresh subtree for the current state at the island's last evaluated time.
    pub fn build(&self) -> Node {
        let d = self.derived();
        let (w, h) = (self.w(), self.h());
        let mut root = clip_group(Rect::new(0., 0., w, h));
        root.native_materials = Some(true);
        let emusic_high = self.emusic_high(&d);

        root.add(self.island_frame(&d));
        if !emusic_high {
            root.add(self.music_expanded_frame());
        }
        root.add(self.idle_content(&d));
        root.add(self.compact_music(&d));
        if !emusic_high {
            root.add(self.expanded_music(&d));
        }
        root.add(self.compact_agent(&d));
        root.add(self.expanded_agent(&d));
        if emusic_high {
            root.add(self.music_expanded_frame());
            root.add(self.expanded_music(&d));
        }
        enable_native_text(&mut root);
        root
    }

    fn surface(&self) -> Color {
        self.tok.pal.module_background
    }

    fn island_frame(&self, d: &Derived) -> Node {
        let f = self.frame_rect(d);
        let border = (d.music_primary || d.agent_active).then_some((1., self.tok.island_stroke));
        let mut n = qt_rect(f, self.a.frame_r.get(self.now) as f32, self.surface(), border);
        n.qt_rect_size=Some((self.a.frame_w.get(self.now),if d.pfc {COLLAPSED_HEIGHT as f64}else{self.a.height.get(self.now)}));
        n.visible = !d.pfc;
        n.clip = Some([0.;4]); // islandFrame.clip: true, including its AA fringe.
        n
    }

    fn music_expanded_frame(&self) -> Node {
        let r = self.mef_rect();
        let mut n = qt_rect(r, self.a.mef_r.get(self.now) as f32, self.surface(), Some((1., self.tok.island_stroke)));
        n.qt_rect_size=Some((self.a.mef_w.get(self.now),self.a.height.get(self.now)));
        n.opacity = self.a.mef_op.get(self.now) as f32;
        n.visible = n.opacity > 0.01;
        n.clip = Some([0.;4]); // musicExpandedFrame.clip: true.
        n
    }

    fn faded(&self, frame: Rect, opacity: f64) -> Node {
        let mut g = group(frame);
        g.opacity = opacity as f32;
        g.visible = opacity > 0.01;
        g
    }

    fn idle_content(&self, d: &Derived) -> Node {
        let f = self.frame_rect(d);
        let mut g = self.faded(f, self.a.idle_op.get(self.now));
        if !g.visible {
            return g;
        }
        let r = self.idle_rects(d);
        for (b, rect) in [(&self.lock, r[0]), (&self.clock, r[1]), (&self.power, r[2])] {
            let mut b = b.clone();
            b.frame = rect;
            let mut button = b.build(&self.tok, self.now);
            if let hyprv_widgets::controls::PillStyle::Island { padding_left, padding_right } = b.style {
                for child in &mut button.children {
                    if matches!(child.content, Content::Text(_)) {
                        *child = text(&b.label, padding_left, vcenter(rect.height, measure(&b.label,16.,BOLD).height),
                            Some(rect.width-padding_left-padding_right), HAlign::Center,16.,BOLD,
                            b.label_color.unwrap_or(self.tok.pal.primary_text));
                    }
                }
            }
            g.add(button);
        }
        g
    }

    /// `AlbumArtFrame` at `(x, y)`, `size`², radius `r` (§3.2).
    fn album_art(&self, d: &Derived, x: f32, y: f32, size: f32, r: f32) -> Node {
        let mut g = group(Rect::new(x, y, size, size));
        g.clip = Some([0.; 4]); // AlbumArtFrame.clip: true
        g.add(qt_rect(Rect::new(0., 0., size, size), r, self.tok.island_soft_fill, None));
        let image = if d.music_active && !self.inputs.media.art_url.is_empty() { self.inputs.art.clone() } else { None };
        match image {
            Some(img) => {
                // DynamicIsland.qml:415-432: Image (PreserveAspectCrop, mipmap) with
                // layer.effect MultiEffect { maskSource: <Rectangle radius r>;
                // maskThresholdMin: 0.5; maskSpreadAtMin: 1 } (engine effect)
                let mut masked = group(Rect::new(0., 0., size, size));
                masked.effects.multi_effect = Some(hyprv_ui::effects::MultiEffect {
                    mask: Some(hyprv_ui::effects::Mask {
                        source: hyprv_ui::effects::MaskSource::Rect { radius: r.min(size / 2.) },
                        threshold_min: 0.5,
                        spread_at_min: 1.,
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                masked.add(cover(img, size, size));
                g.add(masked);
            }
            None => {
                let px = (size * 0.46).round();
                g.add(centered_text("\u{f001}", 0., 0., size, size, px, BOLD, self.tok.pal.accent));
            }
        }
        g
    }

    fn spectrum(&self, x: f32, y: f32, expanded: bool, wave: usize) -> Node {
        let (w,h) = if expanded { (70.,56.) } else { (22.,22.) };
        crate::spectrum::build(Rect::new(x,y,w,h), &self.inputs.spectrum,
            self.inputs.media.playing, self.waves[wave].phase, &self.tok, expanded)
    }

    fn compact_music(&self, d: &Derived) -> Node {
        let f = self.frame_rect(d);
        let mut g = self.faded(f, self.a.cmusic_op.get(self.now));
        if !g.visible {
            return g;
        }
        // Row margins 7/7/6/6, spacing 3, children v-centred in 26.
        g.add(self.album_art(d, 7., 6. + vcenter(f.height - 12., 24.), 24., 8.));
        let clock = &self.inputs.now_hh_mm;
        let th = measure(clock, 16., BOLD).height;
        g.add(text(clock, 34., 6. + vcenter(f.height - 12., th), Some(54.), HAlign::Center, 16., BOLD, self.tok.pal.primary_text));
        g.add(self.spectrum(91., 6. + vcenter(f.height - 12., 22.), false, 0));
        g
    }

    fn expanded_music(&self, d: &Derived) -> Node {
        let b = self.emusic_rect(d);
        let mut g = self.faded(b, self.a.emusic_op.get(self.now));
        if !g.visible {
            return g;
        }
        let cw = b.width;
        let tok = &self.tok;
        let md = &self.inputs.media;
        // Top row (h 72, spacing 12).
        g.add(self.album_art(d, 0., 0., 70., 16.));
        let col_w = cw - 164.;
        let title = m::display_title(md);
        let th = measure(&title, 18., BOLD).height;
        let artist = md.artist.clone();
        let ah = measure(&artist, 13., DEMIBOLD).height;
        let col_h = if artist.is_empty() { th } else { th + 4. + ah };
        let cy = vcenter(72., col_h);
        g.add(text(&title, 82., cy, Some(col_w), HAlign::Left, 18., BOLD, tok.pal.primary_text));
        if !artist.is_empty() {
            g.add(text(&artist, 82., cy + th + 4., Some(col_w), HAlign::Left, 13., DEMIBOLD, tok.island_muted_text));
        }
        g.add(self.spectrum(82. + col_w + 12., vcenter(72., 56.), true, 1));
        // Progress track (y 86, h 5).
        let mut track = clip_group(Rect::new(0., 86., cw, 5.));
        track.add(qt_rect(Rect::new(0., 0., cw, 5.), 2.5, tok.island_soft_fill, None));
        if md.length_s > 0. {
            track.add(qt_rect(Rect::new(0., 0., self.a.fill_w.get(self.now) as f32, 5.), 2.5, tok.pal.accent, None));
        }
        g.add(track);
        // Time labels (y 96).
        let shown = self.displayed_position();
        let (left, right) = if md.length_s > 0. {
            (m::track_time(shown), format!("-{}", m::track_time((md.length_s - shown).max(0.))))
        } else {
            ("--:--".into(), "--:--".into())
        };
        g.add(text(&left, 0., 96., Some(cw / 2.), HAlign::Left, 9., BOLD, tok.island_muted_text));
        g.add(text(&right, cw / 2., 96., Some(cw / 2.), HAlign::Right, 9., BOLD, tok.island_muted_text));
        // Controls.
        let r = self.media_button_rects(d);
        // Preserve QML's Row transform before its children: merging translations
        // has a different float addition order when the content frame animates.
        let mut controls=group(Rect::new(r[0].x,r[1].y,160.,44.));
        for (index,(btn, rect)) in [(&self.prev_btn, Rect::new(0.,2.,40.,40.)), (&self.play_btn, Rect::new(58.,0.,44.,44.)), (&self.next_btn, Rect::new(120.,2.,40.,40.))].into_iter().enumerate() {
            let mut b = btn.clone();
            b.frame = rect;
            let mut button = b.build(tok, self.now);
            let scale=self.media_scales[index].value(self.now);
            // QQuickItem builds its centre-origin QTransform in qreal first.
            let mut transform=hyprv_ui::skia::Matrix::translate(((rect.width as f64/2.*(1.-scale))as f32,(rect.height as f64/2.*(1.-scale))as f32));
            transform.pre_scale((scale as f32,scale as f32),None);
            button.transform=transform;
            for child in &mut button.children {
                if matches!(child.content, Content::Text(_)) {
                    *child = centered_text(&b.glyph,0.,if b.primary {1.}else{0.},rect.width,rect.height,
                        if b.primary {21.}else{17.},BOLD,
                        if b.primary {Color::rgba(1.,1.,1.,1.)}else{tok.pal.primary_text});
                }
            }
            controls.add(button);
        }
        g.add(controls);
        g
    }

    fn ring(&self, x: f32, y: f32, size: f32, value: f64) -> Node {
        let mut r = ProgressRing::new(size, value);
        r.frame.x = x;
        r.frame.y = y;
        r.build(&self.tok, self.now)
    }

    /// `AgentLogo` (image inset 3, else the source's first letter, 12 px Black).
    fn logo(&self, x: f32, y: f32, size: f32, agent: Option<&Agent>) -> Node {
        let mut g = group(Rect::new(x, y, size, size));
        match agent.and_then(|a| crate::assets::agent_logo(&a.source)) {
            Some(img) => g.add_fit(img, Rect::new(3., 3., size - 6., size - 6.)),
            None => {
                g.add(centered_text(&m::logo_letter(agent), 0., 0., size, size, 12., BLACK, self.tok.pal.accent));
            }
        };
        g
    }

    fn compact_agent(&self, d: &Derived) -> Node {
        let cb = self.cagent_rect(d);
        let mut g = self.faded(cb, self.a.cagent_op.get(self.now));
        if !g.visible {
            return g;
        }
        let tok = &self.tok;
        let agent = self.primary_agent();
        let minimal = d.dual;
        let pill_w = if minimal { AGENT_MINIMAL_WIDTH } else { crate::island::AGENT_COMPACT_PILL_WIDTH };
        let own_bg = minimal && d.pfc;
        let mut pill = qt_rect(
            Rect::new(0., 0., pill_w, COLLAPSED_HEIGHT),
            19.,
            if own_bg { self.surface() } else { Color::rgba(0., 0., 0., 0.) },
            own_bg.then_some((1., tok.island_stroke)),
        );
        let clock = &self.inputs.now_hh_mm;
        let th = measure(clock, 16., BOLD).height;
        let progress = m::progress(agent);
        if minimal {
            pill.add(text(clock, 34., vcenter(38., th), Some(54.), HAlign::Center, 16., BOLD, tok.pal.primary_text));
            let mut slot = group(Rect::new(3., 0., 31., 38.));
            slot.add(self.ring(hc(31.) - hc(24.), vcenter(38., 24.), 24., progress));
            let glyph = agent.map_or("", m::minimal_glyph);
            if !glyph.is_empty() {
                slot.add(centered_text(glyph, 0., 0., 31., 38., 10., BLACK, tok.pal.primary_text));
            }
            pill.add(slot);
        } else {
            pill.add(self.logo(8., vcenter(38., 28.), 28., agent));
            pill.add(text(clock, 39., vcenter(38., th), Some(58.), HAlign::Center, 16., BOLD, tok.pal.primary_text));
            pill.add(self.ring(100., vcenter(38., 24.), 24., progress));
            if agent.is_some_and(m::is_done) {
                pill.add(centered_text("✓", 100., vcenter(38., 24.), 24., 24., 10., BLACK, tok.pal.primary_text));
            }
        }
        g.add(pill);
        if minimal {
            let x = pill_w + AGENT_DETACHED_GAP;
            let mut music = qt_rect(Rect::new(x, 0., MUSIC_DETACHED_WIDTH, COLLAPSED_HEIGHT), 19., self.surface(), Some((1., tok.island_stroke)));
            music.add(self.album_art(d, hc(38.) - hc(28.), hc(38.) - hc(28.), 28., 14.));
            g.add(music);
        }
        g
    }

    fn expanded_agent(&self, d: &Derived) -> Node {
        let b = self.eagent_rect(d);
        let mut g = self.faded(b, self.a.eagent_op.get(self.now));
        if !g.visible {
            return g;
        }
        let tok = &self.tok;
        let cw = b.width;
        let agent = self.primary_agent();
        // Header (h 58, spacing 12).
        g.add(self.logo(0., vcenter(58., 50.), 50., agent));
        let title = agent.map(m::display_title_agent).unwrap_or_else(|| "Agent".into());
        let status = agent.map(m::status_text).unwrap_or_else(|| "Working".into());
        let th = measure(&title, 18., BOLD).height;
        let sh = measure(&status, 12., DEMIBOLD).height;
        let cy = vcenter(58., th + 4. + sh);
        let col_w = cw - 132.;
        g.add(text(&title, 62., cy, Some(col_w), HAlign::Left, 18., BOLD, tok.pal.primary_text));
        g.add(text(&status, 62., cy + th + 4., Some(col_w), HAlign::Left, 12., DEMIBOLD, tok.island_muted_text));
        let rx = 62. + col_w + 12.;
        let ry = vcenter(58., 50.);
        g.add(self.ring(rx, ry, 50., m::progress(agent)));
        if agent.is_some_and(m::is_done) {
            g.add(centered_text("✓", rx, ry, 50., 50., 18., BLACK, tok.pal.primary_text));
        }
        // Session list.
        let l = self.list_rect(d);
        let mut list = clip_group(l);
        for (i, a) in self.inputs.agents.iter().enumerate() {
            let y = i as f32 * (ROW_HEIGHT + ROW_SPACING) - self.scroll_y;
            if y + ROW_HEIGHT <= 0. || y >= l.height {
                continue;
            }
            list.add(self.agent_row(a, l.width, y));
        }
        g.add(list);
        // Scroll indicator.
        let op = self.a.scroll_op.get(self.now);
        if op > 0. && d.agent_content_height > 0. {
            let content = d.agent_content_height;
            let y_pos = self.scroll_y / content;
            let ratio = l.height / content;
            let ih = 24f32.max(l.height * ratio);
            let mut ind = qt_rect(Rect::new(l.width - 4. - 3., l.y + y_pos * l.height, 3., ih), 2., tok.island_muted_text, None);
            ind.opacity = op as f32;
            g.add(ind);
        }
        g
    }

    fn agent_row(&self, a: &Agent, width: f32, y: f32) -> Node {
        let tok = &self.tok;
        let mut row = qt_rect(Rect::new(0., y, width, ROW_HEIGHT), 13., tok.island_soft_fill, Some((1., tok.island_stroke)));
        row.add(self.logo(8., vcenter(42., 23.), 23., Some(a)));
        let done = m::is_done(a);
        let col_w = (width - 16.) - 23. - 7. - if done { 25. } else { 0. };
        let title = m::display_title_agent(a);
        let status = m::status_text(a);
        let th = measure(&title, 11., BOLD).height;
        let sh = measure(&status, 9., DEMIBOLD).height;
        let cy = vcenter(42., th + 1. + sh);
        row.add(text(&title, 38., cy, Some(col_w), HAlign::Left, 11., BOLD, tok.pal.primary_text));
        row.add(text(&status, 38., cy + th + 1., Some(col_w), HAlign::Left, 9., DEMIBOLD, tok.island_muted_text));
        if done {
            row.add(centered_text("✓", 38. + col_w + 7., 0., 18., 42., 13., BLACK, tok.pal.accent));
        }
        row
    }
}

/// Image filling `w×h` with `PreserveAspectCrop` (centre crop; parent clips).
fn cover(img: Image, w: f32, h: f32) -> Node {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let s = (w / iw).max(h / ih);
    let (dw, dh) = (iw * s, ih * s);
    let mut n = Node::new(Rect::new((w - dw) / 2., (h - dh) / 2., dw, dh), Content::Image(img));
    n.image_sampling = hyprv_ui::image::ImageSampling::qt(true, true); // smooth; mipmap
    n
}

trait AddFit {
    fn add_fit(&mut self, img: Image, r: Rect);
}
impl AddFit for Node {
    /// `PreserveAspectFit` inside `r`.
    fn add_fit(&mut self, img: Image, r: Rect) {
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let s = (r.width / iw).min(r.height / ih);
        let (dw, dh) = (iw * s, ih * s);
        let mut n = Node::new(Rect::new(r.x + (r.width - dw) / 2., r.y + (r.height - dh) / 2., dw, dh), Content::Image(img));
        n.image_sampling = hyprv_ui::image::ImageSampling::qt(true, true); // DynamicIsland.qml:545-552 smooth; mipmap
        self.add(n);
    }
}


// Keep widget-created native labels on the same Qt GL path as island labels.
fn enable_native_text(node: &mut Node) {
    if matches!(&node.content, Content::Text(t) if t.render_type == hyprv_ui::scene::RenderType::NativeRendering) {
        node.native_text_mode = hyprv_ui::scene::NativeTextMode::QtGl;
    }
    for child in &mut node.children { enable_native_text(child); }
}
