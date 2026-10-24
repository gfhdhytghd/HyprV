//! Pure MPRIS model: player records, selection (`BarServices.qml:88`), the legacy
//! `updateMediaState` caching rule (`shell.qml:400`) and the Cider merge
//! (`media-status.sh:102-118`). No I/O here.
use crate::cider::{CiderTrack, Source};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Value};

pub const PREFIX: &str = "org.mpris.MediaPlayer2.";
pub const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback { Playing, Paused, #[default] Stopped }
impl Playback {
    pub fn parse(s: &str) -> Self {
        match s { "Playing" => Self::Playing, "Paused" => Self::Paused, _ => Self::Stopped }
    }
}

/// KDE mpris2-engine style local clock: position at `sampled_at_us` (CLOCK_MONOTONIC,
/// see [`crate::monotonic_us`]), advancing at `rate` only while playing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PositionAnchor {
    pub position_us: i64,
    pub sampled_at_us: u64,
    pub rate: f64,
    pub playing: bool,
    pub length_us: Option<i64>,
}
impl PositionAnchor {
    pub fn position_us_at(&self, now_us: u64) -> i64 {
        let mut p = self.position_us as f64;
        if self.playing {
            p += now_us.saturating_sub(self.sampled_at_us) as f64 * self.rate;
        }
        let mut p = p.max(0.0) as i64;
        if let Some(len) = self.length_us.filter(|l| *l > 0) {
            p = p.min(len);
        }
        p
    }
    pub fn seconds_at(&self, now_us: u64) -> f64 { self.position_us_at(now_us) as f64 / 1e6 }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub track_id: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub art_url: String,
    pub length_us: Option<i64>,
    pub url: String,
}
impl Metadata {
    /// Quickshell `trackArtist`.
    pub fn artist(&self) -> String { self.artists.join(", ") }
    pub fn has_content(&self) -> bool {
        !self.title.is_empty() || !self.artists.iter().all(|a| a.is_empty()) || !self.art_url.is_empty()
    }
    pub fn parse(map: &HashMap<String, OwnedValue>) -> Self {
        let string = |k: &str| map.get(k).and_then(|v| match &**v {
            Value::Str(s) => Some(s.to_string()),
            Value::ObjectPath(p) => Some(p.to_string()),
            _ => None,
        });
        let strings = |k: &str| map.get(k).map(|v| match &**v {
            Value::Str(s) => vec![s.to_string()],
            Value::Array(a) => a.iter().filter_map(|i| match i { Value::Str(s) => Some(s.to_string()), _ => None }).collect(),
            _ => Vec::new(),
        }).unwrap_or_default();
        let length = map.get("mpris:length").and_then(|v| match &**v {
            Value::I64(i) => Some(*i),
            Value::U64(u) => i64::try_from(*u).ok(),
            Value::I32(i) => Some(*i as i64),
            Value::U32(u) => Some(*u as i64),
            Value::F64(f) if f.is_finite() => Some(*f as i64),
            _ => None,
        }).filter(|l| *l > 0);
        Self {
            track_id: string("mpris:trackid").filter(|t| t != NO_TRACK && !t.is_empty()),
            title: string("xesam:title").unwrap_or_default(),
            artists: strings("xesam:artist"),
            album: string("xesam:album").unwrap_or_default(),
            art_url: string("mpris:artUrl").unwrap_or_default(),
            length_us: length,
            url: string("xesam:url").unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
    pub can_raise: bool,
}

/// One `org.mpris.MediaPlayer2.*` owner.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MprisPlayer {
    pub bus_name: String,
    /// `dbusName` without `org.mpris.MediaPlayer2.` (QML `playerName`).
    pub name: String,
    pub owner: String,
    pub pid: Option<u32>,
    /// `/proc/PID/comm` of the bus owner (legacy `busctl status` `Comm=`).
    pub comm: Option<String>,
    pub identity: String,
    pub desktop_entry: Option<String>,
    pub playback: Playback,
    pub rate: f64,
    pub metadata: Metadata,
    pub capabilities: Capabilities,
    pub position_supported: bool,
    pub position: PositionAnchor,
    /// Discovery order (Quickshell `Mpris.players` order).
    pub order: u64,
}
impl MprisPlayer {
    pub fn is_playing(&self) -> bool { self.playback == Playback::Playing }
    /// `media-status.sh:78`: Comm=Cider or a player name containing cider/Cider.
    pub fn is_cider(&self) -> bool {
        self.comm.as_deref() == Some("Cider") || self.name.contains("cider") || self.name.contains("Cider")
    }
    /// `BarServices.qml:155`: compatibility repair candidates.
    pub fn compat_candidate(&self) -> bool {
        let key = format!("{} {}", self.name, self.identity).to_lowercase();
        key.contains("chromium") || key.contains("cider")
    }
    /// `mediaSnapshot`: any change invalidates a Cider correction and re-triggers it.
    pub fn snapshot_key(&self) -> String {
        serde_json::json!({
            "name": self.name, "title": self.metadata.title, "artist": self.metadata.artist(),
            "art": self.metadata.art_url, "album": self.metadata.album,
            "length": self.metadata.length_us.unwrap_or(0), "playing": self.is_playing(),
            "identity": self.identity,
        }).to_string()
    }
    pub fn reanchor(&mut self, position_us: i64, now_us: u64) {
        self.position = PositionAnchor {
            position_us: position_us.max(0),
            sampled_at_us: now_us,
            rate: if self.rate.is_finite() && self.rate > 0.0 { self.rate } else { 1.0 },
            playing: self.is_playing(),
            length_us: self.metadata.length_us,
        };
    }
    /// Carry the extrapolated position into a new anchor when playback/rate/length change.
    pub fn rebase(&mut self, now_us: u64) {
        let current = self.position.position_us_at(now_us);
        self.reanchor(current, now_us);
    }
}

/// `BarServices.qml:88`: first playing, else paused-with-content, content, paused, first.
pub fn select(players: &[MprisPlayer]) -> Option<&MprisPlayer> {
    let mut ordered: Vec<&MprisPlayer> = players.iter().collect();
    ordered.sort_by_key(|p| p.order);
    let (mut paused_content, mut content, mut paused) = (None, None, None);
    for p in &ordered {
        if p.is_playing() { return Some(p) }
        let has = p.metadata.has_content();
        if has && content.is_none() { content = Some(*p) }
        if p.playback == Playback::Paused {
            if paused.is_none() { paused = Some(*p) }
            if has && paused_content.is_none() { paused_content = Some(*p) }
        }
    }
    paused_content.or(content).or(paused).or(ordered.first().copied())
}

/// Corrected track fields (`correctedMediaOutput`) valid for one snapshot key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub bus_name: String,
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub source: Option<Source>,
}

/// `media-status.sh:102-118`: API results replace non-empty fields; cache results only
/// repair a missing/generic title and fill empty artist/album/art.
pub fn merge_cider(player: &MprisPlayer, key: String, fallback: Option<&CiderTrack>) -> Correction {
    let mut c = Correction {
        bus_name: player.bus_name.clone(),
        key,
        title: player.metadata.title.clone(),
        artist: player.metadata.artist(),
        album: player.metadata.album.clone(),
        art_url: player.metadata.art_url.clone(),
        source: None,
    };
    let Some(f) = fallback else { return c };
    c.source = f.source;
    if f.source == Some(Source::Api) {
        if !f.title.is_empty() { c.title = f.title.clone() }
        if !f.artist.is_empty() { c.artist = f.artist.clone() }
        if !f.album.is_empty() { c.album = f.album.clone() }
        if !f.art_url.is_empty() { c.art_url = f.art_url.clone() }
    } else {
        if !f.title.is_empty() && (c.title.is_empty() || c.title == "Cider" || c.title == "Chromium") { c.title = f.title.clone() }
        if !f.artist.is_empty() && c.artist.is_empty() { c.artist = f.artist.clone() }
        if !f.album.is_empty() && c.album.is_empty() { c.album = f.album.clone() }
        if !f.art_url.is_empty() && c.art_url.is_empty() { c.art_url = f.art_url.clone() }
    }
    c
}

/// Flattened QML root media properties (`shell.qml:166-173`) plus all players.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MprisState {
    pub players: Vec<MprisPlayer>,
    pub selected: Option<String>,
    /// `mediaAvailable`
    pub available: bool,
    /// `mediaPlaying`
    pub playing: bool,
    /// `mediaTitle`
    pub title: String,
    /// `mediaArtist`
    pub artist: String,
    /// `mediaPlayerName`
    pub player_name: String,
    /// `mediaArtUrl`
    pub art_url: String,
    pub album: String,
    /// `mediaLengthSeconds`
    pub length_seconds: f64,
    /// `mediaPositionSeconds` = `position.seconds_at(monotonic_us())`, sampled by the UI
    /// only while it presents media (no service tick).
    pub position: PositionAnchor,
    /// `media-status.sh` `cider=` for the selected player.
    pub cider: bool,
    /// Source of the applied Cider correction, if any.
    pub correction: Option<Source>,
}
impl MprisState {
    pub fn position_seconds(&self, now_us: u64) -> f64 { self.position.seconds_at(now_us).max(0.0) }
    pub fn selected_player(&self) -> Option<&MprisPlayer> {
        let name = self.selected.as_deref()?;
        self.players.iter().find(|p| p.bus_name == name)
    }
}

/// Compute the next state. `previous` provides the legacy cached-content behavior.
pub fn flatten(players: &[MprisPlayer], correction: Option<&Correction>, previous: &MprisState, now_us: u64) -> MprisState {
    let mut sorted = players.to_vec();
    sorted.sort_by_key(|p| p.order);
    let Some(p) = select(&sorted).cloned() else {
        return MprisState { players: sorted, ..Default::default() };
    };
    let key = p.snapshot_key();
    let applied = correction.filter(|c| c.bus_name == p.bus_name && c.key == key);
    let (mut title, mut artist, mut art_url, album) = match applied {
        Some(c) => (c.title.clone(), c.artist.clone(), c.art_url.clone(), c.album.clone()),
        None => (p.metadata.title.clone(), p.metadata.artist(), p.metadata.art_url.clone(), p.metadata.album.clone()),
    };
    let mut length_seconds = p.metadata.length_us.map(|l| l as f64 / 1e6).unwrap_or(0.0);
    let mut position = if p.position_supported { p.position } else { PositionAnchor::default() };
    let playing = p.is_playing();
    let incoming = !title.is_empty() || !artist.is_empty() || !art_url.is_empty();
    let cached = !previous.title.is_empty() || !previous.artist.is_empty() || !previous.art_url.is_empty();
    if !playing && !incoming && !p.name.is_empty() && p.name == previous.player_name && cached {
        title = previous.title.clone();
        artist = previous.artist.clone();
        art_url = previous.art_url.clone();
        if length_seconds <= 0.0 && previous.length_seconds > 0.0 {
            length_seconds = previous.length_seconds;
        }
        if position.position_us_at(now_us) <= 0 && previous.position.position_us_at(now_us) > 0 {
            position = previous.position;
        }
    }
    MprisState {
        selected: Some(p.bus_name.clone()),
        available: true,
        playing,
        title,
        artist,
        player_name: p.name.clone(),
        art_url,
        album,
        length_seconds: length_seconds.max(0.0),
        position,
        cider: p.is_cider(),
        correction: applied.and_then(|c| c.source),
        players: sorted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player(name: &str, order: u64, playback: Playback, title: &str) -> MprisPlayer {
        MprisPlayer {
            bus_name: format!("{PREFIX}{name}"), name: name.into(), order, playback, rate: 1.0,
            metadata: Metadata { title: title.into(), length_us: Some(200_000_000), ..Default::default() },
            position_supported: true,
            ..Default::default()
        }
    }
    #[test]
    fn selection_priority() {
        let a = player("a", 0, Playback::Stopped, "");
        let b = player("b", 1, Playback::Paused, "");
        let c = player("c", 2, Playback::Stopped, "x");
        let d = player("d", 3, Playback::Paused, "y");
        let e = player("e", 4, Playback::Playing, "");
        assert_eq!(select(&[a.clone(), b.clone(), c.clone(), d.clone(), e.clone()]).unwrap().name, "e");
        assert_eq!(select(&[a.clone(), b.clone(), c.clone(), d.clone()]).unwrap().name, "d");
        assert_eq!(select(&[a.clone(), b.clone(), c.clone()]).unwrap().name, "c");
        assert_eq!(select(&[a.clone(), b.clone()]).unwrap().name, "b");
        assert_eq!(select(&[b.clone(), a.clone()]).unwrap().name, "b");
        assert_eq!(select(&[a]).unwrap().name, "a");
        assert!(select(&[]).is_none());
    }
    #[test]
    fn anchor_extrapolates_only_while_playing_and_clamps() {
        let mut p = player("a", 0, Playback::Playing, "t");
        p.rate = 2.0;
        p.reanchor(10_000_000, 1_000);
        assert_eq!(p.position.position_us_at(1_001_000), 12_000_000);
        assert_eq!(p.position.position_us_at(1_000_000_000), 200_000_000);
        p.playback = Playback::Paused;
        p.rebase(2_001_000);
        assert_eq!(p.position.position_us_at(9_000_000), 14_000_000);
        assert!(!p.position.playing);
        p.reanchor(-5, 0);
        assert_eq!(p.position.position_us_at(10), 0);
    }
    #[test]
    fn metadata_parsing() {
        let mut m = HashMap::new();
        m.insert("xesam:title".to_owned(), OwnedValue::try_from(Value::from("T")).unwrap());
        m.insert("xesam:artist".to_owned(), OwnedValue::try_from(Value::from(vec!["A", "B"])).unwrap());
        m.insert("mpris:length".to_owned(), OwnedValue::from(123_000_000u64));
        m.insert("mpris:trackid".to_owned(), OwnedValue::try_from(Value::from(zbus::zvariant::ObjectPath::try_from(NO_TRACK).unwrap())).unwrap());
        let md = Metadata::parse(&m);
        assert_eq!((md.title.as_str(), md.artist(), md.length_us, md.track_id.clone()), ("T", "A, B".to_owned(), Some(123_000_000), None));
        assert!(md.has_content());
        m.insert("mpris:length".to_owned(), OwnedValue::from(-1i64));
        assert_eq!(Metadata::parse(&m).length_us, None);
    }
    #[test]
    fn flatten_basic_and_cached_paused_content() {
        let mut p = player("spotify", 0, Playback::Playing, "Song");
        p.metadata.artists = vec!["Band".into()];
        p.reanchor(5_000_000, 0);
        let s1 = flatten(&[p.clone()], None, &MprisState::default(), 0);
        assert!(s1.available && s1.playing);
        assert_eq!((s1.title.as_str(), s1.artist.as_str(), s1.player_name.as_str(), s1.length_seconds), ("Song", "Band", "spotify", 200.0));
        assert_eq!(s1.position_seconds(1_000_000), 6.0);
        // Paused with metadata cleared: keep cached text, length and position.
        p.playback = Playback::Paused;
        p.metadata = Metadata::default();
        p.reanchor(0, 2_000_000);
        let s2 = flatten(&[p.clone()], None, &s1, 2_000_000);
        assert_eq!((s2.title.as_str(), s2.length_seconds, s2.playing), ("Song", 200.0, false));
        assert!(s2.position_seconds(2_000_000) > 0.0);
        // Different player: no carry-over.
        let mut q = p.clone();
        q.name = "other".into();
        q.bus_name = format!("{PREFIX}other");
        assert_eq!(flatten(&[q], None, &s1, 0).title, "");
        assert_eq!(flatten(&[], None, &s1, 0), MprisState::default());
    }
    #[test]
    fn cider_merge_rules_and_key_validity() {
        let mut p = player("chromium.instance42", 0, Playback::Playing, "Chromium");
        p.comm = Some("Cider".into());
        assert!(p.is_cider() && p.compat_candidate());
        let key = p.snapshot_key();
        let cache = CiderTrack { title: "Real".into(), artist: "Art".into(), album: "Alb".into(), art_url: "u".into(), source: Some(Source::Cache), ..Default::default() };
        let c = merge_cider(&p, key.clone(), Some(&cache));
        assert_eq!((c.title.as_str(), c.artist.as_str(), c.album.as_str(), c.art_url.as_str()), ("Real", "Art", "Alb", "u"));
        p.metadata.artists = vec!["Mine".into()];
        let c = merge_cider(&p, p.snapshot_key(), Some(&cache));
        assert_eq!(c.artist, "Mine");
        let api = CiderTrack { title: "Api".into(), artist: "".into(), source: Some(Source::Api), ..Default::default() };
        let c = merge_cider(&p, p.snapshot_key(), Some(&api));
        assert_eq!((c.title.as_str(), c.artist.as_str()), ("Api", "Mine"));
        let s = flatten(&[p.clone()], Some(&c), &MprisState::default(), 0);
        assert_eq!((s.title.as_str(), s.correction, s.cider), ("Api", Some(Source::Api), true));
        // A playback change alters the key: the correction no longer applies.
        p.playback = Playback::Paused;
        let s = flatten(&[p], Some(&c), &MprisState::default(), 0);
        assert_eq!((s.title.as_str(), s.correction), ("Chromium", None));
    }
}
