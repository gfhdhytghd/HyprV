//! UI theme state (`ui-state.sh`) and desktop-wide application
//! (`apply-ui-state.sh`, `toggle-theme.sh`, `set-variant.sh`).
//!
//! State lives in `$XDG_STATE_HOME/hyprv/quickshell/ui-state.env`, watched via
//! its parent directory and written atomically in the same two-line format.
//! Startup ensures/imports the state but never re-applies the desktop theme.
pub mod apply;
pub mod dconf;
pub mod files;
pub mod legacy;
pub mod ram_rgb;

use crate::{hypr::Hypr, topic_key, util, ActionResult, Ctx, Env, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    V1,
    #[default]
    V2,
    V3,
    Bt,
}
impl Variant {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "v1" => Some(Self::V1),
            "v2" => Some(Self::V2),
            "v3" => Some(Self::V3),
            "bt" => Some(Self::Bt),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
            Self::Bt => "bt",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::V1 => "HyprV1",
            Self::V2 => "HyprV2",
            Self::V3 => "HyprV3",
            Self::Bt => "HyprV Battery",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluentIconDirs {
    pub light: String,
    pub dark: String,
    pub base: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeState {
    /// Normalized (`load_state`) values.
    pub theme: Theme,
    pub variant: Variant,
    /// QML `darkMode`: the file literally contains a `theme=dark` line
    /// (`shell.qml:3225`), independent of normalization.
    pub dark_mode: bool,
    /// QML `fluentLightIconDir`/`fluentDarkIconDir`/`fluentBaseIconDir` (`shell.qml:2935`).
    pub fluent_icons: FluentIconDirs,
    pub busy: bool,
    /// Per-application partial failures of the last apply.
    pub application_errors: BTreeMap<String, ServiceError>,
}
topic_key!(ThemeTopic, ThemeState);

#[derive(Clone, Copy, Debug)]
pub enum ThemeAction {
    /// `toggle-theme.sh` (QML `toggleTheme`): flip, notify, apply.
    ToggleTheme,
    SetTheme(Theme),
    /// `set-variant.sh V`.
    SetVariant(Variant),
    /// `apply-ui-state.sh apply`.
    Apply,
}
pub type ThemeHandle = Handle<ThemeState, ThemeAction>;

pub fn state_file(env: &Env) -> PathBuf {
    env.state_home.join("hyprv/quickshell/ui-state.env")
}
fn readlink_f(p: &Path) -> String {
    std::fs::canonicalize(p)
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}
pub fn legacy_theme(env: &Env) -> Theme {
    if readlink_f(&env.home.join(".config/waybar/style.css")).ends_with("-dark.css") {
        Theme::Dark
    } else {
        Theme::Light
    }
}
pub fn legacy_variant(env: &Env) -> Variant {
    let config = readlink_f(&env.home.join(".config/waybar/config.jsonc"));
    let style = readlink_f(&env.home.join(".config/waybar/style.css"));
    for v in [Variant::Bt, Variant::V1, Variant::V2, Variant::V3] {
        let c = v.as_str();
        if config.ends_with(&format!("/{c}-config.jsonc")) || style.contains(&format!("/{c}-style"))
        {
            return v;
        }
    }
    Variant::V2
}
/// `ensure_state`: create from legacy waybar links when missing.
pub fn ensure_state(env: &Env) -> std::io::Result<()> {
    let path = state_file(env);
    if !path.exists() {
        write_state(&path, legacy_theme(env), legacy_variant(env))?;
    }
    Ok(())
}
pub fn write_state(path: &Path, theme: Theme, variant: Variant) -> std::io::Result<()> {
    util::atomic_write(
        path,
        format!("theme={}\nvariant={}\n", theme.as_str(), variant.as_str()).as_bytes(),
        None,
    )
}
pub fn load_state(env: &Env) -> (Theme, Variant, bool) {
    let _ = ensure_state(env);
    let text = util::read_string(&state_file(env)).unwrap_or_default();
    let dark_line = text.split('\n').any(|l| l.trim() == "theme=dark");
    let (t, v) = files::parse_ui_state(&text);
    let theme = t
        .as_deref()
        .and_then(Theme::parse)
        .unwrap_or_else(|| legacy_theme(env));
    let variant = v
        .as_deref()
        .and_then(Variant::parse)
        .unwrap_or_else(|| legacy_variant(env));
    (theme, variant, dark_line)
}
/// One startup scan, first match per base directory order.
pub fn fluent_icon_dirs(home: &Path) -> FluentIconDirs {
    let mut d = FluentIconDirs::default();
    for base in [
        home.join(".local/share/icons"),
        "/usr/local/share/icons".into(),
        PathBuf::from("/usr/share/icons"),
    ] {
        for (slot, name) in [
            (&mut d.light, "Fluent-light"),
            (&mut d.dark, "Fluent-dark"),
            (&mut d.base, "Fluent"),
        ] {
            if slot.is_empty() && base.join(name).is_dir() {
                *slot = base.join(name).display().to_string();
            }
        }
    }
    d
}

pub fn spawn(ctx: &Ctx) -> ThemeHandle {
    let topic = crate::topic::<ThemeTopic>(ctx, ThemeState::default());
    let (handle, mut rx) = Handle::<ThemeState, ThemeAction>::new(topic.clone());
    let env = ctx.env.clone();
    let hypr = Hypr::new(&ctx.env);
    let mut shutdown = ctx.shutdown.clone();
    // Desktop hardware (ENE DRAM sticks); the laptop has none to drive.
    let desktop = hyprv_core::profile::profile() == hyprv_core::profile::Profile::Desktop;
    let ram_rgb = ram_rgb::Config::from_env(&env)
        .filter(|_| desktop)
        .map(|cfg| ram_rgb::spawn(&ctx.rt, cfg, ctx.shutdown.clone()));
    ctx.rt.spawn(async move {
        let mut state = ThemeState {
            fluent_icons: fluent_icon_dirs(&env.home),
            ..Default::default()
        };
        let path = state_file(&env);
        // One long-lived watch; it re-arms itself after replaces.
        let watch =
            util::Watch::new(&[(path.parent().unwrap().to_path_buf(), &["ui-state.env"][..])]).ok();
        loop {
            let (t, v, dark) = load_state(&env);
            state.theme = t;
            state.variant = v;
            state.dark_mode = dark;
            topic.publish(state.clone());
            if let Some(tx) = &ram_rgb {
                ram_rgb::request(tx, t);
            }
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(30)).await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = changed => {}
                msg = rx.recv() => {
                    let Some(envl) = msg else { return };
                    state.busy = true;
                    topic.publish(state.clone());
                    let result = run(&env, &hypr, envl.action, &mut state, ram_rgb.as_ref()).await;
                    state.busy = false;
                    envl.respond(result);
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}

/// One-shot `set-variant.sh` / `apply-ui-state.sh` equivalent without a running actor, for
/// `hyprv-shell desktop setVariant` when no shell is reachable. It is the same code path as
/// the actor; a running shell's actor picks up the new state through its file watch.
pub async fn run_once(env: &Env, action: ThemeAction) -> ActionResult {
    let mut state = ThemeState::default();
    run(env, &Hypr::new(env), action, &mut state, None).await
}

async fn run(
    env: &Env,
    hypr: &Hypr,
    action: ThemeAction,
    state: &mut ThemeState,
    ram_rgb: Option<&tokio::sync::watch::Sender<Option<Theme>>>,
) -> ActionResult {
    let path = state_file(env);
    let (mut theme, mut variant, _) = load_state(env);
    let note = match action {
        ThemeAction::Apply => None,
        ThemeAction::ToggleTheme => {
            theme = if theme == Theme::Dark {
                Theme::Light
            } else {
                Theme::Dark
            };
            Some(theme.label())
        }
        ThemeAction::SetTheme(t) => {
            theme = t;
            Some(t.label())
        }
        ThemeAction::SetVariant(v) => {
            variant = v;
            Some(v.label())
        }
    };
    if note.is_some() {
        write_state(&path, theme, variant)?;
        // Reflect the new state immediately; inotify confirms shortly after.
        state.theme = theme;
        state.variant = variant;
        state.dark_mode = theme == Theme::Dark;
        // The LEDs follow the new theme now, not after the multi-second apply below.
        if let Some(tx) = ram_rgb {
            ram_rgb::request(tx, theme);
        }
    }
    if let Some(label) = note {
        util::notify(
            &format!("switching to {label}"),
            "",
            true,
            Some("sys-notify"),
        )
        .await;
    }
    state.application_errors = apply::apply(env, hypr, theme, variant).await;
    Ok(match action {
        ThemeAction::ToggleTheme => format!("{}\n", theme.as_str()),
        _ => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ensure_and_load_with_legacy_links() {
        let root = std::env::temp_dir().join(format!("hyprv-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let env = Env::rooted(&root);
        let waybar = env.home.join(".config/waybar");
        std::fs::create_dir_all(&waybar).unwrap();
        std::fs::write(waybar.join("v3-style-dark.css"), "").unwrap();
        std::os::unix::fs::symlink(waybar.join("v3-style-dark.css"), waybar.join("style.css"))
            .unwrap();
        let (t, v, dark) = load_state(&env);
        assert_eq!((t, v, dark), (Theme::Dark, Variant::V3, true));
        assert_eq!(
            std::fs::read_to_string(state_file(&env)).unwrap(),
            "theme=dark\nvariant=v3\n"
        );
        std::fs::write(state_file(&env), "theme=bogus\nvariant=bt\n").unwrap();
        assert_eq!(load_state(&env), (Theme::Dark, Variant::Bt, false));
        std::fs::remove_dir_all(root).unwrap();
    }
}
