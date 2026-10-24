//! Bundled agent logos (copied from `quickshell/assets/agent/`), decoded once per thread.
use hyprv_ui::skia::{Data, Image};

static CODEX: &[u8] = include_bytes!("../assets/codex.png");
static CLAUDE: &[u8] = include_bytes!("../assets/claude.png");

thread_local! {
    static LOGOS: (Option<Image>, Option<Image>) = (
        Image::from_encoded(Data::new_copy(CODEX)),
        Image::from_encoded(Data::new_copy(CLAUDE)),
    );
}

/// `agentIconSource`: codex / claude logos, anything else has none (letter fallback).
pub fn agent_logo(source: &str) -> Option<Image> {
    LOGOS.with(|(codex, claude)| match source {
        "codex" => codex.clone(),
        "claude" => claude.clone(),
        _ => None,
    })
}
