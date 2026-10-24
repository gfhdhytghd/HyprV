//! Island inputs (ui-island.md §9) and the QML text/status helpers (§3.15).
use hyprv_svc_desktop::agent_island::{AgentIslandState, AgentSession};
use hyprv_svc_media::{mpris::MprisState, spectrum::SpectrumState};
use hyprv_ui::skia::Image;

/// `root.media*` properties (`shell.qml:166-173`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Media {
    pub available: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    /// MPRIS bus suffix (`mediaPlayerName`).
    pub player: String,
    /// `file://` or `http(s)://`; decoded by the host into [`Inputs::art`].
    pub art_url: String,
    pub position_s: f64,
    pub length_s: f64,
}

/// One `agentSessions[i]` entry as the island reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Agent {
    pub id: String,
    pub source: String,
    pub source_label: String,
    pub title: String,
    pub project: String,
    pub status: String,
    pub current_tool: String,
    pub detail: String,
    /// `Number(agent.progress)`; `None` (missing) → 0.35 like `NaN`.
    pub progress: Option<f64>,
}

/// Everything the island reads from its host (`shell.qml:3912-3958`).
#[derive(Clone, Default)]
pub struct Inputs {
    pub dark: bool,
    /// `Qt.formatTime(now, "hh:mm")`.
    pub now_hh_mm: String,
    pub media: Media,
    /// `audioSpectrumValues`: 16 values 0..100, empty while cava/spectrum is stopped.
    pub spectrum: Vec<f32>,
    /// Ordered; `[0]` is the primary agent.
    pub agents: Vec<Agent>,
    /// `calendarOpen` = host `calendarPanelPresented`.
    pub calendar_open: bool,
    /// Host `calendarProgress` (island opacity = 1 − progress).
    pub calendar_progress: f32,
    /// Decoded album art for `media.art_url` (None while loading/absent → U+F001 fallback).
    pub art: Option<Image>,
}
impl PartialEq for Inputs {
    fn eq(&self, o: &Self) -> bool {
        self.dark == o.dark
            && self.now_hh_mm == o.now_hh_mm
            && self.media == o.media
            && self.spectrum == o.spectrum
            && self.agents == o.agents
            && self.calendar_open == o.calendar_open
            && self.calendar_progress == o.calendar_progress
            && self.art.as_ref().map(|i| i.unique_id()) == o.art.as_ref().map(|i| i.unique_id())
    }
}

/// Typed actions (ui-island.md §1.4 signals). The host maps them to services:
/// `PlayPause/Previous/Next/Seek` → `mpris::MprisAction`, `FocusMediaApp` →
/// `MprisAction::Raise` (+ Hyprland fallback), `FocusAgent` → `AgentAction::FocusSession`,
/// `Lock`/`Power` → app activation of hyprlock / wlogout, `ToggleCalendar` → host.
#[derive(Clone, Debug, PartialEq)]
pub enum IslandAction {
    Lock,
    Power,
    ToggleCalendar,
    Previous,
    /// The host applies the optimistic `mediaPlaying = !mediaPlaying` (`shell.qml:3955`).
    PlayPause,
    Next,
    /// Absolute seconds, already clamped like `seekMedia`.
    Seek(f64),
    FocusMediaApp,
    FocusAgent(String),
}

impl Media {
    /// From the MPRIS topic. `position_s` must be the host's 1 Hz-stepped sample of
    /// `state.position_seconds(monotonic_us)` (legacy 1 s poll, ui-island.md §6.2).
    pub fn from_mpris(state: &MprisState, position_s: f64) -> Self {
        Self {
            available: state.available,
            playing: state.playing,
            title: state.title.clone(),
            artist: state.artist.clone(),
            player: state.player_name.clone(),
            art_url: state.art_url.clone(),
            position_s: position_s.max(0.),
            length_s: state.length_seconds.max(0.),
        }
    }
}

impl Agent {
    pub fn from_session(s: &AgentSession, now_epoch_s: f64) -> Self {
        Self {
            id: s.id.clone(),
            source: s.source.clone(),
            source_label: s.source_label.clone(),
            title: s.title.clone(),
            project: s.project.clone(),
            status: s.status.clone(),
            current_tool: s.current_tool.clone(),
            detail: s.detail.clone(),
            progress: Some(s.progress_at(now_epoch_s)),
        }
    }
}

/// `spectrumValues` from the spectrum topic (empty when not running).
pub fn spectrum_values(state: &SpectrumState) -> Vec<f32> {
    if state.running { state.values.iter().map(|v| if v.is_finite() { v.max(0.) as f32 } else { 0. }).collect() } else { Vec::new() }
}

pub fn agents(state: &AgentIslandState, now_epoch_s: f64) -> Vec<Agent> {
    state.sessions.iter().map(|s| Agent::from_session(s, now_epoch_s)).collect()
}

// ---------------------------------------------------------------- §3.15 helpers

/// `trackTime(s)`: `m:ss`, minutes unpadded; non-finite/negative → `0:00`.
pub fn track_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0. {
        return "0:00".into();
    }
    let s = seconds.floor() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}
pub fn display_title(m: &Media) -> String {
    if m.title.is_empty() { "Unknown track".into() } else { m.title.clone() }
}
pub fn is_done(a: &Agent) -> bool {
    matches!(a.status.to_lowercase().as_str(), "completed" | "complete" | "done" | "success" | "succeeded")
}
pub fn display_title_agent(a: &Agent) -> String {
    if !a.project.is_empty() && a.project != "Session" {
        a.project.clone()
    } else if a.source_label.is_empty() {
        // `agentField(a, "source_label", "Agent")`: the session snapshot always carries the
        // field, so an empty label stays empty; only a missing one would fall back.
        String::new()
    } else {
        a.source_label.clone()
    }
}
pub fn status_text(a: &Agent) -> String {
    match a.status.as_str() {
        "waitingApproval" => "Waiting approval".into(),
        "waitingQuestion" => "Waiting input".into(),
        "running" => if a.current_tool.is_empty() { "Running".into() } else { a.current_tool.clone() },
        "processing" => "Thinking".into(),
        _ if is_done(a) => "Done".into(),
        _ => a.detail.clone(),
    }
}
pub fn progress(a: Option<&Agent>) -> f64 {
    let Some(a) = a else { return 0. };
    if is_done(a) {
        return 1.;
    }
    match a.progress {
        Some(p) if p.is_finite() => p.clamp(0., 1.),
        _ => 0.35,
    }
}
pub fn minimal_glyph(a: &Agent) -> &'static str {
    match a.status.as_str() {
        "waitingApproval" => "!",
        "waitingQuestion" => "?",
        _ if is_done(a) => "✓",
        _ => "",
    }
}
/// `agentField(a, "source", "a").slice(0, 1).toUpperCase()`.
pub fn logo_letter(a: Option<&Agent>) -> String {
    let source = a.map_or("a", |a| a.source.as_str());
    source.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn agent(status: &str) -> Agent {
        Agent { status: status.into(), source_label: "Codex".into(), project: "Session".into(), ..Default::default() }
    }
    #[test]
    fn helpers_match_qml() {
        assert_eq!(track_time(3785.9), "63:05");
        assert_eq!(track_time(f64::NAN), "0:00");
        assert_eq!(track_time(-1.), "0:00");
        assert_eq!(display_title_agent(&agent("running")), "Codex");
        let mut a = agent("running");
        assert_eq!(status_text(&a), "Running");
        a.current_tool = "Bash".into();
        assert_eq!(status_text(&a), "Bash");
        assert_eq!(status_text(&agent("Completed")), "Done");
        assert_eq!(status_text(&agent("weird")), "");
        assert_eq!(progress(Some(&agent("done"))), 1.);
        assert_eq!(progress(Some(&agent("running"))), 0.35);
        assert_eq!(progress(None), 0.);
        assert_eq!(minimal_glyph(&agent("waitingQuestion")), "?");
        assert_eq!(logo_letter(None), "A");
        assert_eq!(logo_letter(Some(&Agent { source: "gemini".into(), ..Default::default() })), "G");
    }
}
